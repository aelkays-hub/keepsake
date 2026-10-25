//! User-metadata edits that keep the catalog and XMP sidecars in step.
//!
//! Every edit runs in one catalog write transaction: the rows are updated, the
//! asset's sidecars are rewritten atomically, and only then does the
//! transaction commit. If a sidecar write fails, the transaction rolls back.
//! If the commit fails after a sidecar was written, the sidecar (the source of
//! truth) is ahead and a rebuild restores the catalog from it.

use std::path::{Path, PathBuf};

use crate::catalog::{
    now_ms, Asset, AssetId, AssetTag, Blake3Hash, Catalog, EditRecipe, Event, NewFile, Rating,
    RecipeId, SidecarLocation, SidecarRecord, TagPath, TagSource, Volume, WriteTx,
};
use crate::library::{write_bytes_atomic, Library, LibraryError, VolumeRecord};
use crate::relpath::RelPath;
use crate::xmp::{sidecar, AssetXmp, FileRef, RecipeRef, SequenceRef, XmpPacket};

/// Applies user edits to a library.
pub struct MetadataService<'a> {
    lib: &'a Library,
    cat: &'a mut Catalog,
}

/// Where a sidecar for a volume copy should be written.
pub(crate) fn sidecar_target(
    lib: &Library,
    vol: &Volume,
    original: &RelPath,
) -> Result<(SidecarLocation, RelPath, PathBuf), LibraryError> {
    let rel = sidecar::beside_rel(original)?;
    match (&vol.mount_path, vol.online && !vol.read_only) {
        (Some(mount), true) => {
            let path = rel.to_fs_path(mount);
            Ok((SidecarLocation::Beside, rel, path))
        }
        _ => {
            let root = sidecar::mirror_root(&lib.sidecars_dir(), vol.id);
            let path = rel.to_fs_path(&root);
            Ok((SidecarLocation::Mirror, rel, path))
        }
    }
}

/// Project an asset's catalog state onto the sidecar for one volume copy.
pub(crate) fn project(
    tx: &WriteTx<'_>,
    asset: &Asset,
    volume: &Volume,
) -> Result<AssetXmp, LibraryError> {
    let tags = tx.tags().for_asset(asset.id)?;
    let files = tx.files().for_asset(asset.id)?;
    let recipes = tx.recipes().for_asset(asset.id)?;
    let sequence = tx
        .sequences()
        .for_asset(asset.id)?
        .map(|(s, position)| SequenceRef {
            id: s.id,
            kind: s.kind,
            position,
        });
    let event = match asset.event_id {
        Some(e) => tx.events().get(e)?,
        None => None,
    };
    Ok(AssetXmp {
        asset_id: Some(asset.id),
        media_kind: Some(asset.media_kind),
        added_ms: Some(asset.created_ms),
        metadata_ms: Some(asset.metadata_ms),
        rating: Some(asset.rating),
        label: asset.label.clone(),
        comment: asset.comment.clone(),
        user_tags: tags
            .iter()
            .filter(|t| t.source == TagSource::User)
            .map(|t| t.path.clone())
            .collect(),
        ai_tags: tags
            .iter()
            .filter(|t| t.source == TagSource::Ai)
            .map(|t| (t.path.clone(), t.confidence.unwrap_or(0.0)))
            .collect(),
        capture: asset.capture,
        capture_shift_s: asset.capture_shift_s,
        duration_ms: asset.duration_ms,
        camera_make: asset.camera_make.clone(),
        camera_model: asset.camera_model.clone(),
        lens_model: asset.lens_model.clone(),
        event,
        recipes: recipes
            .into_iter()
            .map(|r| RecipeRef {
                id: r.id,
                kind: r.kind,
                schema_version: r.schema_version,
                rel_path: r.rel_path,
                content_hash: r.content_hash,
                created_ms: r.created_ms,
                updated_ms: r.updated_ms,
            })
            .collect(),
        files: files
            .into_iter()
            .filter(|f| f.volume_id == volume.id)
            .map(|f| FileRef {
                rel_path: f.rel_path,
                role: f.role,
                segment_index: f.segment_index,
            })
            .collect(),
        capture_source: asset.capture_source,
        capture_confidence: asset.capture_confidence,
        gps: asset.gps,
        sequence,
    })
}

/// Rewrite every sidecar of an asset (one per volume holding an original).
pub(crate) fn sync_sidecars(
    tx: &WriteTx<'_>,
    lib: &Library,
    id: AssetId,
) -> Result<(), LibraryError> {
    let asset = tx.assets().require(id)?;
    for original in tx.files().originals(id)? {
        let vol = tx.volumes().require(original.volume_id)?;
        let (location, rel, path) = sidecar_target(lib, &vol, &original.rel_path)?;
        let mut packet = match sidecar::read(&path)? {
            Some(existing) => existing.packet,
            None => XmpPacket::new(),
        };
        project(tx, &asset, &vol)?.write(&mut packet);
        let (blake3, mtime_ms) = sidecar::write_atomic(&path, &packet)?;
        tx.sidecars().upsert(&SidecarRecord {
            asset_id: id,
            volume_id: vol.id,
            location,
            rel_path: rel,
            blake3,
            mtime_ms,
        })?;
    }
    Ok(())
}

impl<'a> MetadataService<'a> {
    pub fn new(lib: &'a Library, cat: &'a mut Catalog) -> Self {
        Self { lib, cat }
    }

    /// Register a volume in the library and the catalog. Pass `mount` when it is online.
    pub fn register_volume(
        &mut self,
        rec: &VolumeRecord,
        mount: Option<&Path>,
    ) -> Result<Volume, LibraryError> {
        self.lib.register_volume(rec)?;
        let v = rec.to_volume(mount);
        self.cat
            .write(|tx| -> Result<(), LibraryError> { Ok(tx.volumes().upsert(&v)?) })?;
        Ok(v)
    }

    /// Catalogue a new asset with its files and write its sidecars.
    pub fn add_asset(&mut self, asset: &Asset, files: &[NewFile]) -> Result<(), LibraryError> {
        let lib = self.lib;
        self.cat.write(|tx| -> Result<(), LibraryError> {
            tx.assets().insert(asset)?;
            for f in files {
                tx.files().insert(asset.id, f)?;
            }
            sync_sidecars(tx, lib, asset.id)
        })
    }

    /// Load the asset, let `f` change it, bump `metadata_ms`, save, sync sidecars.
    fn edit(
        &mut self,
        id: AssetId,
        f: impl FnOnce(&WriteTx<'_>, &mut Asset) -> Result<(), LibraryError>,
    ) -> Result<Asset, LibraryError> {
        let lib = self.lib;
        self.cat.write(|tx| -> Result<Asset, LibraryError> {
            let mut a = tx.assets().require(id)?;
            f(tx, &mut a)?;
            a.metadata_ms = now_ms().max(a.metadata_ms + 1);
            tx.assets().update(&a)?;
            sync_sidecars(tx, lib, id)?;
            Ok(a)
        })
    }

    pub fn set_rating(&mut self, id: AssetId, rating: Rating) -> Result<Asset, LibraryError> {
        self.edit(id, |_, a| {
            a.rating = rating;
            Ok(())
        })
    }

    pub fn set_label(&mut self, id: AssetId, label: Option<&str>) -> Result<Asset, LibraryError> {
        let label = label
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned);
        self.edit(id, |_, a| {
            a.label = label;
            Ok(())
        })
    }

    pub fn set_comment(
        &mut self,
        id: AssetId,
        comment: Option<&str>,
    ) -> Result<Asset, LibraryError> {
        let comment = comment.filter(|c| !c.trim().is_empty()).map(str::to_owned);
        self.edit(id, |_, a| {
            a.comment = comment;
            Ok(())
        })
    }

    /// Replace the asset's user tags.
    /// The asset's user keywords (not AI tags).
    pub fn user_tags(&self, id: AssetId) -> Result<Vec<TagPath>, LibraryError> {
        Ok(self
            .cat
            .read(|tx| tx.tags().for_asset(id))?
            .into_iter()
            .filter(|t| t.source == crate::catalog::TagSource::User)
            .map(|t| t.path)
            .collect())
    }

    pub fn set_user_tags(&mut self, id: AssetId, tags: &[TagPath]) -> Result<Asset, LibraryError> {
        let tags: Vec<AssetTag> = tags.iter().cloned().map(AssetTag::user).collect();
        self.edit(id, |tx, _| {
            tx.tags().set_for_asset(id, TagSource::User, &tags)?;
            tx.tags().prune_unused()?;
            Ok(())
        })
    }

    /// Replace the asset's AI tags (confidence 0–1).
    pub fn set_ai_tags(
        &mut self,
        id: AssetId,
        tags: &[(TagPath, f64)],
    ) -> Result<Asset, LibraryError> {
        let tags: Vec<AssetTag> = tags
            .iter()
            .map(|(p, c)| AssetTag::ai(p.clone(), *c))
            .collect();
        self.edit(id, |tx, _| {
            tx.tags().set_for_asset(id, TagSource::Ai, &tags)?;
            tx.tags().prune_unused()?;
            Ok(())
        })
    }

    /// Shift the capture time by `delta_s` seconds (cumulative). Errors when the
    /// asset has no capture time.
    pub fn shift_capture_time(&mut self, id: AssetId, delta_s: i64) -> Result<Asset, LibraryError> {
        self.edit(id, |_, a| {
            let c = a
                .capture
                .as_mut()
                .ok_or_else(|| LibraryError::Invalid(format!("asset {id} has no capture time")))?;
            c.utc_ms += delta_s * 1000;
            a.capture_shift_s += delta_s;
            Ok(())
        })
    }

    /// Create or update an event and put `assets` in it.
    pub fn set_event(&mut self, event: &Event, assets: &[AssetId]) -> Result<(), LibraryError> {
        let lib = self.lib;
        self.cat.write(|tx| -> Result<(), LibraryError> {
            tx.events().upsert(event)?;
            tx.events().assign(assets, Some(event.id))?;
            let mut affected = tx.assets().in_event(event.id)?;
            affected.extend_from_slice(assets);
            affected.sort();
            affected.dedup();
            for id in affected {
                let mut a = tx.assets().require(id)?;
                a.metadata_ms = now_ms().max(a.metadata_ms + 1);
                tx.assets().update(&a)?;
                sync_sidecars(tx, lib, id)?;
            }
            tx.events().delete_empty()?;
            Ok(())
        })
    }

    /// Take assets out of whatever event they are in.
    pub fn clear_event(&mut self, assets: &[AssetId]) -> Result<(), LibraryError> {
        let lib = self.lib;
        self.cat.write(|tx| -> Result<(), LibraryError> {
            tx.events().assign(assets, None)?;
            for &id in assets {
                let mut a = tx.assets().require(id)?;
                a.metadata_ms = now_ms().max(a.metadata_ms + 1);
                tx.assets().update(&a)?;
                sync_sidecars(tx, lib, id)?;
            }
            tx.events().delete_empty()?;
            Ok(())
        })
    }

    /// Save a new edit recipe for an asset.
    pub fn add_recipe(
        &mut self,
        asset: AssetId,
        kind: &str,
        schema_version: u32,
        json: &serde_json::Value,
    ) -> Result<EditRecipe, LibraryError> {
        let id = RecipeId::new_random();
        let rel = RelPath::parse(&format!("{asset}/{id}.json"))?;
        let now = now_ms();
        let recipe = EditRecipe {
            id,
            asset_id: asset,
            kind: kind.to_owned(),
            schema_version,
            rel_path: rel,
            content_hash: Blake3Hash::of(&[]),
            created_ms: now,
            updated_ms: now,
        };
        self.save_recipe(recipe, json)
    }

    /// Replace the content of an existing recipe.
    pub fn update_recipe(
        &mut self,
        id: RecipeId,
        json: &serde_json::Value,
    ) -> Result<EditRecipe, LibraryError> {
        let mut r = self
            .cat
            .read(|tx| tx.recipes().get(id))?
            .ok_or_else(|| LibraryError::Invalid(format!("unknown recipe {id}")))?;
        r.updated_ms = now_ms().max(r.updated_ms + 1);
        self.save_recipe(r, json)
    }

    fn save_recipe(
        &mut self,
        mut recipe: EditRecipe,
        json: &serde_json::Value,
    ) -> Result<EditRecipe, LibraryError> {
        let bytes = serde_json::to_vec_pretty(json)?;
        recipe.content_hash = Blake3Hash::of(&bytes);
        let path = recipe.rel_path.to_fs_path(&self.lib.recipes_dir());
        write_bytes_atomic(&path, &bytes)?;
        let r2 = recipe.clone();
        self.edit(recipe.asset_id, move |tx, _| {
            Ok(tx.recipes().upsert(&r2)?)
        })?;
        Ok(recipe)
    }

    /// Remove a recipe reference and delete its JSON file from the library.
    pub fn remove_recipe(&mut self, id: RecipeId) -> Result<(), LibraryError> {
        let Some(r) = self.cat.read(|tx| tx.recipes().get(id))? else {
            return Ok(());
        };
        self.edit(r.asset_id, |tx, _| Ok(tx.recipes().delete(id)?))?;
        let path = r.rel_path.to_fs_path(&self.lib.recipes_dir());
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}
