//! Property tests for volume-relative paths: separators, Unicode normalisation
//! (NFC vs NFD as on macOS) and case-insensitive file systems.

mod common;

use proptest::prelude::*;
use unicode_normalization::UnicodeNormalization;

use keepsake_core::catalog::{
    Asset, AssetId, Blake3Hash, Catalog, CatalogError, FileRole, MediaKind, NewFile, Volume,
    VolumeId, VolumeKind,
};
use keepsake_core::relpath::{path_key, CaseSensitivity, RelPath};

/// A path component: letters (including accented and decomposable ones), digits,
/// spaces, dots and a few punctuation marks — never `.` or `..`.
fn component() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 _\\-éÉèçÇüÜöÖåÅñÑßøÆ\u{212B}\u{0301}\u{0308}가-힣ｱ-ﾝ.]{1,12}"
        .prop_filter("not a dot component", |s| s != "." && s != "..")
        .prop_filter("no leading combining mark", |s| {
            !s.starts_with(['\u{0301}', '\u{0308}'])
        })
}

fn components() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(component(), 1..6)
}

fn sep() -> impl Strategy<Value = &'static str> {
    prop_oneof![Just("/"), Just("\\"), Just("//"), Just("\\/")]
}

fn nfd(s: &str) -> String {
    s.nfd().collect()
}

fn nfc(s: &str) -> String {
    s.nfc().collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Windows `\` and POSIX `/` (and doubled separators) parse to the same path.
    #[test]
    fn separators_are_equivalent(parts in components(), seps in prop::collection::vec(sep(), 5)) {
        let canonical = parts.join("/");
        let mut mixed = String::new();
        for (i, p) in parts.iter().enumerate() {
            if i > 0 {
                mixed.push_str(seps[i % seps.len()]);
            }
            mixed.push_str(p);
        }
        let a = RelPath::parse(&canonical).unwrap();
        let b = RelPath::parse(&mixed).unwrap();
        prop_assert_eq!(&a, &b);
        prop_assert_eq!(a.as_str(), canonical.as_str());
        prop_assert_eq!(a.components().count(), parts.len());
    }

    /// Canonical form is a fixed point and survives the file-system round trip.
    #[test]
    fn canonical_round_trip(parts in components()) {
        let p = RelPath::parse(&parts.join("/")).unwrap();
        prop_assert_eq!(RelPath::parse(p.as_str()).unwrap(), p.clone());
        let root = std::path::Path::new("mnt").join("vol");
        let full = p.to_fs_path(&root);
        prop_assert_eq!(RelPath::from_fs(&root, &full).unwrap(), p.clone());
        let json = serde_json::to_string(&p).unwrap();
        prop_assert_eq!(serde_json::from_str::<RelPath>(&json).unwrap(), p);
    }

    /// `..`, absolute paths, drive letters and UNC prefixes are always rejected.
    #[test]
    fn escapes_are_rejected(parts in components(), at in 0usize..6, drive in "[A-Za-z]") {
        let mut with_dotdot = parts.clone();
        with_dotdot.insert(at.min(parts.len()), "..".into());
        prop_assert!(RelPath::parse(&with_dotdot.join("/")).is_err());
        let joined = parts.join("/");
        let bad = [
            format!("/{joined}"),
            format!("\\{joined}"),
            format!("{drive}:\\{joined}"),
            format!("\\\\server\\share\\{joined}"),
        ];
        for b in &bad {
            prop_assert!(RelPath::parse(b).is_err(), "{:?} accepted", b);
        }
    }

    /// NFC and NFD spellings (macOS HFS+ stores NFD) share one identity key on
    /// every volume type, while the on-disk spelling is kept exactly.
    #[test]
    fn nfc_and_nfd_share_a_key(parts in components()) {
        let s = parts.join("/");
        let a = RelPath::parse(&nfc(&s)).unwrap();
        let b = RelPath::parse(&nfd(&s)).unwrap();
        prop_assert_eq!(a.as_str(), nfc(&s));
        prop_assert_eq!(b.as_str(), nfd(&s));
        for case in [CaseSensitivity::Sensitive, CaseSensitivity::Insensitive] {
            prop_assert_eq!(a.key(case), b.key(case));
            // Keys are always NFC.
            let k = a.key(case);
            prop_assert_eq!(nfc(&k), k);
        }
        prop_assert_eq!(a.file_name_nfc(), b.file_name_nfc());
    }

    /// On case-insensitive volumes, case variants collide; on case-sensitive
    /// volumes they stay distinct.
    #[test]
    fn case_rules(parts in components()) {
        let s = parts.join("/");
        let upper = s.to_uppercase();
        let lower = s.to_lowercase();
        let ins = CaseSensitivity::Insensitive;
        prop_assert_eq!(path_key(&lower, ins), path_key(&s, ins));
        // Full uppercasing can change length (ß → SS); only compare when it round-trips.
        if upper.to_lowercase() == lower {
            prop_assert_eq!(path_key(&upper, ins), path_key(&s, ins));
        }
        if nfc(&upper) != nfc(&lower) {
            prop_assert_ne!(
                path_key(&upper, CaseSensitivity::Sensitive),
                path_key(&lower, CaseSensitivity::Sensitive)
            );
        }
    }
}

fn catalog_with(case: CaseSensitivity) -> (Catalog, VolumeId, AssetId) {
    let mut cat = Catalog::open_in_memory().unwrap();
    let v = Volume {
        id: VolumeId::new_random(),
        label: "v".into(),
        fs_id: "fs".into(),
        kind: VolumeKind::Internal,
        case,
        read_only: false,
        online: false,
        mount_path: None,
        last_seen_ms: None,
    };
    let a = Asset::new(AssetId::new_random(), MediaKind::Photo, 0);
    cat.write(|tx| {
        tx.volumes().upsert(&v)?;
        tx.assets().insert(&a)
    })
    .unwrap();
    (cat, v.id, a.id)
}

fn insert(cat: &mut Catalog, vol: VolumeId, asset: AssetId, rel: &str) -> Result<(), CatalogError> {
    let f = NewFile {
        volume_id: vol,
        rel_path: RelPath::parse(rel).unwrap(),
        role: FileRole::Sidecar,
        size_bytes: 0,
        blake3: Blake3Hash::of(b""),
        xxh3: None,
        mtime_ms: None,
    };
    cat.write(|tx| tx.files().insert(asset, &f).map(|_| ()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// The catalog's UNIQUE constraint enforces the same identity rules.
    #[test]
    fn catalog_identity_matches_file_system_rules(parts in components()) {
        let s = parts.join("/");

        // NFD then NFC: duplicate on both kinds of volume.
        for case in [CaseSensitivity::Sensitive, CaseSensitivity::Insensitive] {
            let (mut cat, v, a) = catalog_with(case);
            insert(&mut cat, v, a, &nfd(&s)).unwrap();
            let dup = insert(&mut cat, v, a, &nfc(&s));
            prop_assert!(matches!(dup, Err(CatalogError::Duplicate(_))));
            // A lookup in either spelling finds the stored file.
            let found = cat
                .read(|tx| tx.files().by_location(v, &RelPath::parse(&nfc(&s)).unwrap()))
                .unwrap();
            prop_assert_eq!(found.map(|f| f.rel_path.as_str().to_owned()), Some(nfd(&s)));
        }

        // Case variants.
        let upper = s.to_uppercase();
        let lower = s.to_lowercase();
        if nfc(&upper) != nfc(&lower) && upper.to_lowercase() == lower {
            let (mut cat, v, a) = catalog_with(CaseSensitivity::Insensitive);
            insert(&mut cat, v, a, &upper).unwrap();
            prop_assert!(matches!(insert(&mut cat, v, a, &lower), Err(CatalogError::Duplicate(_))));

            let (mut cat, v, a) = catalog_with(CaseSensitivity::Sensitive);
            insert(&mut cat, v, a, &upper).unwrap();
            prop_assert!(insert(&mut cat, v, a, &lower).is_ok());
        }
    }
}
