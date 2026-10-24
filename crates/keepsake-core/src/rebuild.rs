//! Reconstruct a volume's part of the catalog from its files and XMP sidecars.
//!
//! Rebuild never writes to the volume. It:
//! 1. walks the volume (skipping hidden/system entries) and hashes every file;
//! 2. reads sidecars beside the files and in the library's mirror tree,
//!    keeping the one with the newest `xmp:MetadataDate` per file;
//! 3. forms assets from Keepsake sidecars' member lists, then groups the
//!    remaining files with [`crate::grouping`] (XAVC, stills, AVCHD playlists);
//! 4. with a [`Prober`], reads capture time, camera, lens, duration and GPS
//!    from the media of assets that have no Keepsake sidecar, and flags
//!    burst/bracket/interval sequences;
//! 5. replaces the volume's rows in one transaction.
//!
//! Values from a Keepsake sidecar win over probed ones (they include the
//! user's corrections). When an asset already exists from another volume, the
//! sidecar wins only if its metadata is newer than the catalog's.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::catalog::{
    Asset, AssetId, AssetTag, Blake3Hash, CaptureSource, Catalog, Confidence, EditRecipe, Event,
    EventId, FileRole, MediaKind, NewFile, Rating, SequenceId, SequenceKind, SidecarLocation,
    SidecarRecord, StillSequence, TagSource, VolumeId, VolumeKind,
};
use crate::corpus::scanner::skip_reason;
use crate::grouping::{self, Grouping};
use crate::library::{probe_volume, Library, LibraryError, VolumeRecord};
use crate::probe::{ProbedAsset, Prober};
use crate::relpath::{path_key, RelPath};
use crate::xmp::{sidecar, AssetXmp};

/// A path that rebuild could not index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SkippedPath {
    pub path: String,
    pub reason: String,
}

/// Summary of a rebuild.
#[derive(Debug, Clone, Default, Serialize)]
pub struct RebuildReport {
    pub files_indexed: u64,
    pub assets: u64,
    pub sidecars_used: u64,
    /// Assets whose metadata came from probing the media.
    pub probed: u64,
    pub sequences: u64,
    pub hidden_skipped: u64,
    pub skipped: Vec<SkippedPath>,
    pub problems: Vec<String>,
}

struct Scanned {
    rel: RelPath,
    key: String,
    size: u64,
    blake3: Blake3Hash,
    quick: Option<u64>,
    mtime_ms: Option<i64>,
}

struct FoundSidecar {
    location: SidecarLocation,
    rel: RelPath,
    blake3: Blake3Hash,
    mtime_ms: i64,
    xmp: AssetXmp,
}

struct Discovered {
    asset_id: AssetId,
    xmp: Option<AssetXmp>,
    sidecar: Option<usize>,
    /// `(file index, role, segment index)`.
    members: Vec<(usize, FileRole, Option<u32>)>,
    media_kind: Option<MediaKind>,
    probed: Option<ProbedAsset>,
}

const VIDEO_EXT: &[&str] = &["mp4", "mts", "m2ts", "mov", "avi", "mkv"];

fn ext_of(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Photo or video, from a file name.
pub fn media_kind_for(name: &str) -> MediaKind {
    if VIDEO_EXT.contains(&ext_of(name).as_str()) {
        MediaKind::Video
    } else {
        MediaKind::Photo
    }
}

fn mtime_ms(meta: &std::fs::Metadata) -> Option<i64> {
    let d = meta
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    i64::try_from(d.as_millis()).ok()
}

fn hash_file(path: &Path) -> std::io::Result<Blake3Hash> {
    let mut f = std::fs::File::open(path)?;
    let mut h = blake3::Hasher::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(Blake3Hash(*h.finalize().as_bytes()))
}

/// Recursively list regular files under `dir`, sorted, skipping hidden/system
/// entries and symlinks.
fn walk(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(RelPath, std::fs::Metadata)>,
    report: &mut RebuildReport,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            report
                .problems
                .push(format!("cannot read folder {}: {e}", dir.display()));
            return;
        }
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        let meta = match std::fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                report
                    .problems
                    .push(format!("cannot stat {}: {e}", path.display()));
                continue;
            }
        };
        if skip_reason(&path, &meta).is_some() {
            report.hidden_skipped += 1;
            continue;
        }
        if meta.file_type().is_symlink() {
            report.skipped.push(SkippedPath {
                path: path.display().to_string(),
                reason: "symbolic link".into(),
            });
        } else if meta.is_dir() {
            walk(root, &path, out, report);
        } else if meta.is_file() {
            match RelPath::from_fs(root, &path) {
                Ok(rel) => out.push((rel, meta)),
                Err(e) => report.skipped.push(SkippedPath {
                    path: path.display().to_string(),
                    reason: e.to_string(),
                }),
            }
        }
    }
}

/// Every non-hidden, non-sidecar file under `root`, sorted.
pub fn list_media_files(root: &Path) -> (Vec<RelPath>, RebuildReport) {
    let mut report = RebuildReport::default();
    let mut listing = Vec::new();
    walk(root, root, &mut listing, &mut report);
    let files = listing
        .into_iter()
        .map(|(r, _)| r)
        .filter(|r| !sidecar::is_sidecar_name(r.file_name()))
        .collect();
    (files, report)
}

/// Group (and optionally probe) the media under `root` without touching any catalog.
pub fn group_folder(root: &Path, files: &[RelPath]) -> Grouping {
    let (layout, problems) = grouping::read_avchd_layout(root, files);
    let mut g = grouping::group(files, layout.as_ref());
    g.problems.extend(problems);
    g
}

fn newer(a: &FoundSidecar, b: &FoundSidecar) -> bool {
    let (ma, mb) = (
        a.xmp.metadata_ms.unwrap_or(i64::MIN),
        b.xmp.metadata_ms.unwrap_or(i64::MIN),
    );
    ma > mb || (ma == mb && a.location == SidecarLocation::Beside && b.location != a.location)
}

fn keep_newest(map: &mut HashMap<String, usize>, key: String, idx: usize, all: &[FoundSidecar]) {
    match map.get(&key) {
        Some(&cur) if !newer(&all[idx], &all[cur]) => {}
        _ => {
            map.insert(key, idx);
        }
    }
}

/// Rebuild one volume's catalog rows from files and sidecars only.
pub fn rebuild_catalog(
    cat: &mut Catalog,
    lib: &Library,
    volume: VolumeId,
    mount: &Path,
) -> Result<RebuildReport, LibraryError> {
    rebuild_catalog_with(cat, lib, volume, mount, None)
}

/// Rebuild one volume, probing media with `prober` when given.
pub fn rebuild_catalog_with(
    cat: &mut Catalog,
    lib: &Library,
    volume: VolumeId,
    mount: &Path,
    prober: Option<&Prober>,
) -> Result<RebuildReport, LibraryError> {
    let rec = lib.volume(volume)?;
    let case = rec.case;
    let mut report = RebuildReport::default();

    // 1. Walk the volume.
    let mut listing = Vec::new();
    walk(mount, mount, &mut listing, &mut report);
    let mut files: Vec<Scanned> = Vec::new();
    let mut beside_sidecars: Vec<RelPath> = Vec::new();
    for (rel, meta) in listing {
        if sidecar::is_sidecar_name(rel.file_name()) {
            beside_sidecars.push(rel);
            continue;
        }
        let full = rel.to_fs_path(mount);
        match hash_file(&full) {
            Ok(blake3) => files.push(Scanned {
                key: rel.key(case),
                quick: crate::ingest::source::quick_hash(&full).ok(),
                rel,
                size: meta.len(),
                blake3,
                mtime_ms: mtime_ms(&meta),
            }),
            Err(e) => report.skipped.push(SkippedPath {
                path: rel.to_string(),
                reason: format!("cannot read: {e}"),
            }),
        }
    }
    let by_key: HashMap<String, usize> = files
        .iter()
        .enumerate()
        .map(|(i, f)| (f.key.clone(), i))
        .collect();

    // 2. Read sidecars.
    let mirror_root = sidecar::mirror_root(&lib.sidecars_dir(), volume);
    let mut mirror_listing = Vec::new();
    if mirror_root.is_dir() {
        walk(&mirror_root, &mirror_root, &mut mirror_listing, &mut report);
    }
    let candidates = beside_sidecars
        .into_iter()
        .map(|r| (SidecarLocation::Beside, r.to_fs_path(mount), r))
        .chain(
            mirror_listing
                .into_iter()
                .filter(|(r, _)| sidecar::is_sidecar_name(r.file_name()))
                .map(|(r, _)| (SidecarLocation::Mirror, r.to_fs_path(&mirror_root), r)),
        );
    let mut found: Vec<FoundSidecar> = Vec::new();
    for (location, full, rel) in candidates {
        match sidecar::read(&full) {
            Ok(Some(l)) => found.push(FoundSidecar {
                location,
                rel,
                blake3: l.blake3,
                mtime_ms: l.mtime_ms,
                xmp: AssetXmp::read(&l.packet),
            }),
            Ok(None) => {}
            Err(e) => report
                .problems
                .push(format!("unreadable sidecar {}: {e}", full.display())),
        }
    }

    // Map sidecars to the files they describe.
    let mut exact: HashMap<String, usize> = HashMap::new();
    let mut lightroom: HashMap<String, usize> = HashMap::new();
    for (i, s) in found.iter().enumerate() {
        let target = s.rel.as_str();
        let target = &target[..target.len().saturating_sub(4)];
        if let Ok(t) = RelPath::parse(target) {
            let key = t.key(case);
            if by_key.contains_key(&key) {
                keep_newest(&mut exact, key, i, &found);
                continue;
            }
            // Lightroom `<stem>.xmp`: matches files with that stem in the same folder.
            for f in &files {
                let Some((stem, _)) = f.rel.file_name().rsplit_once('.') else {
                    continue;
                };
                let stem_rel = match f.rel.parent() {
                    Some(p) => format!("{p}/{stem}"),
                    None => stem.to_owned(),
                };
                if path_key(&stem_rel, case) == key {
                    keep_newest(&mut lightroom, f.key.clone(), i, &found);
                }
            }
        } else {
            report
                .problems
                .push(format!("sidecar {} has no target name", s.rel));
        }
    }

    // 3a. Assets from Keepsake sidecars that list their member files.
    let mut claimed: HashSet<usize> = HashSet::new();
    let mut used_ids: HashSet<AssetId> = HashSet::new();
    let mut discovered: Vec<Discovered> = Vec::new();
    let mut exact_sorted: Vec<(&String, &usize)> = exact.iter().collect();
    exact_sorted.sort();
    for (target_key, &si) in exact_sorted {
        let s = &found[si];
        let (Some(asset_id), false) = (s.xmp.asset_id, s.xmp.files.is_empty()) else {
            continue;
        };
        if used_ids.contains(&asset_id) {
            report.problems.push(format!(
                "asset {asset_id} has more than one sidecar on this volume"
            ));
            continue;
        }
        let Some(&orig) = by_key.get(target_key) else {
            continue;
        };
        if claimed.contains(&orig) {
            continue;
        }
        let mut members = vec![(orig, FileRole::Original, None)];
        for fr in &s.xmp.files {
            let Some(&i) = by_key.get(&fr.rel_path.key(case)) else {
                report.problems.push(format!(
                    "asset {asset_id}: listed file {} is missing",
                    fr.rel_path
                ));
                continue;
            };
            if i == orig || members.iter().any(|(m, _, _)| *m == i) {
                continue;
            }
            if claimed.contains(&i) {
                report.problems.push(format!(
                    "asset {asset_id}: file {} already belongs to another asset",
                    fr.rel_path
                ));
                continue;
            }
            let role = if fr.role == FileRole::Original {
                FileRole::RawPair
            } else {
                fr.role
            };
            members.push((
                i,
                role,
                fr.segment_index.filter(|_| role == FileRole::Segment),
            ));
        }
        claimed.extend(members.iter().map(|(i, _, _)| *i));
        used_ids.insert(asset_id);
        discovered.push(Discovered {
            asset_id,
            xmp: Some(s.xmp.clone()),
            sidecar: Some(si),
            members,
            media_kind: None,
            probed: None,
        });
    }

    // 3b. Group the remaining files.
    let rest: Vec<usize> = (0..files.len()).filter(|i| !claimed.contains(i)).collect();
    let rest_rels: Vec<RelPath> = rest.iter().map(|i| files[*i].rel.clone()).collect();
    let all_rels: Vec<RelPath> = files.iter().map(|f| f.rel.clone()).collect();
    let (layout, layout_problems) = grouping::read_avchd_layout(mount, &all_rels);
    report.problems.extend(layout_problems);
    let grouped = grouping::group(&rest_rels, layout.as_ref());
    report.problems.extend(grouped.problems.iter().cloned());
    for (list, reason) in [
        (&grouped.card_files, "camera card file"),
        (&grouped.ungrouped, "not part of any asset"),
    ] {
        for &i in list {
            report.skipped.push(SkippedPath {
                path: rest_rels[i].to_string(),
                reason: reason.into(),
            });
        }
    }
    let probe_report = prober.map(|p| p.probe(mount, &rest_rels, &grouped));
    let first_new = discovered.len();
    for (gi, g) in grouped.groups.iter().enumerate() {
        let orig = rest[g.original()];
        let member_files: Vec<usize> = g.members.iter().map(|m| rest[m.index]).collect();
        let pick = |map: &HashMap<String, usize>| -> Option<usize> {
            map.get(&files[orig].key).copied().or_else(|| {
                member_files
                    .iter()
                    .find_map(|i| map.get(&files[*i].key).copied())
            })
        };
        let si = pick(&exact).or_else(|| pick(&lightroom));
        let xmp = si.map(|i| found[i].xmp.clone());
        let asset_id = xmp
            .as_ref()
            .and_then(|x| x.asset_id)
            .filter(|id| !used_ids.contains(id))
            .unwrap_or_else(|| AssetId::for_original(volume, &files[orig].key));
        used_ids.insert(asset_id);
        claimed.extend(member_files.iter().copied());
        discovered.push(Discovered {
            asset_id,
            xmp,
            sidecar: si,
            members: g
                .members
                .iter()
                .map(|m| (rest[m.index], m.role, m.segment.filter(|p| *p > 0)))
                .collect(),
            media_kind: Some(g.media_kind()),
            probed: probe_report
                .as_ref()
                .and_then(|r| r.assets.get(gi).cloned()),
        });
    }

    // Sequences: from Keepsake sidecars, then from probing.
    let mut sequences: BTreeMap<SequenceId, (SequenceKind, Vec<(u32, AssetId)>)> = BTreeMap::new();
    for d in &discovered {
        if d.probed.is_some() && d.xmp.as_ref().is_none_or(|x| x.asset_id.is_none()) {
            continue; // probed assets take their sequence from the probe below
        }
        if let Some(s) = d.xmp.as_ref().and_then(|x| x.sequence) {
            sequences
                .entry(s.id)
                .or_insert_with(|| (s.kind, Vec::new()))
                .1
                .push((s.position, d.asset_id));
        }
    }
    if let Some(r) = &probe_report {
        for s in &r.sequences {
            let ids: Vec<AssetId> = s
                .members
                .iter()
                .map(|gi| discovered[first_new + gi].asset_id)
                .collect();
            let Some(first) = s.members.first() else {
                continue;
            };
            let first_key = &files[rest[grouped.groups[*first].original()]].key;
            let id = SequenceId::for_first(volume, first_key);
            let entry = sequences.entry(id).or_insert_with(|| (s.kind, Vec::new()));
            entry.1 = ids
                .into_iter()
                .enumerate()
                .map(|(p, a)| (p as u32, a))
                .collect();
        }
    }

    // Events: newest sidecar wins.
    let mut events: BTreeMap<EventId, (i64, Event)> = BTreeMap::new();
    for d in &discovered {
        if let Some(e) = d.xmp.as_ref().and_then(|x| x.event.clone()) {
            let m = d
                .xmp
                .as_ref()
                .and_then(|x| x.metadata_ms)
                .unwrap_or(i64::MIN);
            match events.get(&e.id) {
                Some((cur, _)) if *cur >= m => {}
                _ => {
                    events.insert(e.id, (m, e));
                }
            }
        }
    }

    // Verify recipe files before the transaction.
    let mut recipes: HashMap<AssetId, Vec<EditRecipe>> = HashMap::new();
    for d in &discovered {
        let Some(x) = &d.xmp else { continue };
        for r in &x.recipes {
            let path = r.rel_path.to_fs_path(&lib.recipes_dir());
            match std::fs::read(&path) {
                Ok(bytes) => {
                    let hash = Blake3Hash::of(&bytes);
                    if hash != r.content_hash {
                        report.problems.push(format!(
                            "recipe {} changed on disk since it was referenced",
                            r.id
                        ));
                    }
                    recipes.entry(d.asset_id).or_default().push(EditRecipe {
                        id: r.id,
                        asset_id: d.asset_id,
                        kind: r.kind.clone(),
                        schema_version: r.schema_version,
                        rel_path: r.rel_path.clone(),
                        content_hash: hash,
                        created_ms: r.created_ms,
                        updated_ms: r.updated_ms,
                    });
                }
                Err(_) => report.problems.push(format!(
                    "recipe {} referenced by asset {} is missing ({})",
                    r.id,
                    d.asset_id,
                    path.display()
                )),
            }
        }
    }

    // 4. Replace the volume's rows.
    let vol = rec.to_volume(Some(mount));
    let mut probed_count = 0u64;
    cat.write(|tx| -> Result<(), LibraryError> {
        tx.files().delete_for_volume(volume)?;
        tx.sidecars().delete_for_volume(volume)?;
        tx.sequences().delete_for_volume(volume)?;
        tx.assets().delete_orphans()?;
        tx.volumes().upsert(&vol)?;
        for (_, e) in events.values() {
            tx.events().upsert(e)?;
        }

        for d in &discovered {
            let orig = d
                .members
                .iter()
                .find(|(_, r, _)| *r == FileRole::Original)
                .map(|(i, _, _)| &files[*i])
                .ok_or_else(|| LibraryError::Invalid("asset without original".into()))?;
            let x = d.xmp.clone().unwrap_or_default();
            let own_sidecar = x.asset_id.is_some();
            let mut asset = Asset {
                id: d.asset_id,
                media_kind: x
                    .media_kind
                    .or(d.media_kind)
                    .unwrap_or_else(|| media_kind_for(orig.rel.file_name())),
                capture: x.capture,
                capture_shift_s: x.capture_shift_s,
                duration_ms: x.duration_ms,
                camera_make: x.camera_make.clone(),
                camera_model: x.camera_model.clone(),
                lens_model: x.lens_model.clone(),
                event_id: x.event.as_ref().map(|e| e.id),
                rating: x.rating.unwrap_or(Rating::default()),
                label: x.label.clone(),
                comment: x.comment.clone(),
                created_ms: x.added_ms.or(orig.mtime_ms).unwrap_or(0),
                metadata_ms: x.metadata_ms.unwrap_or(0),
                capture_source: x.capture_source,
                capture_confidence: x.capture_confidence,
                gps: x.gps,
            };
            if !own_sidecar {
                // A foreign (Lightroom/digiKam) date is a fallback only.
                if asset.capture.is_some() && asset.capture_source.is_none() {
                    asset.capture_source = Some(CaptureSource::Sidecar);
                    asset.capture_confidence = Some(
                        if asset.capture.is_some_and(|c| c.utc_offset_min.is_some()) {
                            Confidence::High
                        } else {
                            Confidence::Low
                        },
                    );
                }
                if let Some(p) = &d.probed {
                    probed_count += 1;
                    if let Some(c) = &p.capture {
                        asset.capture = Some(c.time);
                        asset.capture_source = Some(c.source);
                        asset.capture_confidence = Some(c.confidence);
                    }
                    asset.camera_make = p.make.clone().or(asset.camera_make);
                    asset.camera_model = p.model.clone().or(asset.camera_model);
                    asset.lens_model = p.lens.clone().or(asset.lens_model);
                    asset.duration_ms = p.duration_ms.or(asset.duration_ms);
                    asset.gps = p.gps.or(asset.gps);
                    for prob in &p.problems {
                        report.problems.push(format!("{}: {prob}", orig.rel));
                    }
                }
            }
            let apply = match tx.assets().get(d.asset_id)? {
                None => {
                    tx.assets().insert(&asset)?;
                    true
                }
                Some(existing) if asset.metadata_ms > existing.metadata_ms => {
                    tx.assets().update(&asset)?;
                    true
                }
                Some(_) => false,
            };
            if apply {
                let user: Vec<AssetTag> = x.user_tags.iter().cloned().map(AssetTag::user).collect();
                let ai: Vec<AssetTag> = x
                    .ai_tags
                    .iter()
                    .map(|(p, c)| AssetTag::ai(p.clone(), *c))
                    .collect();
                tx.tags()
                    .set_for_asset(d.asset_id, TagSource::User, &user)?;
                tx.tags().set_for_asset(d.asset_id, TagSource::Ai, &ai)?;
                for old in tx.recipes().for_asset(d.asset_id)? {
                    tx.recipes().delete(old.id)?;
                }
                for r in recipes.get(&d.asset_id).into_iter().flatten() {
                    tx.recipes().upsert(r)?;
                }
            }
            for (i, role, seg) in &d.members {
                let f = &files[*i];
                tx.files().insert(
                    d.asset_id,
                    &NewFile {
                        volume_id: volume,
                        rel_path: f.rel.clone(),
                        role: *role,
                        size_bytes: f.size,
                        blake3: f.blake3,
                        xxh3: f.quick,
                        mtime_ms: f.mtime_ms,
                        segment_index: *seg,
                    },
                )?;
            }
            if let Some(si) = d.sidecar {
                let s = &found[si];
                tx.sidecars().upsert(&SidecarRecord {
                    asset_id: d.asset_id,
                    volume_id: volume,
                    location: s.location,
                    rel_path: s.rel.clone(),
                    blake3: s.blake3,
                    mtime_ms: s.mtime_ms,
                })?;
            }
        }
        for (id, (kind, mut members)) in std::mem::take(&mut sequences) {
            members.sort();
            tx.sequences().upsert(&StillSequence {
                id,
                volume_id: volume,
                kind,
                members: members.into_iter().map(|(_, a)| a).collect(),
            })?;
        }
        tx.sequences().delete_small()?;
        tx.events().delete_empty()?;
        tx.tags().prune_unused()?;
        report.sequences = tx
            .sequences()
            .list()?
            .iter()
            .filter(|s| s.volume_id == volume)
            .count() as u64;
        Ok(())
    })?;

    report.assets = discovered.len() as u64;
    report.files_indexed = discovered.iter().map(|d| d.members.len() as u64).sum();
    report.sidecars_used = discovered.iter().filter(|d| d.sidecar.is_some()).count() as u64;
    report.probed = probed_count;
    tracing::info!(
        %volume,
        assets = report.assets,
        files = report.files_indexed,
        probed = report.probed,
        problems = report.problems.len(),
        "rebuilt volume"
    );
    Ok(report)
}

/// Register the folder at `mount` as a volume (or find it again if it was
/// registered before) and index it. Read-only is detected, or forced with
/// `force_read_only`.
///
/// Until the device watcher provides real file-system ids, the canonical mount
/// path is used as the volume's `fs_id`.
pub fn add_volume(
    cat: &mut Catalog,
    lib: &Library,
    label: &str,
    kind: VolumeKind,
    mount: &Path,
    force_read_only: bool,
    prober: Option<&Prober>,
) -> Result<(VolumeRecord, RebuildReport), LibraryError> {
    let mount = std::fs::canonicalize(mount)?;
    if !mount.is_dir() {
        return Err(LibraryError::Invalid(format!(
            "{} is not a folder",
            mount.display()
        )));
    }
    let fs_id = mount.display().to_string();
    let probe = probe_volume(&mount);
    let existing = lib.volumes()?.into_iter().find(|v| v.fs_id == fs_id);
    let rec = VolumeRecord {
        id: existing
            .as_ref()
            .map(|v| v.id)
            .unwrap_or_else(VolumeId::new_random),
        label: label.to_owned(),
        fs_id,
        kind,
        case: probe.case,
        read_only: force_read_only || probe.read_only,
        last_mount: Some(mount.clone()),
    };
    lib.register_volume(&rec)?;
    let report = rebuild_catalog_with(cat, lib, rec.id, &mount, prober)?;
    Ok((rec, report))
}

/// Rebuild every registered volume found at its last mount point. Volumes that
/// are not mounted are marked offline and reported as `None`.
pub fn rebuild_library(
    cat: &mut Catalog,
    lib: &Library,
    prober: Option<&Prober>,
) -> Result<Vec<(VolumeRecord, Option<RebuildReport>)>, LibraryError> {
    let mut out = Vec::new();
    // Cards are import sources; their files are not library storage.
    for rec in lib
        .volumes()?
        .into_iter()
        .filter(|v| v.kind != VolumeKind::Card)
    {
        match rec.last_mount.as_deref().filter(|m| m.is_dir()) {
            Some(m) => {
                let report = rebuild_catalog_with(cat, lib, rec.id, m, prober)?;
                out.push((rec, Some(report)));
            }
            None => {
                let v = rec.to_volume(None);
                cat.write(|tx| -> Result<(), LibraryError> {
                    if tx.volumes().get(v.id)?.is_some() {
                        tx.volumes()
                            .set_mount(v.id, None, crate::catalog::now_ms())?;
                    } else {
                        tx.volumes().upsert(&v)?;
                    }
                    Ok(())
                })?;
                out.push((rec, None));
            }
        }
    }
    Ok(out)
}
