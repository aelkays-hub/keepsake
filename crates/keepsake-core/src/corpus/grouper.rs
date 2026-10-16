use std::collections::BTreeMap;

use crate::corpus::types::{AssetGroup, Expected, FileEntry, FolderRole};

/// Group a camera card's file list into logical assets.
///
/// Uses path-only heuristics; no file content is read.
pub fn group(camera: &str, files: &[FileEntry]) -> Expected {
    let mut buckets: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for f in files {
        let key = group_key(f);
        buckets.entry(key).or_default().push(f.rel_path.clone());
    }

    let asset = buckets
        .into_values()
        .map(|mut files| {
            files.sort();
            AssetGroup {
                camera: camera.to_owned(),
                files,
                capture_ts: String::new(),
                camera_model: String::new(),
                duration_ms: None,
                confirmed: false,
            }
        })
        .collect();

    Expected { asset }
}

fn group_key(f: &FileEntry) -> String {
    let filename = f.rel_path.split('/').next_back().unwrap_or(&f.rel_path);
    let stem = std::path::Path::new(filename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(filename);

    match &f.folder_role {
        // XAVC: C0001.MP4, C0001M01.XML → "xavc:C0001"
        //        C0001S01.MP4 (proxy)    → "xavc:C0001"
        FolderRole::Clip | FolderRole::Proxy => {
            format!("xavc:{}", xavc_base_stem(stem))
        }
        // AVCHD: 00001.MTS + 00001.CPI → "avchd:00001"
        //        00001.MPL               → "avchd:00001"
        FolderRole::Stream | FolderRole::ClipInfo | FolderRole::Playlist => {
            format!("avchd:{stem}")
        }
        // INDEX.BDM, MOVIEOBJ.BDM — a single BDMV metadata group per card.
        FolderRole::BdmvRoot => "avchd:bdmv_meta".to_owned(),
        // DCIM: group by full stem (DSC00001 across JPG + ARW + HIF).
        FolderRole::Dcim => format!("dcim:{stem}"),
        // MP_ROOT: each file is its own asset.
        FolderRole::MpRoot => format!("mp_root:{}", f.rel_path),
        FolderRole::Unknown => format!("unknown:{}", f.rel_path),
    }
}

/// Strip the XAVC sidecar / proxy suffix to get the base clip stem.
///
/// `C0001M01` → `C0001`  (XML sidecar: base + M + 2 digits)
/// `C0001S01` → `C0001`  (proxy:       base + S + 2 digits)
/// `C0001`    → `C0001`  (main clip: no suffix)
fn xavc_base_stem(stem: &str) -> &str {
    if stem.len() == 8 {
        let b = stem.as_bytes();
        let suffix_char = b[5] as char;
        if matches!(suffix_char, 'M' | 'S') && b[6].is_ascii_digit() && b[7].is_ascii_digit() {
            return &stem[..5];
        }
    }
    stem
}

#[cfg(test)]
mod tests {
    use super::xavc_base_stem;

    #[test]
    fn xavc_base_stem_strips_sidecar() {
        assert_eq!(xavc_base_stem("C0001M01"), "C0001");
    }

    #[test]
    fn xavc_base_stem_strips_proxy() {
        assert_eq!(xavc_base_stem("C0001S01"), "C0001");
    }

    #[test]
    fn xavc_base_stem_passthrough() {
        assert_eq!(xavc_base_stem("C0001"), "C0001");
        assert_eq!(xavc_base_stem("C0001X01"), "C0001X01"); // unknown suffix
    }
}
