//! On-device AI (ADR-035): catalog, decisions, duplicates and the job.
//!
//! Most tests need neither the model nor the media tools: embeddings are
//! made up, or taken from the built-in label vectors. Tests that run the
//! model skip when it is not installed (`scripts/setup.ps1 -WithAi`,
//! `scripts/setup.sh --with-ai`) or ONNX Runtime is missing.

mod common;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use image::{DynamicImage, Rgb, RgbImage};
use keepsake_core::ai::duplicates::Scope;
use keepsake_core::ai::labels::LIST_VERSION;
use keepsake_core::ai::{
    self, analyze, dhash, duplicates, install, Embedding, ImageModel, LabelSet, Purpose, JOB_AI,
    JOB_AI_TAG, MODEL_ID,
};
use keepsake_core::catalog::{
    AiAnalysis, AssetId, Catalog, DuplicateChoice, JobState, Priority, TagPath, TagSource,
    VolumeKind,
};
use keepsake_core::library::Library;
use keepsake_core::metadata::MetadataService;
use keepsake_core::rebuild::{add_volume, rebuild_catalog};
use keepsake_core::render::{
    Cache, RenderEvent, RenderOptions, RenderService, RenderTools, JOB_STILLS,
};

// ── Helpers ─────────────────────────────────────────────────────────────────

/// A synthetic photo: smooth bands whose layout depends on `seed`.
fn scene(seed: u32, w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        let fx = x as f32 / w as f32 * 6.0 + seed as f32;
        let fy = y as f32 / h as f32 * 4.0 * (1.0 + seed as f32 * 0.3);
        let r = ((fx.sin() * 0.5 + 0.5) * 255.0) as u8;
        let g = ((fy.cos() * 0.5 + 0.5) * 255.0) as u8;
        let b = (((fx + fy).sin() * 0.5 + 0.5) * 255.0) as u8;
        Rgb([r, g, b])
    }))
}

fn checker(w: u32, h: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(RgbImage::from_fn(w, h, |x, y| {
        if (x / 40 + y / 40) % 2 == 0 {
            Rgb([250, 250, 250])
        } else {
            Rgb([10, 10, 30])
        }
    }))
}

fn jpeg(img: &DynamicImage) -> Vec<u8> {
    keepsake_core::render::still::encode_jpeg(img).unwrap()
}

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: Catalog,
    media: PathBuf,
    cache: Cache,
}

impl World {
    /// A library with one folder of synthetic JPEGs, all taken on one day
    /// (EXIF-less files get their modification time; that is fine here, the
    /// tests set capture times directly).
    fn new(photos: &[(&str, DynamicImage)]) -> Self {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().to_path_buf();
        let lib = Library::create(&root.join("lib")).unwrap();
        let mut cat = lib.open_catalog().unwrap();
        let media = root.join("media");
        std::fs::create_dir_all(&media).unwrap();
        for (name, img) in photos {
            // JPEG bytes whatever the extension: the catalog goes by name
            // (an `.ARW` here is a RAW to it), and these tests never decode.
            let path = media.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, jpeg(img)).unwrap();
        }
        add_volume(
            &mut cat,
            &lib,
            "Media",
            VolumeKind::External,
            &media,
            // Read-only: sidecars go to the library's mirror, and the
            // media folder must stay byte-for-byte as it was.
            true,
            None,
        )
        .unwrap();
        let cache = Cache::new(root.join("cache"));
        Self {
            _tmp: tmp,
            lib,
            cat,
            media,
            cache,
        }
    }

    fn asset(&self, name: &str) -> AssetId {
        let path = std::fs::canonicalize(self.media.join(name)).unwrap();
        self.lib
            .locate(&self.cat, &path)
            .unwrap()
            .unwrap_or_else(|| panic!("{name} not catalogued"))
            .asset_id
    }

    /// Put every asset on 2026-07-04, `secs` apart in file-name order.
    fn same_day(&mut self, names: &[&str], secs: i64) {
        let ids: Vec<AssetId> = names.iter().map(|n| self.asset(n)).collect();
        let base = 1_783_159_200_000i64; // 2026-07-04T10:00:00Z
        self.cat
            .write(|tx| {
                for (i, id) in ids.iter().enumerate() {
                    let mut a = tx.assets().require(*id)?;
                    a.capture = Some(keepsake_core::catalog::CaptureTime {
                        utc_ms: base + i as i64 * secs * 1000,
                        utc_offset_min: Some(0),
                    });
                    tx.assets().update(&a)?;
                }
                Ok::<_, keepsake_core::catalog::CatalogError>(())
            })
            .unwrap();
    }

    fn ai_tags(&self, id: AssetId) -> Vec<String> {
        self.cat
            .read(|tx| tx.tags().for_asset(id))
            .unwrap()
            .into_iter()
            .filter(|t| t.source == TagSource::Ai)
            .map(|t| t.path.to_lightroom())
            .collect()
    }
}

fn label_vector(set: &LabelSet, name: &str) -> Embedding {
    set.labels()
        .iter()
        .find(|l| l.name == name)
        .unwrap()
        .vector()
        .clone()
}

fn tag(s: &str) -> TagPath {
    TagPath::parse_lightroom(s).unwrap()
}

/// The installed model, or `None` (the test then skips).
fn model(gpu: bool) -> Option<ImageModel> {
    if install::find_model().is_none() || install::find_runtime().is_none() {
        eprintln!("skipping: AI model or ONNX Runtime not installed");
        return None;
    }
    Some(ImageModel::load_installed(gpu).unwrap())
}

// ── Catalog ─────────────────────────────────────────────────────────────────

#[test]
fn analyses_are_stored_and_found_by_day() {
    let mut w = World::new(&[("a.jpg", scene(1, 64, 48)), ("b.jpg", scene(2, 64, 48))]);
    w.same_day(&["a.jpg", "b.jpg"], 60);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    let labels = LabelSet::builtin().unwrap();
    analyze::store_results(
        &mut w.cat,
        Some(&labels),
        a,
        &label_vector(&labels, "Dog"),
        0xABCD,
    )
    .unwrap();

    let got = w.cat.read(|tx| tx.ai().analysis(a)).unwrap().unwrap();
    assert_eq!(got.model, MODEL_ID);
    assert_eq!(got.dhash, 0xABCD);
    assert_eq!(got.embedding.len(), ai::embedding::DIM);
    let back = Embedding::from_quantized(&got.embedding, got.emb_scale).unwrap();
    assert!(back.cosine(&label_vector(&labels, "Dog")) > 0.999);

    let day = w
        .cat
        .read(|tx| tx.ai().day_items("2026-07-04", MODEL_ID, false))
        .unwrap();
    assert_eq!(day.len(), 1, "only analysed items");
    let counts = w.cat.read(|tx| tx.ai().counts(MODEL_ID, JOB_AI)).unwrap();
    assert_eq!((counts.analysed, counts.assets), (1, 2));

    // Tagging the day queues the untagged one; the tagged one is not.
    let st = analyze::enqueue_range(&mut w.cat, "2026-07-04", "2026-07-04", Purpose::Tags).unwrap();
    assert_eq!(st.pending, 1);
    let job = w
        .cat
        .read(|tx| tx.jobs().find(JOB_AI_TAG, b))
        .unwrap()
        .unwrap();
    assert_eq!(job.state, JobState::Queued);
    assert!(w
        .cat
        .read(|tx| tx.jobs().find(JOB_AI_TAG, a))
        .unwrap()
        .is_none());

    // AI tags are searchable like any other tag.
    assert!(w.ai_tags(a).contains(&"Dog".to_owned()));
    let hits = w.cat.read(|tx| tx.tags().assets_with(&tag("Dog"))).unwrap();
    assert_eq!(hits, vec![a]);
}

#[test]
fn a_removed_ai_tag_is_never_given_again() {
    let mut w = World::new(&[("a.jpg", scene(1, 64, 48))]);
    let a = w.asset("a.jpg");
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    analyze::store_results(&mut w.cat, Some(&labels), a, &dog, 1).unwrap();
    assert!(w.ai_tags(a).contains(&"Dog".to_owned()));

    MetadataService::new(&w.lib, &mut w.cat)
        .reject_ai_tag(a, &tag("Dog"))
        .unwrap();
    assert!(!w.ai_tags(a).contains(&"Dog".to_owned()));
    assert_eq!(
        w.cat.read(|tx| tx.ai().rejections(a)).unwrap(),
        vec![tag("Dog")]
    );

    // Analysing again (new model run, re-tag) leaves it out.
    analyze::store_results(&mut w.cat, Some(&labels), a, &dog, 1).unwrap();
    assert!(!w.ai_tags(a).contains(&"Dog".to_owned()));
    analyze::retag_all(&mut w.cat, &labels).unwrap();
    assert!(!w.ai_tags(a).contains(&"Dog".to_owned()));

    // The decision is in the sidecar.
    let xmp = common::xmp_files(&w.lib.sidecars_dir());
    assert_eq!(xmp.len(), 1);
    let text = std::fs::read_to_string(&xmp[0]).unwrap();
    assert!(text.contains("AITagsRejected"), "{text}");
}

#[test]
fn keeping_an_ai_tag_makes_it_the_users() {
    let mut w = World::new(&[("a.jpg", scene(1, 64, 48))]);
    let a = w.asset("a.jpg");
    let labels = LabelSet::builtin().unwrap();
    analyze::store_results(
        &mut w.cat,
        Some(&labels),
        a,
        &label_vector(&labels, "Beach"),
        1,
    )
    .unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .keep_ai_tag(a, &tag("Beach"))
        .unwrap();
    let user: Vec<String> = w
        .cat
        .read(|tx| tx.tags().for_asset(a))
        .unwrap()
        .into_iter()
        .filter(|t| t.source == TagSource::User)
        .map(|t| t.path.to_lightroom())
        .collect();
    assert_eq!(user, vec!["Beach"]);
}

#[test]
fn decisions_survive_a_rebuild_and_originals_are_untouched() {
    let mut w = World::new(&[("a.jpg", scene(1, 64, 48)), ("b.jpg", scene(2, 64, 48))]);
    let before = common::tree_hashes(&w.media);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    let labels = LabelSet::builtin().unwrap();
    analyze::store_results(
        &mut w.cat,
        Some(&labels),
        a,
        &label_vector(&labels, "Dog"),
        1,
    )
    .unwrap();
    {
        let mut svc = MetadataService::new(&w.lib, &mut w.cat);
        svc.set_duplicate_choice(a, Some(DuplicateChoice::Keeper))
            .unwrap();
        svc.set_duplicate_choice(b, Some(DuplicateChoice::NotKept))
            .unwrap();
        svc.reject_ai_tag(a, &tag("Dog")).unwrap();
    }
    let vol = w.cat.read(|tx| tx.volumes().list()).unwrap()[0].id;
    let snap = w.cat.read(|tx| tx.snapshot()).unwrap();
    let mine = |id: AssetId| {
        snap.assets
            .iter()
            .find(|s| s.asset.id == id)
            .unwrap()
            .clone()
    };
    assert_eq!(mine(a).duplicate_choice, Some(DuplicateChoice::Keeper));
    assert_eq!(mine(a).ai_rejected, vec![tag("Dog")]);

    drop(std::mem::replace(
        &mut w.cat,
        Catalog::open_in_memory().unwrap(),
    ));
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    w.cat = w.lib.open_catalog().unwrap();
    rebuild_catalog(&mut w.cat, &w.lib, vol, &w.media).unwrap();
    let again = |id: AssetId| {
        (
            w.cat.read(|tx| tx.ai().choice(id)).unwrap(),
            w.cat.read(|tx| tx.ai().rejections(id)).unwrap(),
        )
    };
    assert_eq!(again(a), (Some(DuplicateChoice::Keeper), vec![tag("Dog")]));
    assert_eq!(again(b), (Some(DuplicateChoice::NotKept), vec![]));
    assert_eq!(common::tree_hashes(&w.media), before, "originals unchanged");

    // Clearing a choice clears it in the sidecar too.
    MetadataService::new(&w.lib, &mut w.cat)
        .set_duplicate_choice(b, None)
        .unwrap();
    assert_eq!(w.cat.read(|tx| tx.ai().choice(b)).unwrap(), None);
}

// ── Duplicates ──────────────────────────────────────────────────────────────

fn put(cat: &mut Catalog, id: AssetId, emb: &Embedding, hash: u64) {
    let (embedding, emb_scale) = emb.quantize();
    cat.write(|tx| {
        tx.ai().put_analysis(&AiAnalysis {
            asset_id: id,
            model: MODEL_ID.into(),
            embedding,
            emb_scale,
            dhash: hash,
            made_ms: 0,
            tag_list: 0,
        })
    })
    .unwrap();
}

#[test]
fn same_day_copies_are_grouped_and_the_choice_is_shown() {
    let names = ["1.jpg", "2.jpg", "3.jpg", "4.jpg"];
    let imgs: Vec<(&str, DynamicImage)> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (*n, scene(i as u32, 64, 48)))
        .collect();
    let mut w = World::new(&imgs);
    w.same_day(&names, 600);
    let ids: Vec<AssetId> = names.iter().map(|n| w.asset(n)).collect();
    let labels = LabelSet::builtin().unwrap();
    // 1 and 2: the same shot (same vector). 3: same picture by hash only.
    // 4: something else.
    let dog = label_vector(&labels, "Dog");
    put(&mut w.cat, ids[0], &dog, 0x00FF_00FF);
    put(&mut w.cat, ids[1], &dog, 0xFF00_FF00_0000);
    put(
        &mut w.cat,
        ids[2],
        &label_vector(&labels, "Pizza"),
        0x00FF_00FE,
    );
    put(
        &mut w.cat,
        ids[3],
        &label_vector(&labels, "Mountain"),
        0x1234_5678_0000_0000,
    );

    let groups = duplicates::day_groups_of(&w.cat, "2026-07-04", Scope::All).unwrap();
    assert_eq!(groups.len(), 1);
    let members: Vec<AssetId> = groups[0].members.iter().map(|m| m.asset_id).collect();
    assert_eq!(members, ids[..3].to_vec());
    assert_eq!(groups[0].keeper, ids[0], "earliest when nothing is rated");
    assert!(!groups[0].decided);
    assert!(groups[0].members[2].exact);

    MetadataService::new(&w.lib, &mut w.cat)
        .set_duplicate_choice(ids[1], Some(DuplicateChoice::Keeper))
        .unwrap();
    let groups = duplicates::day_groups_of(&w.cat, "2026-07-04", Scope::All).unwrap();
    assert_eq!(groups[0].keeper, ids[1]);
    assert!(groups[0].decided);

    let sum = duplicates::summary_of(&w.cat, None, None, Scope::All).unwrap();
    assert_eq!((sum.groups, sum.items, sum.undecided), (1, 3, 0));
    assert_eq!(sum.days[0].date, "2026-07-04");

    // "Not duplicates" takes an item out of the group.
    MetadataService::new(&w.lib, &mut w.cat)
        .set_duplicate_choice(ids[2], Some(DuplicateChoice::NotDuplicate))
        .unwrap();
    let groups = duplicates::day_groups_of(&w.cat, "2026-07-04", Scope::All).unwrap();
    assert_eq!(groups[0].members.len(), 2);
}

#[test]
fn only_raw_photos_are_duplicates_and_an_edited_jpeg_is_not_one() {
    let names = ["DSC001.ARW", "DSC002.ARW", "DSC003.JPG", "Edits/DSC001.JPG"];
    let imgs: Vec<(&str, DynamicImage)> = names.iter().map(|n| (*n, scene(1, 64, 48))).collect();
    let mut w = World::new(&imgs);
    w.same_day(&names, 20);
    let ids: Vec<AssetId> = names.iter().map(|n| w.asset(n)).collect();
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    // All four are the same picture.
    for id in &ids {
        put(&mut w.cat, *id, &dog, 7);
    }
    // Every photo: one group of four.
    let all = duplicates::day_groups_of(&w.cat, "2026-07-04", Scope::All).unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].members.len(), 4);
    // What the app shows: the two RAWs only; the JPEG-only shot and the
    // edited copy named like the RAW are left out.
    let groups = duplicates::day_groups(&w.cat, "2026-07-04").unwrap();
    assert_eq!(groups.len(), 1);
    let members: Vec<AssetId> = groups[0].members.iter().map(|m| m.asset_id).collect();
    assert_eq!(members, ids[..2].to_vec());
    let sum = duplicates::summary_in(&w.cat, None, None).unwrap();
    assert_eq!((sum.groups, sum.items), (1, 2));
}

#[test]
fn different_days_never_group() {
    let mut w = World::new(&[("a.jpg", scene(1, 64, 48)), ("b.jpg", scene(1, 64, 48))]);
    w.same_day(&["a.jpg", "b.jpg"], 86_400); // one day apart
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    for n in ["a.jpg", "b.jpg"] {
        let id = w.asset(n);
        put(&mut w.cat, id, &dog, 7);
    }
    assert_eq!(
        duplicates::summary_of(&w.cat, None, None, Scope::All)
            .unwrap()
            .groups,
        0
    );
}

// ── Asking for a date range ─────────────────────────────────────────────────

fn find_job(cat: &Catalog, kind: &str, id: AssetId) -> Option<keepsake_core::catalog::Job> {
    cat.read(|tx| tx.jobs().find(kind, id)).unwrap()
}

#[test]
fn only_the_dates_asked_for_are_queued_ahead_of_the_backlog() {
    let names = ["a.jpg", "b.jpg", "c.jpg"];
    let imgs: Vec<(&str, DynamicImage)> = names.iter().map(|n| (*n, scene(1, 64, 48))).collect();
    let mut w = World::new(&imgs);
    // a and b on 2026-07-04, c a day later.
    w.same_day(&names, 43_200);
    let ids: Vec<AssetId> = names.iter().map(|n| w.asset(n)).collect();
    let labels = LabelSet::builtin().unwrap();
    analyze::store_results(
        &mut w.cat,
        Some(&labels),
        ids[0],
        &label_vector(&labels, "Dog"),
        1,
    )
    .unwrap();

    let st = analyze::enqueue_range(&mut w.cat, "2026-07-04", "2026-07-04", Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (2, 1, 1));
    // b: tagging and its thumbnail queued, ahead of the backlog; no proxy.
    let b = find_job(&w.cat, JOB_AI_TAG, ids[1]).unwrap();
    assert_eq!((b.state, b.priority), (JobState::Queued, Priority::Recent));
    assert_eq!(
        find_job(&w.cat, JOB_STILLS, ids[1]).unwrap().priority,
        Priority::Recent
    );
    // a is already tagged; c is outside the range; nothing for duplicates.
    assert!(find_job(&w.cat, JOB_AI_TAG, ids[0]).is_none());
    assert!(find_job(&w.cat, JOB_AI_TAG, ids[2]).is_none());
    assert!(find_job(&w.cat, JOB_AI, ids[1]).is_none());

    // A month covers both days.
    let st = analyze::range_status(&w.cat, "2026-07-01", "2026-07-31", Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (3, 1, 1));
    // Finding duplicates covers RAW photos only: none here.
    let st =
        analyze::range_status(&w.cat, "2026-07-01", "2026-07-31", Purpose::Duplicates).unwrap();
    assert_eq!(st.items, 0);
    for bad in [("2026-07-31", "2026-07-01"), ("July", "2026-07-01")] {
        assert!(analyze::enqueue_range(&mut w.cat, bad.0, bad.1, Purpose::Tags).is_err());
    }

    // Background AI jobs can be dropped; asked-for ones stay.
    analyze::enqueue(&mut w.cat, &ids[2..], Priority::Backlog, false).unwrap();
    let dropped = w
        .cat
        .write(|tx| tx.jobs().remove_queued(JOB_AI, Priority::Backlog))
        .unwrap();
    assert_eq!(dropped, 1);
    assert!(find_job(&w.cat, JOB_AI_TAG, ids[1]).is_some());
}

#[test]
fn finding_duplicates_and_tagging_are_separate() {
    let names = ["DSC001.ARW", "DSC002.ARW", "DSC003.JPG"];
    let imgs: Vec<(&str, DynamicImage)> = names.iter().map(|n| (*n, scene(1, 64, 48))).collect();
    let mut w = World::new(&imgs);
    w.same_day(&names, 20);
    let ids: Vec<AssetId> = names.iter().map(|n| w.asset(n)).collect();
    let day = "2026-07-04";

    // Find duplicates: the two RAWs, analysed only (no tag jobs).
    let st = analyze::enqueue_range(&mut w.cat, day, day, Purpose::Duplicates).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (2, 0, 2));
    assert!(find_job(&w.cat, JOB_AI, ids[0]).is_some());
    assert!(find_job(&w.cat, JOB_AI, ids[1]).is_some());
    assert!(
        find_job(&w.cat, JOB_AI, ids[2]).is_none(),
        "a JPEG is not compared"
    );
    assert!(ids
        .iter()
        .all(|id| find_job(&w.cat, JOB_AI_TAG, *id).is_none()));

    // Their analysis is stored without tags.
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    for id in &ids[..2] {
        analyze::store_results(&mut w.cat, None, *id, &dog, 7).unwrap();
    }
    assert!(w.ai_tags(ids[0]).is_empty());
    assert_eq!(
        w.cat
            .read(|tx| tx.ai().analysis(ids[0]))
            .unwrap()
            .unwrap()
            .tag_list,
        0
    );
    let st = analyze::range_status(&w.cat, day, day, Purpose::Duplicates).unwrap();
    assert_eq!(st.analysed, 2);

    // Tag photos: all three; the analysed RAWs need no new thumbnail.
    let thumbs_before: Vec<bool> = ids
        .iter()
        .map(|id| find_job(&w.cat, JOB_STILLS, *id).is_some())
        .collect();
    let st = analyze::enqueue_range(&mut w.cat, day, day, Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed), (3, 0));
    assert!(ids
        .iter()
        .all(|id| find_job(&w.cat, JOB_AI_TAG, *id).is_some()));
    assert!(
        thumbs_before[0] && thumbs_before[1],
        "queued by Find duplicates"
    );
    assert!(
        find_job(&w.cat, JOB_STILLS, ids[2]).is_some(),
        "the JPEG needs one"
    );

    // Tagging an analysed photo keeps it analysed for duplicates, now tagged.
    analyze::store_results(&mut w.cat, Some(&labels), ids[0], &dog, 7).unwrap();
    assert!(w.ai_tags(ids[0]).contains(&"Dog".to_owned()));
    // Finding duplicates again leaves the tags alone.
    analyze::store_results(&mut w.cat, None, ids[0], &dog, 7).unwrap();
    assert!(w.ai_tags(ids[0]).contains(&"Dog".to_owned()));
    assert_eq!(
        w.cat
            .read(|tx| tx.ai().analysis(ids[0]))
            .unwrap()
            .unwrap()
            .tag_list,
        LIST_VERSION
    );
    let st = analyze::range_status(&w.cat, day, day, Purpose::Tags).unwrap();
    assert_eq!(st.analysed, 1);
}

#[test]
fn photos_tagged_with_an_older_tag_list_are_tagged_again() {
    let mut w = World::new(&[("a.jpg", scene(1, 640, 480)), ("b.jpg", scene(2, 640, 480))]);
    let (a, b) = (w.asset("a.jpg"), w.asset("b.jpg"));
    w.same_day(&["a.jpg", "b.jpg"], 60);
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    for id in [a, b] {
        analyze::store_results(&mut w.cat, Some(&labels), id, &dog, 7).unwrap();
    }
    // a was tagged by an older build (list 1).
    w.cat
        .write(|tx| tx.ai().set_tag_list(a, LIST_VERSION - 1))
        .unwrap();
    let day = "2026-07-04";
    let st = analyze::range_status(&w.cat, day, day, Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed), (2, 1));
    analyze::enqueue_range(&mut w.cat, day, day, Purpose::Tags).unwrap();
    assert!(find_job(&w.cat, JOB_AI_TAG, a).is_some());
    assert!(find_job(&w.cat, JOB_AI_TAG, b).is_none());
    // It has an analysis, so it needs no thumbnail.
    assert!(find_job(&w.cat, JOB_STILLS, a).is_none());
    // "keepsake ai retag" brings every tagged photo up to date.
    assert_eq!(analyze::retag_all(&mut w.cat, &labels).unwrap(), 2);
    let st = analyze::range_status(&w.cat, day, day, Purpose::Tags).unwrap();
    assert_eq!(st.analysed, 2);
}

#[test]
fn the_summary_can_be_limited_to_dates() {
    let names = ["a.jpg", "b.jpg", "c.jpg", "d.jpg"];
    let imgs: Vec<(&str, DynamicImage)> = names.iter().map(|n| (*n, scene(1, 64, 48))).collect();
    let mut w = World::new(&imgs);
    // Two per day: 07-04 and 07-05.
    w.same_day(&names, 30_000);
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    for n in names {
        let id = w.asset(n);
        put(&mut w.cat, id, &dog, 7);
    }
    assert_eq!(
        duplicates::summary_of(&w.cat, None, None, Scope::All)
            .unwrap()
            .days
            .len(),
        2
    );
    let one =
        duplicates::summary_of(&w.cat, Some("2026-07-05"), Some("2026-07-05"), Scope::All).unwrap();
    assert_eq!(one.days.len(), 1);
    assert_eq!(one.days[0].date, "2026-07-05");
}

// ── With the model ──────────────────────────────────────────────────────────

#[test]
fn the_model_sees_a_reencoded_copy_as_the_same_picture() {
    let Some(mut m) = model(false) else { return };
    let a = scene(3, 640, 480);
    let copy = image::load_from_memory(&jpeg(&scene(3, 320, 240))).unwrap();
    let other = checker(640, 480);
    let (ea, ec, eo) = (
        m.embed(&a).unwrap(),
        m.embed(&copy).unwrap(),
        m.embed(&other).unwrap(),
    );
    assert_eq!(ea.len(), ai::embedding::DIM);
    assert!((ea.cosine(&ea) - 1.0).abs() < 1e-4);
    let same = ea.cosine(&ec);
    let diff = ea.cosine(&eo);
    assert!(same >= duplicates::COSINE_SAME, "copy cosine {same}");
    assert!(diff < duplicates::COSINE_BURST, "other cosine {diff}");
    assert!(
        dhash::distance(dhash::dhash(&a).unwrap(), dhash::dhash(&copy).unwrap()) <= dhash::NEAR
    );
}

#[test]
fn gpu_and_cpu_agree() {
    let Some(mut cpu) = model(false) else { return };
    let Some(mut gpu) = model(true) else { return };
    let img = scene(5, 400, 300);
    let c = cpu.embed(&img).unwrap().cosine(&gpu.embed(&img).unwrap());
    assert!(c > 0.99, "{c} ({:?})", gpu.accel());
}

#[test]
fn the_ai_worker_tags_items_after_their_thumbnails() {
    let Some(_) = model(false) else { return };
    let Some((t, ffmpeg)) = common::media::tools() else {
        eprintln!("skipping: media tools not installed");
        return;
    };
    let mut w = World::new(&[
        ("a.jpg", scene(1, 800, 600)),
        ("b.jpg", scene(1, 800, 600)),
        ("c.jpg", checker(800, 600)),
    ]);
    w.same_day(&["a.jpg", "b.jpg", "c.jpg"], 30);
    let before = common::tree_hashes(&w.media);
    let ids: Vec<AssetId> = ["a.jpg", "b.jpg", "c.jpg"]
        .iter()
        .map(|n| w.asset(n))
        .collect();
    keepsake_core::render::enqueue_backlog(&mut w.cat).unwrap();
    // A background AI job (an earlier build's sweep) is dropped by the
    // worker; the day asked for is analysed.
    ai::analyze::enqueue(&mut w.cat, &ids[..1], Priority::Backlog, false).unwrap();
    let st =
        ai::analyze::enqueue_range(&mut w.cat, "2026-07-04", "2026-07-04", Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (3, 0, 3));
    let tools = RenderTools {
        exiftool: t.exiftool,
        ffprobe: t.ffprobe,
        ffmpeg,
        dcraw: PathBuf::from("none"),
    };
    let opts = RenderOptions {
        workers: 1,
        proxy_workers: 0,
        max_bytes: u64::MAX,
        pause_on_battery: false,
        until_empty: true,
        forced_encoder: None,
        evict_every: 25,
        ai: true,
        ai_gpu: false,
    };
    let svc = RenderService::new(&w.lib, tools, w.cache.clone(), opts);
    let cancel = AtomicBool::new(false);
    let mut evs = Vec::new();
    svc.run(&cancel, &mut |e| evs.push(e.clone())).unwrap();
    assert!(
        evs.iter().any(|e| matches!(e, RenderEvent::AiReady { .. })),
        "{evs:?}"
    );
    for id in &ids {
        let job = w
            .cat
            .read(|tx| tx.jobs().find(JOB_AI_TAG, *id))
            .unwrap()
            .unwrap();
        assert_eq!(job.state, JobState::Done, "{job:?} {evs:?}");
        let a = w.cat.read(|tx| tx.ai().analysis(*id)).unwrap().unwrap();
        assert_eq!(a.tag_list, LIST_VERSION);
    }
    // The stale background job was dropped, not run.
    assert!(find_job(&w.cat, JOB_AI, ids[0]).is_none());
    let groups = duplicates::day_groups_of(&w.cat, "2026-07-04", Scope::All).unwrap();
    assert_eq!(groups.len(), 1, "{groups:?}");
    let members: Vec<AssetId> = groups[0].members.iter().map(|m| m.asset_id).collect();
    assert_eq!(members, ids[..2].to_vec());
    let st = ai::analyze::range_status(&w.cat, "2026-07-04", "2026-07-04", Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (3, 3, 0));
    assert_eq!(common::tree_hashes(&w.media), before, "originals unchanged");
}

#[test]
fn a_folder_and_the_folders_in_it_can_be_tagged_and_stopped() {
    let mut w = World::new(&[
        ("2025/May/a.jpg", scene(1, 640, 480)),
        ("2025/May/Trip/b.jpg", scene(2, 640, 480)),
        ("2025/June/c.jpg", scene(3, 640, 480)),
    ]);
    let (a, b, c) = (
        w.asset("2025/May/a.jpg"),
        w.asset("2025/May/Trip/b.jpg"),
        w.asset("2025/June/c.jpg"),
    );
    let volume = w.cat.read(|tx| tx.volumes().list()).unwrap()[0].id;
    let may = analyze::Target::Folder {
        volume,
        folder: "2025/May".into(),
    };
    // Tag "2025/May": a and the one in May/Trip, not June.
    let st = analyze::enqueue_target(&mut w.cat, &may, Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (2, 0, 2));
    assert!(find_job(&w.cat, JOB_AI_TAG, a).is_some());
    assert!(find_job(&w.cat, JOB_AI_TAG, b).is_some());
    assert!(find_job(&w.cat, JOB_AI_TAG, c).is_none());
    // One is done; then Stop drops the rest, and what was done stays.
    let labels = LabelSet::builtin().unwrap();
    let dog = label_vector(&labels, "Dog");
    analyze::store_results(&mut w.cat, Some(&labels), a, &dog, 7).unwrap();
    w.cat
        .write(|tx| {
            let j = tx.jobs().find(JOB_AI_TAG, a)?.unwrap();
            tx.jobs().complete(j.id, 0)
        })
        .unwrap();
    let st = analyze::target_status(&w.cat, &may, Purpose::Tags).unwrap();
    assert_eq!((st.items, st.analysed, st.pending), (2, 1, 1));
    assert_eq!(
        analyze::stop_target(&mut w.cat, &may, Purpose::Tags).unwrap(),
        1
    );
    let st = analyze::target_status(&w.cat, &may, Purpose::Tags).unwrap();
    assert_eq!((st.analysed, st.pending), (1, 0));
    assert!(w.ai_tags(a).contains(&"Dog".to_owned()));
    // The whole volume ("") has all three.
    let all = analyze::Target::Folder {
        volume,
        folder: String::new(),
    };
    let st = analyze::target_status(&w.cat, &all, Purpose::Tags).unwrap();
    assert_eq!(st.items, 3);
}

// ── Tag with AI (ADR-036, ADR-038) ─────────────────────────────────────────

use keepsake_core::ai::Provider;

/// Stands in for the AI providers: the answer is fixed; what was sent (and
/// to whom) is kept.
struct FakeCloud {
    answer: Result<ai::cloud::CloudTags, (String, bool)>,
    sent: std::sync::Mutex<Vec<(Provider, usize, usize)>>,
}

impl ai::cloud::Tagger for FakeCloud {
    fn tag(
        &self,
        provider: Provider,
        jpeg: &[u8],
        names: &[String],
    ) -> Result<ai::cloud::CloudTags, ai::AiError> {
        self.sent
            .lock()
            .unwrap()
            .push((provider, jpeg.len(), names.len()));
        assert_eq!(&jpeg[..2], &[0xff, 0xd8], "a JPEG is sent");
        self.answer
            .clone()
            .map_err(|(message, retry)| ai::AiError::Cloud { message, retry })
    }
}

fn cloud_service(w: &World, tagger: std::sync::Arc<FakeCloud>) -> RenderService {
    let tools = RenderTools {
        exiftool: PathBuf::from("none"),
        ffprobe: PathBuf::from("none"),
        ffmpeg: PathBuf::from("none"),
        dcraw: PathBuf::from("none"),
    };
    let opts = RenderOptions {
        workers: 1,
        proxy_workers: 0,
        max_bytes: u64::MAX,
        pause_on_battery: false,
        until_empty: true,
        forced_encoder: None,
        evict_every: 25,
        ai: false,
        ai_gpu: false,
    };
    RenderService::new(&w.lib, tools, w.cache.clone(), opts).with_cloud(tagger)
}

fn keywords(w: &mut World, id: AssetId) -> Vec<String> {
    let mut k: Vec<String> = MetadataService::new(&w.lib, &mut w.cat)
        .user_tags(id)
        .unwrap()
        .iter()
        .map(TagPath::to_lightroom)
        .collect();
    k.sort();
    k
}

#[test]
fn cloud_tags_become_keywords_from_the_thumbnail_only() {
    let mut w = World::new(&[("a.jpg", scene(1, 800, 600)), ("b.jpg", scene(2, 800, 600))]);
    let before = common::tree_hashes(&w.media);
    let a = w.asset("a.jpg");
    let b = w.asset("b.jpg");
    // a has a thumbnail and a keyword of its own; b has no thumbnail yet.
    let hash = keepsake_core::render::source::source_hash(&w.cat, a)
        .unwrap()
        .unwrap();
    w.cache
        .put_bytes(
            &mut w.cat,
            &hash,
            keepsake_core::render::Variant::Thumb1024,
            &jpeg(&scene(1, 1024, 768)),
        )
        .unwrap();
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(a, &[tag("Holiday")])
        .unwrap();
    assert_eq!(
        ai::cloud::enqueue(&mut w.cat, Provider::Claude, &[a, b]).unwrap(),
        2
    );
    let (st, _) = ai::cloud::status(&w.cat, Provider::Claude, &[a, b]).unwrap();
    assert_eq!((st.items, st.done, st.pending), (2, 0, 2));

    let fake = std::sync::Arc::new(FakeCloud {
        answer: Ok(ai::cloud::CloudTags {
            tags: vec!["Portrait".into(), "Beach".into()],
            new_tags: vec!["Sand Castle".into()],
            model: "claude-opus-5-5".into(),
        }),
        sent: Default::default(),
    });
    let svc = cloud_service(&w, fake.clone());
    svc.run(&AtomicBool::new(false), &mut |_| {}).unwrap();

    assert_eq!(
        keywords(&mut w, a),
        ["AI|Sand Castle", "Beach", "Holiday", "People", "Portrait"]
    );
    let sent = fake.sent.lock().unwrap().clone();
    assert_eq!(sent.len(), 1, "only the photo with a thumbnail so far");
    assert_eq!(sent[0].0, Provider::Claude);
    assert!(sent[0].2 > 100, "the whole tag list is sent");
    // b waits for its thumbnail, which was queued.
    let (st, problem) = ai::cloud::status(&w.cat, Provider::Claude, &[a, b]).unwrap();
    assert_eq!((st.done, st.pending, st.failed), (1, 1, 0), "{problem:?}");
    assert!(find_job(&w.cat, JOB_STILLS, b).is_some());
    // The keywords are in a's sidecar too, so they survive a rebuild.
    let vol = w.cat.read(|tx| tx.volumes().list()).unwrap()[0].id;
    drop(std::mem::replace(
        &mut w.cat,
        Catalog::open_in_memory().unwrap(),
    ));
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    w.cat = w.lib.open_catalog().unwrap();
    rebuild_catalog(&mut w.cat, &w.lib, vol, &w.media).unwrap();
    // (Asset ids are kept in the sidecars.)
    ai::cloud::enqueue(&mut w.cat, Provider::Claude, &[b]).unwrap();
    assert!(keywords(&mut w, a).contains(&"AI|Sand Castle".to_owned()));
    // Stop drops what is still queued.
    assert_eq!(
        ai::cloud::stop(&mut w.cat, Provider::Claude, &[b]).unwrap(),
        1
    );
    assert_eq!(common::tree_hashes(&w.media), before, "originals unchanged");
}

#[test]
fn a_refused_key_fails_the_photo_and_says_why() {
    let mut w = World::new(&[("a.jpg", scene(1, 800, 600))]);
    let a = w.asset("a.jpg");
    let hash = keepsake_core::render::source::source_hash(&w.cat, a)
        .unwrap()
        .unwrap();
    w.cache
        .put_bytes(
            &mut w.cat,
            &hash,
            keepsake_core::render::Variant::Thumb1024,
            &jpeg(&scene(1, 1024, 768)),
        )
        .unwrap();
    ai::cloud::enqueue(&mut w.cat, Provider::Gemini, &[a]).unwrap();
    let fake = std::sync::Arc::new(FakeCloud {
        answer: Err(("Gemini did not accept the API key".into(), false)),
        sent: Default::default(),
    });
    cloud_service(&w, fake)
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    let (st, problem) = ai::cloud::status(&w.cat, Provider::Gemini, &[a]).unwrap();
    assert_eq!((st.done, st.pending, st.failed), (0, 0, 1));
    assert_eq!(
        problem.as_deref(),
        Some("Gemini did not accept the API key")
    );
    assert!(keywords(&mut w, a).is_empty());
    // Asked again (e.g. after a new key), it is queued again.
    assert_eq!(
        ai::cloud::enqueue(&mut w.cat, Provider::Gemini, &[a]).unwrap(),
        1
    );
}

#[test]
fn each_provider_has_its_own_jobs_and_one_worker_runs_them_all() {
    let mut w = World::new(&[("a.jpg", scene(1, 800, 600))]);
    let a = w.asset("a.jpg");
    let hash = keepsake_core::render::source::source_hash(&w.cat, a)
        .unwrap()
        .unwrap();
    w.cache
        .put_bytes(
            &mut w.cat,
            &hash,
            keepsake_core::render::Variant::Thumb1024,
            &jpeg(&scene(1, 1024, 768)),
        )
        .unwrap();
    // The same photo, sent to ChatGPT and to Gemini.
    ai::cloud::enqueue(&mut w.cat, Provider::OpenAi, &[a]).unwrap();
    ai::cloud::enqueue(&mut w.cat, Provider::Gemini, &[a]).unwrap();
    let (claude, _) = ai::cloud::status(&w.cat, Provider::Claude, &[a]).unwrap();
    assert_eq!(claude.pending, 0, "Claude was not asked");
    let fake = std::sync::Arc::new(FakeCloud {
        answer: Ok(ai::cloud::CloudTags {
            tags: vec!["Dog".into()],
            new_tags: vec![],
            model: "m".into(),
        }),
        sent: Default::default(),
    });
    cloud_service(&w, fake.clone())
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    let mut to: Vec<Provider> = fake.sent.lock().unwrap().iter().map(|s| s.0).collect();
    to.sort_by_key(|p| p.id());
    assert_eq!(to, [Provider::Gemini, Provider::OpenAi]);
    for p in [Provider::OpenAi, Provider::Gemini] {
        let (st, problem) = ai::cloud::status(&w.cat, p, &[a]).unwrap();
        assert_eq!(
            (st.done, st.pending, st.failed),
            (1, 0, 0),
            "{p}: {problem:?}"
        );
    }
    assert_eq!(keywords(&mut w, a), ["Dog"], "a keyword once, not twice");
}

// ── Which AI gave which keyword (ADR-039) ──────────────────────────────────

fn sources(w: &World, id: AssetId) -> Vec<(String, String)> {
    w.cat
        .read(|tx| tx.ai().tag_sources(id))
        .unwrap()
        .into_iter()
        .map(|(p, t)| (p, t.to_lightroom()))
        .collect()
}

fn pair(p: &str, t: &str) -> (String, String) {
    (p.to_owned(), t.to_owned())
}

#[test]
fn each_ai_is_recorded_for_the_keywords_it_gave_and_it_survives_a_rebuild() {
    let mut w = World::new(&[("a.jpg", scene(1, 800, 600))]);
    let before = common::tree_hashes(&w.media);
    let a = w.asset("a.jpg");
    let hash = keepsake_core::render::source::source_hash(&w.cat, a)
        .unwrap()
        .unwrap();
    w.cache
        .put_bytes(
            &mut w.cat,
            &hash,
            keepsake_core::render::Variant::Thumb1024,
            &jpeg(&scene(1, 1024, 768)),
        )
        .unwrap();
    // A keyword of the user's own, which an AI then also gives.
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(a, &[tag("Dog"), tag("Holiday")])
        .unwrap();
    // "All": the same photo to ChatGPT and Gemini.
    ai::cloud::enqueue(&mut w.cat, Provider::OpenAi, &[a]).unwrap();
    ai::cloud::enqueue(&mut w.cat, Provider::Gemini, &[a]).unwrap();
    let fake = std::sync::Arc::new(FakeCloud {
        answer: Ok(ai::cloud::CloudTags {
            tags: vec!["Dog".into(), "Beach".into()],
            new_tags: vec![],
            model: "m".into(),
        }),
        sent: Default::default(),
    });
    cloud_service(&w, fake)
        .run(&AtomicBool::new(false), &mut |_| {})
        .unwrap();
    assert_eq!(keywords(&mut w, a), ["Beach", "Dog", "Holiday"]);
    // Each AI has its own rows, also for the keyword the user had; nobody
    // gave Holiday.
    let both = vec![
        pair("gemini", "Beach"),
        pair("gemini", "Dog"),
        pair("openai", "Beach"),
        pair("openai", "Dog"),
    ];
    assert_eq!(sources(&w, a), both);

    // The sidecar has them, so a rebuild restores them.
    let vol = w.cat.read(|tx| tx.volumes().list()).unwrap()[0].id;
    drop(std::mem::replace(
        &mut w.cat,
        Catalog::open_in_memory().unwrap(),
    ));
    Catalog::delete_files(&w.lib.catalog_path()).unwrap();
    w.cat = w.lib.open_catalog().unwrap();
    rebuild_catalog(&mut w.cat, &w.lib, vol, &w.media).unwrap();
    assert_eq!(sources(&w, a), both);

    // Removing a keyword forgets who gave it.
    MetadataService::new(&w.lib, &mut w.cat)
        .set_user_tags(a, &[tag("Dog"), tag("Holiday")])
        .unwrap();
    assert_eq!(
        sources(&w, a),
        [pair("gemini", "Dog"), pair("openai", "Dog")]
    );
    assert_eq!(common::tree_hashes(&w.media), before, "originals unchanged");
}
