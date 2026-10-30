//! Conversions between core types and the UI contract (`api`).

use std::collections::HashSet;
use std::path::Path;
use std::str::FromStr;

use keepsake_core::catalog::{
    AssetId, BrowseRow, CalendarLevel, EventId, ImportSessionId, MediaKind, SearchQuery, Sort,
    SortField, VolumeId,
};
use keepsake_core::render::{Cache, Variant};

use crate::api::{CalendarLevelDto, ItemKind, KindFilter, Query, Row, SortKey, SortSpec};

pub fn id<T: FromStr>(s: &str, what: &str) -> Result<T, String> {
    T::from_str(s).map_err(|_| format!("bad {what} id: {s}"))
}

pub fn ids(v: &[String]) -> Result<Vec<AssetId>, String> {
    v.iter().map(|s| id(s, "asset")).collect()
}

pub fn query(q: &Query) -> Result<SearchQuery, String> {
    let (kind, raw) = match q.kind {
        None => (None, None),
        Some(KindFilter::Photo) => (Some(MediaKind::Photo), None),
        Some(KindFilter::Video) => (Some(MediaKind::Video), None),
        Some(KindFilter::Raw) => (None, Some(true)),
    };
    Ok(SearchQuery {
        text: q.text.clone().filter(|t| !t.trim().is_empty()),
        from: q.from.clone().filter(|d| !d.is_empty()),
        to: q.to.clone().filter(|d| !d.is_empty()),
        camera: q.camera.clone().filter(|c| !c.is_empty()),
        lens: q.lens.clone().filter(|c| !c.is_empty()),
        kind,
        raw,
        min_rating: q.min_rating.filter(|r| *r > 0),
        label: q.label.clone().filter(|l| !l.is_empty()),
        event: q
            .event
            .as_deref()
            .map(|e| id::<EventId>(e, "event"))
            .transpose()?,
        volume: q
            .volume
            .as_deref()
            .map(|v| id::<VolumeId>(v, "volume"))
            .transpose()?,
        folder: q.folder.clone().filter(|f| !f.trim_matches('/').is_empty()),
        session: q
            .session
            .as_deref()
            .map(|s| id::<ImportSessionId>(s, "import"))
            .transpose()?,
        undated: q.undated,
        limit: None,
    })
}

pub fn sort(s: &SortSpec) -> Sort {
    Sort {
        field: match s.field {
            SortKey::Date => SortField::Date,
            SortKey::Name => SortField::Name,
            SortKey::Camera => SortField::Camera,
            SortKey::Lens => SortField::Lens,
            SortKey::Type => SortField::Type,
            SortKey::Size => SortField::Size,
            SortKey::Duration => SortField::Duration,
            SortKey::Rating => SortField::Rating,
        },
        descending: s.descending,
    }
}

pub fn level(l: CalendarLevelDto) -> CalendarLevel {
    match l {
        CalendarLevelDto::Years => CalendarLevel::Years,
        CalendarLevelDto::Months { year } => CalendarLevel::Months { year },
        CalendarLevelDto::Days { year, month } => CalendarLevel::Days { year, month },
    }
}

pub fn kind(k: MediaKind, raw: bool) -> ItemKind {
    match (k, raw) {
        (MediaKind::Video, _) => ItemKind::Video,
        (MediaKind::Photo, true) => ItemKind::Raw,
        (MediaKind::Photo, false) => ItemKind::Photo,
    }
}

/// Camera-clock local time, `YYYY-MM-DDTHH:MM:SS`.
pub fn local_time(utc_ms: Option<i64>, offset_min: Option<i16>) -> Option<String> {
    let ms = utc_ms? + i64::from(offset_min.unwrap_or(0)) * 60_000;
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.naive_utc().format("%Y-%m-%dT%H:%M:%S").to_string())
}

pub fn path_text(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\")
        .map(str::to_owned)
        .unwrap_or_else(|| s.into_owned())
}

/// The path of a cached rendition, if it exists (a file check; no catalog).
pub fn cached(
    cache: &Cache,
    hash: Option<&keepsake_core::catalog::Blake3Hash>,
    v: Variant,
) -> Option<String> {
    let rel = Cache::rel_path(hash?, v).ok()?;
    let full = rel.to_fs_path(cache.root());
    full.is_file().then(|| path_text(&full))
}

pub fn row(r: &BrowseRow, cache: &Cache, online: &HashSet<VolumeId>) -> Row {
    Row {
        id: r.id.to_string(),
        name: r.name.clone(),
        captured: local_time(r.capture_ms, r.utc_offset_min),
        camera: r.camera.clone(),
        lens: r.lens.clone(),
        kind: kind(r.kind, r.raw),
        size: r.size,
        duration_ms: r.duration_ms,
        rating: r.rating,
        label: r.label.clone(),
        thumb: cached(cache, r.source_hash.as_ref(), Variant::Thumb256),
        online: r.volumes.iter().any(|v| online.contains(v)),
    }
}
