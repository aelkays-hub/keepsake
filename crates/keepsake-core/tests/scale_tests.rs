//! Scale: a synthetic library of 300,000 assets. Ignored by default (it
//! seeds for about a minute); run in release:
//!
//! ```text
//! cargo test --release -p keepsake-core --test scale_tests -- --ignored --nocapture
//! ```
//!
//! Limits (prompt 12): every search 300 ms; the calendar 300 ms; the
//! catalog side of start-up (open, sidebar, first view and page, facets)
//! 1 s, leaving the rest of the 3 s for the window. `KEEPSAKE_SCALE_SLACK`
//! multiplies the limits (CI machines vary; CI uses 2).

use std::time::{Duration, Instant};

use keepsake_core::catalog::{CalendarLevel, FacetColumn, MediaKind, SearchQuery, Sort, SortField};
use keepsake_core::library::Library;
use keepsake_core::render::Cache;
use keepsake_core::seed::{seed, SeedOptions};

const ASSETS: u64 = 300_000;

fn slack() -> f64 {
    std::env::var("KEEPSAKE_SCALE_SLACK")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0)
}

fn limit(ms: u64) -> Duration {
    Duration::from_millis((ms as f64 * slack()) as u64)
}

/// Run `f` 3 times; the slowest run.
fn worst<T>(mut f: impl FnMut() -> T) -> (Duration, T) {
    let mut worst = Duration::ZERO;
    let mut out = None;
    for _ in 0..3 {
        let t = Instant::now();
        let v = f();
        worst = worst.max(t.elapsed());
        out = Some(v);
    }
    (worst, out.expect("ran"))
}

fn q() -> SearchQuery {
    SearchQuery::default()
}

#[test]
#[ignore = "scale: seeds 300,000 assets (run with --release)"]
fn scale_300k_meets_the_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().join("lib");
    {
        let lib = Library::create(&root).unwrap();
        let cache = Cache::new(tmp.path().join("cache"));
        let t = Instant::now();
        seed(
            &lib,
            &cache,
            &SeedOptions {
                assets: ASSETS,
                distinct: 16,
                offline_share: 0.25,
            },
        )
        .unwrap();
        eprintln!("seeded {ASSETS} assets in {:?}", t.elapsed());
    }
    let mut rows: Vec<(String, Duration, u64, Duration)> = Vec::new();
    let mut check = |what: &str, took: Duration, n: u64, max: Duration| {
        rows.push((what.to_owned(), took, n, max));
    };
    let by_date = Sort {
        field: SortField::Date,
        descending: true,
    };

    // ── Start-up, as the app does it (cold: a fresh process opens the library).
    let t = Instant::now();
    let lib = Library::open(&root).unwrap();
    let cat = lib.open_catalog().unwrap();
    let open = t.elapsed();
    let t = Instant::now();
    let total = cat.read(|tx| tx.assets().count()).unwrap();
    let vols = cat.read(|tx| tx.browse().volume_counts()).unwrap();
    cat.read(|tx| tx.browse().event_counts()).unwrap();
    let sidebar = t.elapsed();
    let t = Instant::now();
    let ids = cat
        .read(|tx| tx.browse().ordered_ids(&q(), by_date))
        .unwrap();
    let first = cat.read(|tx| tx.browse().rows(&ids[..200])).unwrap();
    let view = t.elapsed();
    let t = Instant::now();
    let cams = cat
        .read(|tx| tx.browse().facets(FacetColumn::Camera))
        .unwrap();
    let lenses = cat
        .read(|tx| tx.browse().facets(FacetColumn::Lens))
        .unwrap();
    let facets = t.elapsed();
    assert_eq!(total, ASSETS);
    assert_eq!(ids.len() as u64, ASSETS);
    assert_eq!(first.len(), 200);
    assert!(!vols.is_empty() && !cams.is_empty() && !lenses.is_empty());
    check("start-up: open catalog", open, 0, limit(1000));
    check("start-up: sidebar counts", sidebar, 0, limit(1000));
    check("start-up: first view + 200 rows", view, ASSETS, limit(1000));
    check("start-up: camera and lens lists", facets, 0, limit(1000));
    check(
        "start-up: catalog total",
        open + sidebar + view + facets,
        0,
        limit(1000),
    );

    // ── Searches: a view of the results plus their first page.
    let searches: Vec<(&str, SearchQuery)> = vec![
        (
            "words: lake",
            SearchQuery {
                text: Some("lake".into()),
                ..q()
            },
        ),
        (
            "words: family",
            SearchQuery {
                text: Some("family".into()),
                ..q()
            },
        ),
        (
            "words: sunset lake",
            SearchQuery {
                text: Some("sunset lake".into()),
                ..q()
            },
        ),
        (
            "prefix: dsc0",
            SearchQuery {
                text: Some("dsc0".into()),
                ..q()
            },
        ),
        (
            "one name: DSC01234",
            SearchQuery {
                text: Some("DSC01234".into()),
                ..q()
            },
        ),
        (
            "no match: wedding beach",
            SearchQuery {
                text: Some("wedding beach".into()),
                ..q()
            },
        ),
        (
            "camera + 4 stars",
            SearchQuery {
                camera: Some("ILCE-7M4".into()),
                min_rating: Some(4),
                ..q()
            },
        ),
        (
            "RAW only",
            SearchQuery {
                raw: Some(true),
                ..q()
            },
        ),
        (
            "videos",
            SearchQuery {
                kind: Some(MediaKind::Video),
                ..q()
            },
        ),
        (
            "label Red",
            SearchQuery {
                label: Some("Red".into()),
                ..q()
            },
        ),
        (
            "one year",
            SearchQuery {
                from: Some("2019-01-01".into()),
                to: Some("2019-12-31".into()),
                ..q()
            },
        ),
        (
            "words + lens + year",
            SearchQuery {
                text: Some("family".into()),
                lens: Some("FE 35mm F1.8".into()),
                from: Some("2018-01-01".into()),
                to: Some("2020-12-31".into()),
                ..q()
            },
        ),
        (
            "no date",
            SearchQuery {
                undated: true,
                ..q()
            },
        ),
    ];
    for (what, query) in &searches {
        for field in [SortField::Date, SortField::Name] {
            let sort = Sort {
                field,
                descending: field == SortField::Date,
            };
            let (took, n) = worst(|| {
                let ids = cat.read(|tx| tx.browse().ordered_ids(query, sort)).unwrap();
                let page = &ids[..ids.len().min(200)];
                cat.read(|tx| tx.browse().rows(page)).unwrap();
                ids.len() as u64
            });
            let label = format!(
                "search {what} (by {})",
                if field == SortField::Date {
                    "date"
                } else {
                    "name"
                }
            );
            check(&label, took, n, limit(300));
        }
    }

    // ── Calendar levels, all and filtered.
    for (what, query) in [
        ("all", q()),
        (
            "words: family",
            SearchQuery {
                text: Some("family".into()),
                ..q()
            },
        ),
    ] {
        for (lv, level) in [
            ("years", CalendarLevel::Years),
            ("months of 2020", CalendarLevel::Months { year: 2020 }),
            (
                "days of June 2020",
                CalendarLevel::Days {
                    year: 2020,
                    month: 6,
                },
            ),
        ] {
            let (took, n) = worst(|| {
                cat.read(|tx| tx.browse().calendar(&query, level))
                    .unwrap()
                    .len() as u64
            });
            check(&format!("calendar {lv} ({what})"), took, n, limit(300));
        }
    }

    eprintln!("\n| Operation | Items | Time (ms) | Limit (ms) |\n|---|---:|---:|---:|");
    let mut over = Vec::new();
    for (what, took, n, max) in &rows {
        eprintln!(
            "| {what} | {} | {:.0} | {} |",
            if *n > 0 { n.to_string() } else { String::new() },
            took.as_secs_f64() * 1000.0,
            max.as_millis()
        );
        if took > max {
            over.push(format!("{what}: {took:?} > {max:?}"));
        }
    }
    assert!(over.is_empty(), "over the limit:\n{}", over.join("\n"));
}
