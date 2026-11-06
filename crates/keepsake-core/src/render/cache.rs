//! The rendition cache: files named by source hash + recipe hash.
//!
//! `<root>/<kind>/<first two hex digits>/<source hash>-<recipe hash>.<ext>`.
//! The catalog's `renditions` table indexes them (size, last use). Files are
//! written to `<root>/.tmp` and renamed into place, so a rendition is either
//! complete or absent.
//!
//! Eviction removes previews and proxies first, then thumbnails (each least
//! recently used first) until the cache is under its limit, but never one
//! whose source is not on a mounted volume: it could not be made again, and
//! it is what lets offline media be browsed.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::recipe::Variant;
use super::RenderError;
use crate::catalog::{
    AssetId, Blake3Hash, Catalog, CatalogError, Rendition, RenditionId, RenditionKind, VolumeId,
};
use crate::library::Library;
use crate::relpath::RelPath;

/// Last-use times are written at most this often per rendition.
const TOUCH_EVERY_MS: i64 = 10 * 60 * 1000;

/// Cache figures.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CacheStats {
    pub root: PathBuf,
    pub files: u64,
    pub bytes: u64,
    pub max_bytes: u64,
    /// Bytes of renditions whose media is offline (never evicted).
    pub kept_offline_bytes: u64,
}

/// What an eviction pass did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct EvictReport {
    pub removed: u64,
    pub freed_bytes: u64,
    pub bytes_after: u64,
    /// Still over the limit because the rest belongs to offline media.
    pub kept_offline_bytes: u64,
}

/// What [`Cache::repair_names`] and [`Cache::move_source`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RepairReport {
    /// Renditions now at their content-addressed name.
    pub renamed: u64,
    /// Renditions dropped: their file was gone, or the same rendition
    /// already existed under the new key (made again when needed).
    pub dropped: u64,
}

/// What to do with a misnamed rendition's row, once its file is dealt with.
enum Fix {
    /// The file is now at `to`.
    Moved { to: RelPath, size: u64 },
    /// No file: remove the row.
    Drop,
    /// Leave it (another row owns the file, or the disk said no).
    Keep,
}

/// Renditions handled per write transaction by [`Cache::repair_names`].
const REPAIR_BATCH: u32 = 500;

/// The rendition cache folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    root: PathBuf,
}

fn now() -> i64 {
    crate::jobs::now_ms()
}

/// Bytes of the files under `p`.
fn dir_size(p: &Path) -> u64 {
    let mut n = 0;
    let mut stack = vec![p.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(e.path()),
                Ok(_) => n += e.metadata().map(|m| m.len()).unwrap_or(0),
                Err(_) => {}
            }
        }
    }
    n
}

/// The sub-folders Keepsake makes in a cache folder. A cache folder may be
/// shared with other files (a photo folder, say); only these are Keepsake's.
pub const CACHE_DIRS: [&str; 6] = ["thumbnail", "preview", "proxy", "export", "card", ".tmp"];

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a rendition of `source` lives, relative to the root.
    pub fn rel_path(source: &Blake3Hash, variant: Variant) -> Result<RelPath, RenderError> {
        name_for(
            source,
            &variant.recipe_hash(),
            variant.kind(),
            variant.ext(),
        )
    }

    /// Where a catalogued rendition should live: its content-addressed name
    /// (keeping its extension, so renditions of older recipes work too).
    pub fn expected_rel_path(r: &Rendition) -> Result<RelPath, RenderError> {
        let name = r.rel_path.file_name();
        let ext = name.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        name_for(&r.source_hash, &r.recipe_hash, r.kind, ext)
    }

    /// Move a rendition's file to its content-addressed name (its source
    /// was re-keyed: ADR-029, ADR-034). The file moves first and the row
    /// after, so a crash in between leaves a row that
    /// [`Self::repair_names`] still fixes:
    /// - the name is free: the file is renamed;
    /// - a file already has the name (the same content, made under the new
    ///   key): the old file is removed and the row points at that one;
    /// - neither file exists: the row is dropped;
    /// - the old name is another row's own file: that file is left alone.
    fn fix_file(&self, cat: &Catalog, r: &Rendition) -> Result<Fix, RenderError> {
        let want = Self::expected_rel_path(r)?;
        if r.rel_path == want {
            return Ok(Fix::Keep);
        }
        let from = r.rel_path.to_fs_path(&self.root);
        let to = want.to_fs_path(&self.root);
        let ours = !self.owned_by_other(cat, r)?;
        if ours && !to.is_file() && from.is_file() {
            if let Some(dir) = to.parent() {
                std::fs::create_dir_all(dir)?;
            }
            if let Err(e) = std::fs::rename(&from, &to) {
                // Another process may have moved it first.
                if !to.is_file() {
                    tracing::warn!("rename {} -> {}: {e}", from.display(), to.display());
                    return Ok(Fix::Keep);
                }
            }
        } else if ours && to.is_file() {
            match std::fs::remove_file(&from) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => tracing::warn!("remove {}: {e}", from.display()),
            }
        }
        match std::fs::metadata(&to) {
            Ok(m) if m.is_file() => Ok(Fix::Moved {
                to: want,
                size: m.len(),
            }),
            _ => Ok(Fix::Drop),
        }
    }

    /// Is `r`'s current file the correctly named file of another row?
    fn owned_by_other(&self, cat: &Catalog, r: &Rendition) -> Result<bool, RenderError> {
        let name = r.rel_path.file_name();
        let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
        let Some((src, recipe)) = stem.split_once('-') else {
            return Ok(false);
        };
        let (Some(src), Some(recipe)) = (Blake3Hash::from_hex(src), Blake3Hash::from_hex(recipe))
        else {
            return Ok(false);
        };
        let other = cat.read(|tx| tx.renditions().find(&src, &recipe, r.kind))?;
        Ok(other.is_some_and(|o| o.id != r.id && o.rel_path == r.rel_path))
    }

    /// Record what [`Self::fix_file`] did, in one transaction.
    fn apply_fixes(cat: &mut Catalog, fixes: &[(Rendition, Fix)]) -> Result<(), RenderError> {
        if fixes.iter().all(|(_, f)| matches!(f, Fix::Keep)) {
            return Ok(());
        }
        cat.write(|tx| -> Result<(), CatalogError> {
            for (r, fix) in fixes {
                match fix {
                    Fix::Moved { to, size } => {
                        tx.renditions().set_rel_path(r.id, &r.rel_path, to, *size)?;
                    }
                    Fix::Drop => tx.renditions().delete(r.id)?,
                    Fix::Keep => {}
                }
            }
            Ok(())
        })?;
        Ok(())
    }

    /// After a source was re-keyed from `old` to `new` (its fingerprint is
    /// now known): give its renditions their new names, and remove those
    /// left under `old` because the same rendition already existed under
    /// `new`.
    pub fn move_source(
        &self,
        cat: &mut Catalog,
        old: &Blake3Hash,
        new: &Blake3Hash,
    ) -> Result<RepairReport, RenderError> {
        let mut fixes = Vec::new();
        for r in cat.read(|tx| tx.renditions().for_source(new))? {
            let fix = self.fix_file(cat, &r)?;
            fixes.push((r, fix));
        }
        for r in cat.read(|tx| tx.renditions().for_source(old))? {
            let full = r.rel_path.to_fs_path(&self.root);
            match std::fs::remove_file(&full) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => {
                    tracing::warn!("remove {}: {e}", full.display());
                    continue;
                }
            }
            fixes.push((r, Fix::Drop));
        }
        Self::apply_fixes(cat, &fixes)?;
        Ok(report(&fixes))
    }

    /// Fix every rendition whose file is not at its content-addressed name
    /// (made under a stand-in key before its fingerprint was known, by a
    /// version without [`Self::move_source`], or left by a crash). Short
    /// transactions of [`REPAIR_BATCH`] rows, so it can run while the app
    /// works; one indexed scan when there is nothing to do.
    pub fn repair_names(&self, cat: &mut Catalog) -> Result<RepairReport, RenderError> {
        let mut total = RepairReport::default();
        let mut after = None;
        loop {
            let rows = cat.read(|tx| tx.renditions().misnamed(after, REPAIR_BATCH))?;
            let Some(last) = rows.last() else {
                break;
            };
            after = Some(last.id);
            let mut fixes = Vec::with_capacity(rows.len());
            for r in rows {
                let fix = self.fix_file(cat, &r)?;
                fixes.push((r, fix));
            }
            Self::apply_fixes(cat, &fixes)?;
            let rep = report(&fixes);
            total.renamed += rep.renamed;
            total.dropped += rep.dropped;
        }
        if total != RepairReport::default() {
            tracing::info!(
                renamed = total.renamed,
                dropped = total.dropped,
                "rendition names repaired"
            );
        }
        Ok(total)
    }

    /// A fresh temporary path inside the cache (same disk, so the final
    /// rename is atomic).
    pub fn temp_path(&self, ext: &str) -> Result<PathBuf, RenderError> {
        let dir = self.root.join(".tmp");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("{}.{ext}", uuid::Uuid::new_v4().simple())))
    }

    /// Delete every file Keepsake made in this cache folder (after moving
    /// the cache elsewhere). Only its own sub-folders are touched; anything
    /// else in the folder stays. Returns the bytes freed.
    pub fn remove_files(&self) -> Result<u64, RenderError> {
        let mut freed = 0;
        for sub in CACHE_DIRS {
            let d = self.root.join(sub);
            if d.is_dir() {
                freed += dir_size(&d);
                std::fs::remove_dir_all(&d)?;
            }
        }
        Ok(freed)
    }

    /// Remove temporaries left by a crash.
    pub fn clear_temp(&self) -> u64 {
        let mut n = 0;
        if let Ok(rd) = std::fs::read_dir(self.root.join(".tmp")) {
            for e in rd.flatten() {
                if std::fs::remove_file(e.path()).is_ok() {
                    n += 1;
                }
            }
        }
        n
    }

    /// Store `bytes` as a rendition.
    pub fn put_bytes(
        &self,
        cat: &mut Catalog,
        source: &Blake3Hash,
        variant: Variant,
        bytes: &[u8],
    ) -> Result<PathBuf, RenderError> {
        let tmp = self.temp_path(variant.ext())?;
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(bytes)?;
            f.sync_all()?;
        }
        self.put_file(cat, source, variant, &tmp)
    }

    /// Move a finished temporary file into place as a rendition.
    pub fn put_file(
        &self,
        cat: &mut Catalog,
        source: &Blake3Hash,
        variant: Variant,
        tmp: &Path,
    ) -> Result<PathBuf, RenderError> {
        let rel = Self::rel_path(source, variant)?;
        let full = rel.to_fs_path(&self.root);
        if let Some(dir) = full.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let size = std::fs::metadata(tmp)?.len();
        if let Err(e) = std::fs::rename(tmp, &full) {
            let _ = std::fs::remove_file(tmp);
            return Err(e.into());
        }
        let t = now();
        cat.write(|tx| {
            tx.renditions().upsert(&Rendition {
                id: RenditionId::new_random(),
                source_hash: *source,
                recipe_hash: variant.recipe_hash(),
                kind: variant.kind(),
                rel_path: rel.clone(),
                size_bytes: size,
                created_ms: t,
                last_used_ms: t,
            })
        })?;
        Ok(full)
    }

    /// The file of a rendition if it exists (a row whose file is gone is
    /// removed). Does not update its last use.
    pub fn find(
        &self,
        cat: &mut Catalog,
        source: &Blake3Hash,
        variant: Variant,
    ) -> Result<Option<(Rendition, PathBuf)>, RenderError> {
        let row = cat.read(|tx| {
            tx.renditions()
                .find(source, &variant.recipe_hash(), variant.kind())
        })?;
        let Some(mut r) = row else {
            return Ok(None);
        };
        // A row not at its content-addressed name (its source was re-keyed)
        // is moved there first; if that fails it counts as missing, so the
        // caller makes it again.
        let want = Self::rel_path(source, variant)?;
        if r.rel_path != want {
            let fix = self.fix_file(cat, &r)?;
            let moved = match &fix {
                Fix::Moved { to, size } if *to == want => Some((to.clone(), *size)),
                _ => None,
            };
            Self::apply_fixes(cat, &[(r.clone(), fix)])?;
            let Some((to, size)) = moved else {
                return Ok(None);
            };
            r.rel_path = to;
            r.size_bytes = size;
        }
        let full = r.rel_path.to_fs_path(&self.root);
        if full.is_file() {
            return Ok(Some((r, full)));
        }
        cat.write(|tx| tx.renditions().delete(r.id))?;
        Ok(None)
    }

    /// The rendition of an asset, for display: updates its last use (at
    /// most every few minutes). `None` if it is not made yet or was evicted;
    /// call [`super::set_visible`] to have it made.
    pub fn lookup(
        &self,
        cat: &mut Catalog,
        asset: AssetId,
        variant: Variant,
    ) -> Result<Option<PathBuf>, RenderError> {
        let Some(hash) = super::source::source_hash(cat, asset)? else {
            return Ok(None);
        };
        let Some((r, path)) = self.find(cat, &hash, variant)? else {
            return Ok(None);
        };
        let t = now();
        if t - r.last_used_ms > TOUCH_EVERY_MS {
            cat.write(|tx| tx.renditions().touch(r.id, t))?;
        }
        Ok(Some(path))
    }

    /// The best proxy of a video asset: the camera's, else the generated one.
    pub fn lookup_proxy(
        &self,
        cat: &mut Catalog,
        asset: AssetId,
    ) -> Result<Option<PathBuf>, RenderError> {
        match self.lookup(cat, asset, Variant::CameraProxy)? {
            Some(p) => Ok(Some(p)),
            None => self.lookup(cat, asset, Variant::Proxy720),
        }
    }

    pub fn stats(
        &self,
        cat: &Catalog,
        lib: &Library,
        max_bytes: u64,
    ) -> Result<CacheStats, RenderError> {
        // Two indexed queries: cheap enough to poll.
        let online = online_volumes(cat, lib)?;
        let (files, bytes) = cat.read(|tx| tx.renditions().totals())?;
        let kept_offline_bytes = cat.read(|tx| tx.renditions().bytes_not_on(&online))?;
        Ok(CacheStats {
            root: self.root.clone(),
            files,
            bytes,
            max_bytes,
            kept_offline_bytes,
        })
    }

    /// Remove renditions of online media until the cache holds at most
    /// `max_bytes`: previews and proxies first, then thumbnails, each least
    /// recently used first.
    pub fn evict(
        &self,
        cat: &mut Catalog,
        lib: &Library,
        max_bytes: u64,
    ) -> Result<EvictReport, RenderError> {
        self.evict_to(cat, lib, max_bytes, max_bytes)
    }

    /// Like [`Self::evict`], but once over `max_bytes` remove down to
    /// `target` (below it), so the next pass is not due after one more file.
    pub fn evict_to(
        &self,
        cat: &mut Catalog,
        lib: &Library,
        max_bytes: u64,
        target: u64,
    ) -> Result<EvictReport, RenderError> {
        let target = target.min(max_bytes);
        let (_, mut total) = cat.read(|tx| tx.renditions().totals())?;
        let mut rep = EvictReport::default();
        if total > max_bytes {
            let rows = cat.read(|tx| tx.renditions().list_lru())?;
            let online = online_volumes(cat, lib)?;
            let mut gone = Vec::new();
            for r in &rows {
                if total <= target {
                    break;
                }
                if !cat.read(|tx| tx.renditions().source_on(&r.source_hash, &online))? {
                    rep.kept_offline_bytes += r.size_bytes;
                    continue;
                }
                let full = r.rel_path.to_fs_path(&self.root);
                match std::fs::remove_file(&full) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        tracing::warn!("evict {}: {e}", full.display());
                        continue;
                    }
                }
                gone.push(r.id);
                total -= r.size_bytes;
                rep.removed += 1;
                rep.freed_bytes += r.size_bytes;
            }
            cat.write(|tx| -> Result<(), CatalogError> {
                for id in &gone {
                    tx.renditions().delete(*id)?;
                }
                Ok(())
            })?;
        }
        rep.bytes_after = total;
        Ok(rep)
    }
}

/// `<kind>/<aa>/<source>-<recipe>.<ext>`.
fn name_for(
    source: &Blake3Hash,
    recipe: &Blake3Hash,
    kind: RenditionKind,
    ext: &str,
) -> Result<RelPath, RenderError> {
    let hex = source.to_hex();
    let p = format!(
        "{}/{}/{}-{}.{}",
        kind.as_str(),
        &hex[..2],
        hex,
        recipe.to_hex(),
        ext
    );
    RelPath::parse(&p).map_err(|e| RenderError::Failed(e.to_string()))
}

fn report(fixes: &[(Rendition, Fix)]) -> RepairReport {
    let mut rep = RepairReport::default();
    for (_, f) in fixes {
        match f {
            Fix::Moved { .. } => rep.renamed += 1,
            Fix::Drop => rep.dropped += 1,
            Fix::Keep => {}
        }
    }
    rep
}

/// Volumes mounted right now.
fn online_volumes(cat: &Catalog, lib: &Library) -> Result<Vec<VolumeId>, RenderError> {
    Ok(cat
        .read(|tx| tx.volumes().list())?
        .into_iter()
        .filter(|v| crate::verify::mount_of(cat, lib, v.id).is_some())
        .map(|v| v.id)
        .collect())
}
