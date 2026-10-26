//! Turn command-line selections (files, folders, search filters) into assets.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::catalog::{AssetId, Catalog, SearchQuery};
use crate::library::{Library, LibraryError};
use crate::relpath::RelPath;

/// Assets selected by `paths` (catalogued files, or folders meaning every
/// asset with a file inside them) and/or `query`. Paths that match nothing
/// are returned in the second list.
pub fn resolve(
    cat: &Catalog,
    lib: &Library,
    paths: &[PathBuf],
    query: Option<&SearchQuery>,
) -> Result<(Vec<AssetId>, Vec<PathBuf>), LibraryError> {
    let mut out: BTreeSet<AssetId> = BTreeSet::new();
    let mut unmatched = Vec::new();
    for p in paths {
        let found = assets_under(cat, lib, p)?;
        if found.is_empty() {
            unmatched.push(p.clone());
        }
        out.extend(found);
    }
    if let Some(q) = query {
        out.extend(cat.read(|tx| tx.search().query(q))?);
    }
    Ok((out.into_iter().collect(), unmatched))
}

fn assets_under(cat: &Catalog, lib: &Library, p: &Path) -> Result<Vec<AssetId>, LibraryError> {
    let Ok(abs) = std::fs::canonicalize(p) else {
        return Ok(Vec::new());
    };
    if abs.is_file() {
        return Ok(lib
            .locate(cat, &abs)?
            .map(|f| vec![f.asset_id])
            .unwrap_or_default());
    }
    let mut out = BTreeSet::new();
    for v in cat.read(|tx| tx.volumes().list())? {
        let Some(mount) = crate::verify::mount_of(cat, lib, v.id) else {
            continue;
        };
        let mount = std::fs::canonicalize(&mount).unwrap_or(mount);
        let prefix = if abs == mount {
            None
        } else if let Ok(rel) = RelPath::from_fs(&mount, &abs) {
            Some(rel.key(v.case))
        } else {
            continue;
        };
        for f in cat.read(|tx| tx.files().for_volume(v.id))? {
            let k = f.rel_path.key(v.case);
            let inside = match &prefix {
                None => true,
                Some(pre) => {
                    k.len() > pre.len()
                        && k.starts_with(pre.as_str())
                        && k.as_bytes()[pre.len()] == b'/'
                }
            };
            if inside {
                out.insert(f.asset_id);
            }
        }
    }
    Ok(out.into_iter().collect())
}

/// Parse a time shift `[+|-]H:MM[:SS]` (hours may exceed 24) into seconds.
pub fn parse_shift(s: &str) -> Option<i64> {
    let s = s.trim();
    let (sign, body) = match s.as_bytes().first()? {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, s),
    };
    let parts: Vec<&str> = body.split(':').collect();
    if !(2..=3).contains(&parts.len())
        || parts
            .iter()
            .any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    let h: i64 = parts[0].parse().ok()?;
    let m: i64 = parts[1].parse().ok()?;
    let sec: i64 = parts.get(2).map_or(Some(0), |x| x.parse().ok())?;
    if m >= 60 || sec >= 60 || parts[1].len() != 2 || parts.get(2).is_some_and(|x| x.len() != 2) {
        return None;
    }
    Some(sign * (h * 3600 + m * 60 + sec))
}

#[cfg(test)]
mod tests {
    use super::parse_shift;

    #[test]
    fn shifts() {
        assert_eq!(parse_shift("+01:00:00"), Some(3600));
        assert_eq!(parse_shift("-0:30"), Some(-1800));
        assert_eq!(parse_shift("25:00:01"), Some(90_001));
        assert_eq!(parse_shift("+1:5"), None);
        assert_eq!(parse_shift("1:60"), None);
        assert_eq!(parse_shift("abc"), None);
        assert_eq!(parse_shift("+1:00:00:00"), None);
    }
}

/// The shift (seconds) that makes `reference` read `correct`, both on the
/// camera's local clock. The reference's UTC offset is kept as it is.
/// Compared in whole seconds, as people see times: a reference at
/// 13:02:11.6 told it was 14:02:11 moves by exactly one hour.
pub fn shift_for(reference: crate::catalog::CaptureTime, correct: chrono::NaiveDateTime) -> i64 {
    let local_ms = reference.utc_ms + i64::from(reference.utc_offset_min.unwrap_or(0)) * 60_000;
    correct.and_utc().timestamp() - local_ms.div_euclid(1000)
}

#[cfg(test)]
mod shift_tests {
    use super::*;
    use crate::catalog::CaptureTime;

    fn local(s: &str) -> chrono::NaiveDateTime {
        chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap()
    }

    #[test]
    fn offsets_from_a_reference_photo() {
        // The camera says 13:02:11 local at +09:00, which is 04:02:11 UTC.
        let r = CaptureTime {
            utc_ms: local("2024-06-15 04:02:11").and_utc().timestamp_millis(),
            utc_offset_min: Some(540),
        };
        assert_eq!(shift_for(r, local("2024-06-15 14:02:11")), 3600);
        assert_eq!(shift_for(r, local("2024-06-15 12:32:11")), -1800);
        assert_eq!(shift_for(r, local("2024-06-16 00:02:11")), 11 * 3600);
        assert_eq!(shift_for(r, local("2023-06-15 13:02:11")), -366 * 24 * 3600);
        // No offset known: the stored time is taken as local.
        let n = CaptureTime {
            utc_ms: r.utc_ms,
            utc_offset_min: None,
        };
        assert_eq!(shift_for(n, local("2024-06-15 04:02:12")), 1);
        // Sub-second capture times: the seconds shown are what count.
        let sub = CaptureTime {
            utc_ms: r.utc_ms + 600,
            utc_offset_min: Some(540),
        };
        assert_eq!(shift_for(sub, local("2024-06-15 13:02:11")), 0);
        assert_eq!(shift_for(sub, local("2024-06-15 14:02:11")), 3600);
        let before = CaptureTime {
            utc_ms: r.utc_ms - 400,
            utc_offset_min: Some(540),
        };
        assert_eq!(shift_for(before, local("2024-06-15 13:02:10")), 0);
    }
}
