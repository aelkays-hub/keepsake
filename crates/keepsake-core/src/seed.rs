//! A synthetic library for UI tests and profiling (`keepsake dev seed`).
//!
//! Adds `assets` catalogue entries (photos, RAW+JPEG pairs, clips) spread
//! over ten years, several cameras and lenses, with ratings and labels. No
//! media files are written: the originals live on two volumes, one of them
//! "unplugged" (its folder does not exist), so offline badges show. Assets
//! share `distinct` source hashes, and those get real thumbnails and
//! previews in the cache, so a 300,000-item grid shows pictures while the
//! cache holds only `distinct` × 3 small JPEGs.
//!
//! For realistic text search, half of the assets get keywords and about a
//! fifth a comment, from a small vocabulary, in the search index.
//!
//! Never run this on a real library: it is for throwaway test libraries.

use std::path::PathBuf;

use image::{DynamicImage, Rgb, RgbImage};
use serde::Serialize;

use crate::catalog::{AssetId, Blake3Hash, CatalogError, FileId, MediaKind, VolumeId, VolumeKind};
use crate::library::{Library, LibraryError, VolumeRecord};
use crate::relpath::CaseSensitivity;
use crate::render::{still, Cache, RenderError, Variant};

#[derive(Debug, Clone, PartialEq)]
pub struct SeedOptions {
    pub assets: u64,
    /// Distinct pictures (source hashes) shared by the assets.
    pub distinct: u32,
    /// Share of assets whose only copy is on the unplugged volume.
    pub offline_share: f64,
}

impl Default for SeedOptions {
    fn default() -> Self {
        Self {
            assets: 1000,
            distinct: 64,
            offline_share: 0.25,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeedReport {
    pub assets: u64,
    pub online_volume: VolumeId,
    pub offline_volume: VolumeId,
    pub renditions: u64,
}

const CAMERAS: [&str; 4] = ["ILCE-7M4", "ILCE-6000", "ZV-E10", "ILCE-1"];
const LENSES: [&str; 5] = [
    "FE 24-70mm F2.8 GM II",
    "E 16-50mm F3.5-5.6 OSS",
    "FE 35mm F1.8",
    "FE 70-200mm F2.8 GM OSS II",
    "FE 20mm F1.8 G",
];
const LABELS: [&str; 5] = ["Red", "Yellow", "Green", "Blue", "Purple"];

/// A small deterministic random sequence (xorshift).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn picture(i: u32, w: u32, h: u32) -> DynamicImage {
    // A two-colour diagonal gradient, different for each `i`.
    let hue = |k: u32| -> [u8; 3] {
        let t = (k * 47) % 360;
        let f = |o: u32| -> u8 {
            let x = ((t + o) % 360) as f32;
            let v = if x < 120.0 {
                1.0 - x / 120.0
            } else if x < 240.0 {
                0.0
            } else {
                (x - 240.0) / 120.0
            };
            (60.0 + v * 180.0) as u8
        };
        [f(0), f(120), f(240)]
    };
    let (a, b) = (hue(i), hue(i + 3));
    let img = RgbImage::from_fn(w, h, |x, y| {
        let t = (x + y) as f32 / (w + h) as f32;
        let mix = |p: u8, q: u8| (p as f32 * (1.0 - t) + q as f32 * t) as u8;
        Rgb([mix(a[0], b[0]), mix(a[1], b[1]), mix(a[2], b[2])])
    });
    DynamicImage::ImageRgb8(img)
}

fn volume(lib: &Library, label: &str, mount: PathBuf) -> Result<VolumeRecord, LibraryError> {
    let rec = VolumeRecord {
        id: VolumeId::new_random(),
        label: label.into(),
        fs_id: format!("seed:{}", uuid::Uuid::new_v4()),
        kind: VolumeKind::External,
        case: CaseSensitivity::Insensitive,
        read_only: false,
        last_mount: Some(mount),
    };
    lib.register_volume(&rec)?;
    Ok(rec)
}

/// Fill the library with synthetic assets (see the module docs).
pub fn seed(lib: &Library, cache: &Cache, opts: &SeedOptions) -> Result<SeedReport, RenderError> {
    let mut cat = lib.open_catalog()?;
    let online_dir = lib.root().join("seed-online");
    std::fs::create_dir_all(&online_dir)?;
    let online = volume(lib, "Seed photos", online_dir.clone())?;
    let offline = volume(
        lib,
        "Seed archive (unplugged)",
        lib.root().join("seed-unplugged"),
    )?;
    cat.write(|tx| -> Result<(), CatalogError> {
        tx.volumes().upsert(&online.to_volume(Some(&online_dir)))?;
        tx.volumes().upsert(&offline.to_volume(None))?;
        Ok(())
    })?;

    // Pictures for the shared source hashes.
    let distinct = opts.distinct.max(1);
    let mut hashes = Vec::with_capacity(distinct as usize);
    let mut renditions = 0u64;
    for i in 0..distinct {
        let hash = Blake3Hash::of(format!("keepsake-seed-{i}").as_bytes());
        hashes.push(hash);
        let (w, h) = if i % 5 == 0 {
            (1067, 1600)
        } else {
            (1600, 1067)
        };
        let full = picture(i, w, h);
        let preview = still::fit(&full, 2560)?;
        let t1024 = still::fit(&preview, 1024)?;
        let t256 = still::fit(&t1024, 256)?;
        for (v, img) in [
            (Variant::Preview2560, &preview),
            (Variant::Thumb1024, &t1024),
            (Variant::Thumb256, &t256),
        ] {
            cache.put_bytes(&mut cat, &hash, v, &still::encode_jpeg(img)?)?;
            renditions += 1;
        }
    }

    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    let start = chrono::NaiveDate::from_ymd_opt(2015, 1, 1)
        .and_then(|d| d.and_hms_opt(8, 0, 0))
        .map(|d| d.and_utc().timestamp_millis())
        .unwrap_or(0);
    let span_ms: u64 = 10 * 365 * 24 * 3600 * 1000;
    let now = crate::jobs::now_ms();
    const BATCH: u64 = 20_000;
    let mut done = 0u64;
    // Bulk path: plain inserts (the v5 triggers still fill name, size and
    // RAW), with the text index written once per batch at the end.
    // A large page cache: random ids touch every page of every index.
    cat.write(|tx| {
        tx.conn()
            .execute_batch("PRAGMA cache_size = -262144")
            .map_err(CatalogError::from)
    })?;
    while done < opts.assets {
        let n = BATCH.min(opts.assets - done);
        cat.write(|tx| -> Result<(), CatalogError> {
            let c = tx.conn();
            let first_seq: i64 = c.query_row("SELECT ifnull(max(seq), 0) FROM assets", [], |r| r.get(0))?;
            let mut ins_a = c.prepare_cached(
                "INSERT INTO assets (id, media_kind, capture_ts_ms, capture_utc_offset_min, duration_ms,
                                     camera_make, camera_model, lens_model, rating, label_id,
                                     created_ms, metadata_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, 'SONY', ?6, ?7, ?8, ?9, ?10, ?10)",
            )?;
            let mut ins_f = c.prepare_cached(
                "INSERT INTO files (id, asset_id, volume_id, rel_path, rel_path_key, file_name, role,
                                    size_bytes, blake3, mtime_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'original', ?7, ?8, ?9)",
            )?;
            for k in 0..n {
                let i = done + k;
                let roll = rng.below(100);
                let kind = if roll < 15 { MediaKind::Video } else { MediaKind::Photo };
                let raw = kind == MediaKind::Photo && roll >= 85;
                let id = AssetId::new_random();
                // Sorted-ish capture times; a few undated.
                let t = start + (span_ms * i / opts.assets.max(1)) as i64 + rng.below(3_600_000) as i64;
                let capture = (rng.below(500) != 0).then_some(t);
                let camera = CAMERAS[rng.below(4) as usize];
                let lens = LENSES[rng.below(5) as usize];
                let rating = match rng.below(10) {
                    0..=5 => 0,
                    6 => 3,
                    7 => 4,
                    8 => 5,
                    _ => 1 + rng.below(2) as i64,
                };
                let label = (rng.below(12) == 0).then(|| 1 + rng.below(LABELS.len() as u64) as i64);
                let duration = (kind == MediaKind::Video).then(|| 2_000 + rng.below(600_000) as i64);
                ins_a.execute(rusqlite::params![
                    id,
                    kind,
                    capture,
                    capture.map(|_| 60i64),
                    duration,
                    camera,
                    lens,
                    rating,
                    label,
                    now
                ])?;
                let hash = hashes[(i % u64::from(distinct)) as usize];
                let vol = if (rng.below(1000) as f64) < opts.offline_share * 1000.0 {
                    offline.id
                } else {
                    online.id
                };
                let (name, size) = match kind {
                    MediaKind::Video => (format!("C{:05}.MP4", i % 100_000), 50_000_000 + rng.below(2_000_000_000)),
                    MediaKind::Photo if raw => (format!("DSC{:05}.ARW", i % 100_000), 24_000_000 + rng.below(30_000_000)),
                    MediaKind::Photo => (format!("DSC{:05}.JPG", i % 100_000), 4_000_000 + rng.below(16_000_000)),
                };
                let rel = format!("{}/{:06}/{name}", i / 1000, i);
                let key = rel.to_lowercase();
                ins_f.execute(rusqlite::params![
                    FileId::for_location(vol, &key),
                    id,
                    vol,
                    rel,
                    key,
                    name,
                    size as i64,
                    hash,
                    t
                ])?;
            }
            c.execute(
                "INSERT INTO asset_fts (rowid, names, comment, tags)
                 SELECT a.seq, f.file_name,
                        CASE a.seq % 13
                          WHEN 0 THEN 'Sunset at the lake'
                          WHEN 1 THEN 'Birthday party in the garden'
                          WHEN 2 THEN 'First snow of the winter'
                          ELSE '' END,
                        CASE a.seq % 10
                          WHEN 0 THEN 'Places Lake Family'
                          WHEN 1 THEN 'Places Beach'
                          WHEN 2 THEN 'Places Mountains Hiking'
                          WHEN 3 THEN 'People Anna'
                          WHEN 4 THEN 'Events Wedding Family'
                          ELSE '' END
                   FROM assets a
                   JOIN files f ON f.asset_id = a.id AND f.role = 'original'
                  WHERE a.seq > ?1",
                [first_seq],
            )?;
            Ok(())
        })?;
        done += n;
    }
    Ok(SeedReport {
        assets: opts.assets,
        online_volume: online.id,
        offline_volume: offline.id,
        renditions,
    })
}
