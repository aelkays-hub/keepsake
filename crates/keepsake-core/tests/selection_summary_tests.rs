//! What a multi-selection has in common (the inspector's combined view).

mod common;

use keepsake_core::catalog::{AssetId, Rating, Shared, TagPath, VolumeKind};
use keepsake_core::library::Library;
use keepsake_core::metadata::{selection_summary, MetadataService};
use keepsake_core::rebuild::add_volume;

fn tag(s: &str) -> TagPath {
    TagPath::parse_lightroom(s).unwrap()
}

#[test]
fn summary_shows_shared_and_mixed_values() {
    let tmp = tempfile::tempdir().unwrap();
    let lib = Library::create(&tmp.path().join("lib")).unwrap();
    let media = tmp.path().join("media");
    // Two photos and two clips.
    common::copy_dir(&common::fixture("dcim"), &media);
    common::copy_dir(&common::fixture("xavc"), &media);
    let mut cat = lib.open_catalog().unwrap();
    add_volume(
        &mut cat,
        &lib,
        "M",
        VolumeKind::External,
        &media,
        false,
        None,
    )
    .unwrap();
    let ids = cat.read(|tx| tx.assets().list_ids()).unwrap();
    assert!(ids.len() >= 3, "the fixtures need three assets");
    let ids: Vec<AssetId> = ids[..3].to_vec();

    {
        let mut svc = MetadataService::new(&lib, &mut cat);
        for id in &ids {
            svc.set_rating(*id, Rating::new(4).unwrap()).unwrap();
            svc.set_comment(*id, Some("Holiday")).unwrap();
            svc.set_user_tags(*id, &[tag("Places|Lake")]).unwrap();
        }
        svc.set_label(ids[0], Some("Red")).unwrap();
        svc.set_user_tags(ids[1], &[tag("Places|Lake"), tag("People|Ann")])
            .unwrap();
    }

    // Duplicates and unknown ids do not change the count.
    let mut asked = ids.clone();
    asked.push(ids[0]);
    asked.push(AssetId::new_random());
    let s = selection_summary(&cat, &asked).unwrap();
    assert_eq!(s.fields.count, 3);
    assert_eq!(s.fields.rating, Shared::Same(4));
    assert_eq!(s.fields.comment, Shared::Same(Some("Holiday".into())));
    assert_eq!(s.fields.label, Shared::Mixed);
    assert_eq!(
        s.keywords,
        vec![(tag("People|Ann"), 1), (tag("Places|Lake"), 3)]
    );

    MetadataService::new(&lib, &mut cat)
        .set_rating(ids[2], Rating::new(1).unwrap())
        .unwrap();
    let s = selection_summary(&cat, &ids).unwrap();
    assert_eq!(s.fields.rating, Shared::Mixed);

    // One item: everything is its own.
    let s = selection_summary(&cat, &ids[..1]).unwrap();
    assert_eq!(s.fields.label, Shared::Same(Some("Red".into())));
    assert_eq!(s.keywords, vec![(tag("Places|Lake"), 1)]);

    // Nothing selected.
    let s = selection_summary(&cat, &[]).unwrap();
    assert_eq!(s.fields.count, 0);
    assert_eq!(s.fields.rating, Shared::Empty);
    assert!(s.keywords.is_empty());
}

#[test]
fn shared_folds_values() {
    let mut s = Shared::default();
    s.add(1);
    assert_eq!(s, Shared::Same(1));
    s.add(1);
    assert_eq!(s, Shared::Same(1));
    s.add(2);
    assert_eq!(s, Shared::Mixed);
    s.add(1);
    assert_eq!(s, Shared::Mixed);
}
