//! A canonical, order-independent dump of the catalog's durable content.
//!
//! Two catalogs are equivalent when their snapshots are equal. Excluded on
//! purpose: renditions (a cache), import history, volume online state and
//! labels/tags that nothing uses (none of these can be rebuilt from files).

use serde::Serialize;

use super::error::CatalogError;
use super::fts;
use super::repo::{Assets, Events, Files, Recipes, Sidecars, Tags, Volumes};
use super::{
    Asset, AssetId, AssetTag, EditRecipe, Event, File, Ro, SidecarRecord, VolumeId, VolumeKind,
};
use crate::relpath::CaseSensitivity;

/// Volume fields that a rebuild must reproduce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VolumeSnapshot {
    pub id: VolumeId,
    pub label: String,
    pub fs_id: String,
    pub kind: VolumeKind,
    pub case: CaseSensitivity,
    pub read_only: bool,
}

/// One asset with everything attached to it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssetSnapshot {
    pub asset: Asset,
    pub files: Vec<File>,
    pub tags: Vec<AssetTag>,
    pub recipes: Vec<EditRecipe>,
    pub sidecars: Vec<SidecarRecord>,
}

/// One row of the full-text index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FtsRow {
    pub asset_id: AssetId,
    pub names: String,
    pub comment: String,
    pub tags: String,
}

/// See the module docs.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CatalogSnapshot {
    pub volumes: Vec<VolumeSnapshot>,
    pub events: Vec<Event>,
    pub assets: Vec<AssetSnapshot>,
    pub fts: Vec<FtsRow>,
}

pub(crate) fn take(conn: &rusqlite::Connection) -> Result<CatalogSnapshot, CatalogError> {
    let mut volumes: Vec<VolumeSnapshot> = Volumes::<Ro>::new(conn)
        .list()?
        .into_iter()
        .map(|v| VolumeSnapshot {
            id: v.id,
            label: v.label,
            fs_id: v.fs_id,
            kind: v.kind,
            case: v.case,
            read_only: v.read_only,
        })
        .collect();
    volumes.sort_by_key(|v| v.id);

    let mut events = Events::<Ro>::new(conn).list()?;
    events.sort_by_key(|e| e.id);

    let assets_repo = Assets::<Ro>::new(conn);
    let mut ids = assets_repo.list_ids()?;
    ids.sort();
    let mut assets = Vec::with_capacity(ids.len());
    let mut fts_rows = Vec::with_capacity(ids.len());
    for id in ids {
        let asset = assets_repo.require(id)?;
        let mut files = Files::<Ro>::new(conn).for_asset(id)?;
        files.sort_by(|a, b| (a.volume_id, &a.rel_path).cmp(&(b.volume_id, &b.rel_path)));
        let tags = Tags::<Ro>::new(conn).for_asset(id)?;
        let mut recipes = Recipes::<Ro>::new(conn).for_asset(id)?;
        recipes.sort_by_key(|r| r.id);
        let sidecars = Sidecars::<Ro>::new(conn).for_asset(id)?;
        if let Some(t) = fts::stored(conn, id)? {
            fts_rows.push(FtsRow {
                asset_id: id,
                names: t.names,
                comment: t.comment,
                tags: t.tags,
            });
        }
        assets.push(AssetSnapshot {
            asset,
            files,
            tags,
            recipes,
            sidecars,
        });
    }

    Ok(CatalogSnapshot {
        volumes,
        events,
        assets,
        fts: fts_rows,
    })
}
