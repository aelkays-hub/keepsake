//! The rendition cache: files named by source hash + recipe hash.
//!
//! `<root>/<kind>/<first two hex digits>/<source hash>-<recipe hash>.<ext>`.
//! The catalog's `renditions` table indexes them (size, last use). Files are
//! written to `<root>/.tmp` and renamed into place, so a rendition is either
//! complete or absent.
//!
//! Eviction removes least-recently-used renditions until the cache is under
//! its limit, but never one whose source is not on a mounted volume: it
//! could not be made again, and it is what lets offline media be browsed.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::recipe::Variant;
use super::RenderError;
use crate::catalog::{AssetId, Blake3Hash, Catalog, CatalogError, Rendition, RenditionId};
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

/// The rendition cache folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cache {
    root: PathBuf,
}

fn now() -> i64 {
    crate::jobs::now_ms()
}

impl Cache {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where a rendition of `source` lives, relative to the root.
    pub fn rel_path(source: &Blake3Hash, variant: Variant) -> Result<RelPath, RenderError> {
        let hex = source.to_hex();
        let p = format!(
            "{}/{}/{}-{}.{}",
            variant.kind().as_str(),
            &hex[..2],
            hex,
            variant.recipe_hash().to_hex(),
            variant.ext()
        );
        RelPath::parse(&p).map_err(|e| RenderError::Failed(e.to_string()))
    }

    /// A fresh temporary path inside the cache (same disk, so the final
    /// rename is atomic).
    pub fn temp_path(&self, ext: &str) -> Result<PathBuf, RenderError> {
        let dir = self.root.join(".tmp");
        std::fs::create_dir_all(&dir)?;
        Ok(dir.join(format!("{}.{ext}", uuid::Uuid::new_v4().simple())))
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
        let Some(r) = row else {
            return Ok(None);
        };
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
        let rows = cat.read(|tx| tx.renditions().list_lru())?;
        let online = online_sources(cat, lib)?;
        let mut s = CacheStats {
            root: self.root.clone(),
            max_bytes,
            ..Default::default()
        };
        for r in &rows {
            s.files += 1;
            s.bytes += r.size_bytes;
            if !online.contains(&r.source_hash) {
                s.kept_offline_bytes += r.size_bytes;
            }
        }
        Ok(s)
    }

    /// Remove least-recently-used renditions of online media until the
    /// cache holds at most `max_bytes`.
    pub fn evict(
        &self,
        cat: &mut Catalog,
        lib: &Library,
        max_bytes: u64,
    ) -> Result<EvictReport, RenderError> {
        let rows = cat.read(|tx| tx.renditions().list_lru())?;
        let mut total: u64 = rows.iter().map(|r| r.size_bytes).sum();
        let mut rep = EvictReport::default();
        if total > max_bytes {
            let online = online_sources(cat, lib)?;
            let mut gone = Vec::new();
            for r in &rows {
                if total <= max_bytes {
                    break;
                }
                if !online.contains(&r.source_hash) {
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

/// BLAKE3 of every original on a mounted volume.
fn online_sources(cat: &Catalog, lib: &Library) -> Result<HashSet<Blake3Hash>, RenderError> {
    let vols = cat.read(|tx| tx.volumes().list())?;
    let mut out = HashSet::new();
    for v in vols {
        if crate::verify::mount_of(cat, lib, v.id).is_none() {
            continue;
        }
        for f in cat.read(|tx| tx.files().for_volume(v.id))? {
            if f.role == crate::catalog::FileRole::Original {
                out.insert(f.blake3);
            }
        }
    }
    Ok(out)
}
