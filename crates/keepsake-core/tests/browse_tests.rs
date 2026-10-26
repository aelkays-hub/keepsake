//! Browsing: filters, sorts, pages, ranges and the calendar, on a seeded
//! synthetic library.

use std::time::Instant;

use keepsake_core::catalog::{
    CalendarLevel, Catalog, FacetColumn, MediaKind, SearchQuery, Sort, SortField,
};
use keepsake_core::library::Library;
use keepsake_core::render::{Cache, Variant};
use keepsake_core::seed::{seed, SeedOptions, SeedReport};

struct World {
    _tmp: tempfile::TempDir,
    lib: Library,
    cat: Catalog,
    cache: Cache,
    report: SeedReport,
}

fn world(assets: u64) -> World {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let cache = Cache::new(tmp.path().join("cache"));
    let report = seed(
        &lib,
        &cache,
        &SeedOptions {
            assets,
            distinct: 8,
            offline_share: 0.25,
        },
    )
    .unwrap();
    let cat = lib.open_catalog().unwrap();
    World {
        _tmp: tmp,
        lib,
        cat,
        cache,
        report,
    }
}

fn all() -> SearchQuery {
    SearchQuery::default()
}

fn sort(field: SortField, descending: bool) -> Sort {
    Sort { field, descending }
}

#[test]
fn counts_and_pages_cover_the_whole_view_in_order() {
    let w = world(1200);
    let n = w.cat.read(|tx| tx.browse().count(&all())).unwrap();
    assert_eq!(n, 1200);
    let mut seen = Vec::new();
    let mut last: Option<i64> = None;
    let mut offset = 0;
    loop {
        let page = w
            .cat
            .read(|tx| {
                tx.browse()
                    .page(&all(), sort(SortField::Date, false), offset, 250)
            })
            .unwrap();
        if page.is_empty() {
            break;
        }
        for r in &page {
            if let (Some(prev), Some(t)) = (last, r.capture_ms) {
                assert!(t >= prev, "date order");
            }
            if r.capture_ms.is_some() {
                last = r.capture_ms;
            }
            assert!(r.source_hash.is_some() && !r.volumes.is_empty());
            assert!(
                r.name.is_some() && r.size.is_some(),
                "kept by the v5 triggers"
            );
            seen.push(r.id);
        }
        offset += 250;
    }
    assert_eq!(seen.len(), 1200);
    seen.sort();
    seen.dedup();
    assert_eq!(seen.len(), 1200, "no duplicates across pages");
    // Undated assets come last.
    let tail = w
        .cat
        .read(|tx| {
            tx.browse()
                .page(&all(), sort(SortField::Date, false), 1199, 1)
        })
        .unwrap();
    let undated = w
        .cat
        .read(|tx| {
            tx.browse().count(&SearchQuery {
                undated: true,
                ..all()
            })
        })
        .unwrap();
    if undated > 0 {
        assert!(tail[0].capture_ms.is_none());
    }
}

#[test]
fn every_sort_is_total_and_reversible() {
    let w = world(400);
    for field in [
        SortField::Date,
        SortField::Name,
        SortField::Camera,
        SortField::Lens,
        SortField::Type,
        SortField::Size,
        SortField::Duration,
        SortField::Rating,
    ] {
        let up = w
            .cat
            .read(|tx| tx.browse().page(&all(), sort(field, false), 0, 400))
            .unwrap();
        let down = w
            .cat
            .read(|tx| tx.browse().page(&all(), sort(field, true), 0, 400))
            .unwrap();
        assert_eq!(up.len(), 400, "{field:?}");
        let key = |r: &keepsake_core::catalog::BrowseRow| -> String {
            match field {
                SortField::Name => r.name.clone().unwrap_or_default().to_lowercase(),
                SortField::Camera => r.camera.clone().unwrap_or_default().to_lowercase(),
                SortField::Lens => r.lens.clone().unwrap_or_default().to_lowercase(),
                SortField::Size => format!("{:020}", r.size.unwrap_or(0)),
                SortField::Rating => r.rating.to_string(),
                SortField::Duration => format!("{:020}", r.duration_ms.unwrap_or(i64::MAX)),
                SortField::Type => format!("{:?}{}", r.kind, r.raw),
                SortField::Date => format!("{:020}", r.capture_ms.unwrap_or(i64::MAX)),
            }
        };
        let keys: Vec<String> = up.iter().map(key).collect();
        if field != SortField::Type && field != SortField::Duration {
            let mut sorted = keys.clone();
            sorted.sort();
            assert_eq!(keys, sorted, "{field:?} ascending");
        }
        // Descending visits the same rows.
        let mut a: Vec<_> = up.iter().map(|r| r.id).collect();
        let mut b: Vec<_> = down.iter().map(|r| r.id).collect();
        a.sort();
        b.sort();
        assert_eq!(a, b, "{field:?}");
    }
}

#[test]
fn filters_narrow_and_combine() {
    let w = world(1500);
    let count = |q: SearchQuery| w.cat.read(|tx| tx.browse().count(&q)).unwrap();
    let total = count(all());
    let videos = count(SearchQuery {
        kind: Some(MediaKind::Video),
        ..all()
    });
    let photos = count(SearchQuery {
        kind: Some(MediaKind::Photo),
        ..all()
    });
    assert_eq!(videos + photos, total);
    let raw = count(SearchQuery {
        raw: Some(true),
        ..all()
    });
    assert!(raw > 0 && raw < photos);
    let a7 = count(SearchQuery {
        camera: Some("7m4".into()),
        ..all()
    });
    assert!(a7 > 0 && a7 < total);
    let gm = count(SearchQuery {
        lens: Some("gm".into()),
        ..all()
    });
    assert!(gm > 0 && gm < total);
    let four = count(SearchQuery {
        min_rating: Some(4),
        ..all()
    });
    let three = count(SearchQuery {
        min_rating: Some(3),
        ..all()
    });
    assert!(four > 0 && four < three);
    let red = count(SearchQuery {
        label: Some("red".into()),
        ..all()
    });
    assert!(red > 0);
    let offline = count(SearchQuery {
        volume: Some(w.report.offline_volume),
        ..all()
    });
    let online = count(SearchQuery {
        volume: Some(w.report.online_volume),
        ..all()
    });
    assert_eq!(offline + online, total);
    assert!(offline > total / 10 && offline < total / 2, "{offline}");
    let combo = count(SearchQuery {
        kind: Some(MediaKind::Photo),
        camera: Some("ILCE-1".into()),
        min_rating: Some(3),
        from: Some("2016-01-01".into()),
        to: Some("2019-12-31".into()),
        ..all()
    });
    assert!(combo > 0 && combo < three);
    // Words match file names.
    let named = count(SearchQuery {
        text: Some("DSC00010".into()),
        ..all()
    });
    assert!(named >= 1);
    assert_eq!(
        count(SearchQuery {
            text: Some("***".into()),
            ..all()
        }),
        0
    );
}

#[test]
fn view_snapshots_page_like_the_query() {
    let w = world(500);
    for field in [SortField::Date, SortField::Name, SortField::Rating] {
        let s = sort(field, true);
        let ids = w.cat.read(|tx| tx.browse().ordered_ids(&all(), s)).unwrap();
        assert_eq!(ids.len(), 500);
        let direct = w
            .cat
            .read(|tx| tx.browse().page(&all(), s, 200, 100))
            .unwrap();
        let snap = w.cat.read(|tx| tx.browse().rows(&ids[200..300])).unwrap();
        assert_eq!(direct, snap, "{field:?}");
    }
}

#[test]
fn ranges_and_positions_match_pages() {
    let w = world(600);
    let s = sort(SortField::Name, false);
    let page = w
        .cat
        .read(|tx| tx.browse().page(&all(), s, 100, 50))
        .unwrap();
    let ids = w
        .cat
        .read(|tx| tx.browse().ids_range(&all(), s, 100, 150))
        .unwrap();
    assert_eq!(ids, page.iter().map(|r| r.id).collect::<Vec<_>>());
    let pos = w
        .cat
        .read(|tx| tx.browse().index_of(&all(), s, page[7].id))
        .unwrap();
    assert_eq!(pos, Some(107));
    let q = SearchQuery {
        kind: Some(MediaKind::Video),
        ..all()
    };
    let photo = page.iter().find(|r| r.kind == MediaKind::Photo).unwrap();
    assert_eq!(
        w.cat
            .read(|tx| tx.browse().index_of(&q, s, photo.id))
            .unwrap(),
        None
    );
}

#[test]
fn calendar_counts_add_up_and_have_covers() {
    let w = world(2000);
    let years = w
        .cat
        .read(|tx| tx.browse().calendar(&all(), CalendarLevel::Years))
        .unwrap();
    assert!(years.len() >= 10, "{years:?}");
    let sum: u64 = years.iter().map(|b| b.count).sum();
    assert_eq!(sum, 2000);
    let y = years.iter().find(|b| b.key == "2018").unwrap();
    assert!(y.cover.is_some() && y.cover_hash.is_some());
    let months = w
        .cat
        .read(|tx| {
            tx.browse()
                .calendar(&all(), CalendarLevel::Months { year: 2018 })
        })
        .unwrap();
    assert_eq!(months.iter().map(|b| b.count).sum::<u64>(), y.count);
    assert!(months
        .iter()
        .all(|b| b.key.starts_with("2018-") && b.key.len() == 7));
    let m = &months[0];
    let month: u32 = m.key[5..].parse().unwrap();
    let days = w
        .cat
        .read(|tx| {
            tx.browse()
                .calendar(&all(), CalendarLevel::Days { year: 2018, month })
        })
        .unwrap();
    assert_eq!(days.iter().map(|b| b.count).sum::<u64>(), m.count);
    // A day's grid is the date filter for that day.
    let d = &days[0];
    let n = w
        .cat
        .read(|tx| {
            tx.browse().count(&SearchQuery {
                from: Some(d.key.clone()),
                to: Some(d.key.clone()),
                ..all()
            })
        })
        .unwrap();
    assert_eq!(n, d.count);
    // The cover is the best-rated asset of the cell, the earliest of those.
    for b in &days {
        let day = SearchQuery {
            from: Some(b.key.clone()),
            to: Some(b.key.clone()),
            ..all()
        };
        let by_date = sort(SortField::Date, false);
        let ids = w
            .cat
            .read(|tx| tx.browse().ordered_ids(&day, by_date))
            .unwrap();
        let rows = w.cat.read(|tx| tx.browse().rows(&ids)).unwrap();
        let best = rows.iter().map(|r| r.rating).max().unwrap();
        let first_best = ids
            .iter()
            .find(|id| rows.iter().any(|r| r.id == **id && r.rating == best))
            .unwrap();
        assert_eq!(b.cover.as_ref(), Some(first_best), "day {}", b.key);
    }
    // ... and has a thumbnail.
    let mut cat = w.lib.open_catalog().unwrap();
    assert!(w
        .cache
        .lookup(&mut cat, d.cover.unwrap(), Variant::Thumb256)
        .unwrap()
        .is_some());
    // Filters apply to the calendar too.
    let videos = w
        .cat
        .read(|tx| {
            tx.browse().calendar(
                &SearchQuery {
                    kind: Some(MediaKind::Video),
                    ..all()
                },
                CalendarLevel::Years,
            )
        })
        .unwrap();
    assert!(videos.iter().map(|b| b.count).sum::<u64>() < sum);
}

#[test]
fn facets_list_cameras_and_lenses() {
    let w = world(300);
    let cams = w
        .cat
        .read(|tx| tx.browse().facets(FacetColumn::Camera))
        .unwrap();
    assert_eq!(cams.len(), 4);
    assert_eq!(cams.iter().map(|f| f.count).sum::<u64>(), 300);
    let lenses = w
        .cat
        .read(|tx| tx.browse().facets(FacetColumn::Lens))
        .unwrap();
    assert_eq!(lenses.len(), 5);
}

#[test]
fn v5_triggers_follow_file_changes() {
    let w = world(10);
    let mut cat = w.lib.open_catalog().unwrap();
    let row = cat
        .read(|tx| tx.browse().page(&all(), sort(SortField::Date, false), 0, 1))
        .unwrap();
    let id = row[0].id;
    let files = cat.read(|tx| tx.files().for_asset(id)).unwrap();
    cat.write(|tx| tx.files().delete(files[0].id)).unwrap();
    let r = cat
        .read(|tx| {
            tx.browse().page(
                &SearchQuery {
                    volume: None,
                    ..all()
                },
                sort(SortField::Date, false),
                0,
                10,
            )
        })
        .unwrap()
        .into_iter()
        .find(|r| r.id == id)
        .unwrap();
    assert_eq!(
        (r.name, r.size),
        (None, None),
        "cleared when the original goes"
    );
}

#[test]
#[ignore = "timing probe"]
fn seed_100k_timing() {
    let t = Instant::now();
    let _w = world(100_000);
    eprintln!("seed 100k: {:?}", t.elapsed());
}

#[test]
#[ignore = "slow: seeds 300,000 assets"]
fn browse_300k_is_fast() {
    let t = Instant::now();
    let w = world(300_000);
    eprintln!("seed: {:?}", t.elapsed());
    let time = |what: &str, f: &dyn Fn()| {
        let t = Instant::now();
        f();
        let e = t.elapsed();
        eprintln!("{what}: {e:?}");
        assert!(e.as_millis() < 500, "{what} took {e:?}");
    };
    time("count", &|| {
        w.cat.read(|tx| tx.browse().count(&all())).unwrap();
    });
    for field in [
        SortField::Date,
        SortField::Name,
        SortField::Size,
        SortField::Rating,
    ] {
        // Opening a view: one sorted scan of every id.
        let t = Instant::now();
        let ids = w
            .cat
            .read(|tx| tx.browse().ordered_ids(&all(), sort(field, false)))
            .unwrap();
        let e = t.elapsed();
        eprintln!("open view by {field:?}: {e:?}");
        assert_eq!(ids.len(), 300_000);
        assert!(e.as_millis() < 1500, "opening a view took {e:?}");
        // Any page, however deep, is a lookup of 200 ids.
        time(&format!("last page by {field:?}"), &|| {
            let p = w.cat.read(|tx| tx.browse().rows(&ids[299_800..])).unwrap();
            assert_eq!(p.len(), 200);
        });
    }
    time("years", &|| {
        w.cat
            .read(|tx| tx.browse().calendar(&all(), CalendarLevel::Years))
            .unwrap();
    });
    time("filtered count", &|| {
        w.cat
            .read(|tx| {
                tx.browse().count(&SearchQuery {
                    camera: Some("7M4".into()),
                    min_rating: Some(3),
                    ..all()
                })
            })
            .unwrap();
    });
}

#[test]
fn sidebar_counts() {
    let w = world(800);
    let v = w.cat.read(|tx| tx.browse().volume_counts()).unwrap();
    assert_eq!(v.iter().map(|(_, n)| n).sum::<u64>(), 800);
    assert_eq!(v.len(), 2);
    assert!(w
        .cat
        .read(|tx| tx.browse().event_counts())
        .unwrap()
        .is_empty());
}

#[test]
fn scan_card_groups_and_knows_what_is_imported() {
    use keepsake_core::catalog::{ImportDiff, VolumeKind};
    use keepsake_core::ingest::{
        self, fault::FaultPlan, template::Template, PlanOptions, RunContext, Selection,
    };
    use std::sync::atomic::AtomicBool;
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path();
    let lib = Library::create(&root.join("lib")).unwrap();
    let card = root.join("card");
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    for f in ["xavc", "dcim"] {
        copy(&fixtures.join(f), &card);
    }
    let mut cat = lib.open_catalog().unwrap();
    let before = ingest::scan_card(&cat, &lib, &card, None).unwrap();
    assert_eq!(before.len(), 4, "{before:#?}");
    assert!(before.iter().all(|a| a.diff == ImportDiff::New));
    let clip = before
        .iter()
        .find(|a| a.files[0].ends_with("C0001.MP4"))
        .unwrap();
    assert!(clip.files.len() >= 3, "clip + XML + proxy: {clip:?}");
    let pair = before
        .iter()
        .find(|a| a.files.len() == 3 && a.files[0].ends_with(".JPG"))
        .unwrap();
    assert!(pair.files.iter().any(|f| f.ends_with(".ARW")));
    // Import one asset; it is then known.
    let dests: Vec<_> = ["a", "b"]
        .iter()
        .map(|d| ingest::destination_volume(&lib, &root.join(d), d, VolumeKind::External).unwrap())
        .collect();
    let opts = PlanOptions {
        template: Template::parse("{orig}").unwrap(),
        selection: Selection::Nothing,
        event: None,
        allow_same_filesystem: true,
        source_id: None,
    };
    let s = ingest::plan(&mut cat, &lib, None, &card, "Card", &dests, &opts).unwrap();
    ingest::select(&mut cat, s, std::slice::from_ref(&pair.key), true).unwrap();
    let cancel = AtomicBool::new(false);
    let mut progress = |_: &ingest::Progress| {};
    let fault = FaultPlan::none();
    let mut ctx = RunContext {
        prober: None,
        fault: &fault,
        cancel: &cancel,
        progress: &mut progress,
        retry_failed: false,
    };
    ingest::run(&mut cat, &lib, s, &mut ctx).unwrap();
    let after = ingest::scan_card(&cat, &lib, &card, None).unwrap();
    let known: Vec<_> = after
        .iter()
        .filter(|a| a.diff == ImportDiff::AlreadyImported)
        .collect();
    assert_eq!(known.len(), 1);
    assert_eq!(known[0].key, pair.key);
}

fn copy(src: &std::path::Path, dst: &std::path::Path) {
    std::fs::create_dir_all(dst).unwrap();
    for e in std::fs::read_dir(src).unwrap().flatten() {
        let to = dst.join(e.file_name());
        if e.path().is_dir() {
            copy(&e.path(), &to);
        } else {
            std::fs::copy(e.path(), to).unwrap();
        }
    }
}
