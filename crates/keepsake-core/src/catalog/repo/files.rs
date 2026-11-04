use rusqlite::{params, OptionalExtension, Row};

use super::{define_repo, rel_path_col, to_i64, u64_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{
    fts, AssetId, Blake3Hash, File, FileHash, FileId, FileRole, NewFile, Rw, VolumeId,
};
use crate::relpath::{CaseSensitivity, RelPath};

define_repo!(
    /// Physical files.
    Files
);

const COLS: &str = "id, asset_id, volume_id, rel_path, role, size_bytes, blake3, xxh3, mtime_ms, \
     segment_index, fingerprinted";

fn map(r: &Row<'_>) -> rusqlite::Result<File> {
    let xxh3: Option<i64> = r.get(7)?;
    let key: Blake3Hash = r.get(6)?;
    let full: bool = r.get(10)?;
    Ok(File {
        id: r.get(0)?,
        asset_id: r.get(1)?,
        volume_id: r.get(2)?,
        rel_path: rel_path_col(r, 3)?,
        role: r.get(4)?,
        size_bytes: u64_col(r, 5)?,
        hash: if full {
            FileHash::Full(key)
        } else {
            FileHash::Pending(key)
        },
        // xxh3 is a 64-bit hash stored bit-for-bit in an INTEGER column.
        xxh3: xxh3.map(|v| v as u64),
        mtime_ms: r.get(8)?,
        segment_index: r.get(9)?,
    })
}

impl<M> Files<'_, M> {
    pub fn get(&self, id: FileId) -> Result<Option<File>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM files WHERE id = ?1"),
                [id],
                map,
            )
            .optional()?)
    }

    pub fn count(&self) -> Result<u64, CatalogError> {
        let n: i64 = self
            .conn
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))?;
        super::to_u64(n, "count")
    }

    pub fn for_asset(&self, asset: AssetId) -> Result<Vec<File>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM files WHERE asset_id = ?1 ORDER BY volume_id, rel_path_key"
        ))?;
        let rows = st.query_map([asset], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    pub fn for_volume(&self, volume: VolumeId) -> Result<Vec<File>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM files WHERE volume_id = ?1 ORDER BY rel_path_key"
        ))?;
        let rows = st.query_map([volume], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// The asset's original files, one per volume holding a copy.
    pub fn originals(&self, asset: AssetId) -> Result<Vec<File>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM files WHERE asset_id = ?1 AND role = 'original' ORDER BY volume_id"
        ))?;
        let rows = st.query_map([asset], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Look a file up by location, honouring the volume's case and Unicode rules.
    pub fn by_location(
        &self,
        volume: VolumeId,
        rel_path: &RelPath,
    ) -> Result<Option<File>, CatalogError> {
        let case = volume_case(self.conn, volume)?;
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM files WHERE volume_id = ?1 AND rel_path_key = ?2"),
                params![volume, rel_path.key(case)],
                map,
            )
            .optional()?)
    }

    /// Files whose size and quick hash (`xxh3`) match.
    pub fn by_quick(&self, size: u64, quick: u64) -> Result<Vec<File>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM files WHERE size_bytes = ?1 AND xxh3 = ?2 ORDER BY volume_id, rel_path_key"
        ))?;
        let rows = st
            .query_map(params![to_i64(size, "size")?, quick as i64], map)?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Files whose full fingerprint is `hash` (stand-in keys never match).
    pub fn by_hash(&self, hash: &Blake3Hash) -> Result<Vec<File>, CatalogError> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {COLS} FROM files WHERE blake3 = ?1 AND fingerprinted = 1
             ORDER BY volume_id, rel_path_key"
        ))?;
        let rows = st.query_map([hash], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// Assets with a file not yet read in full, newest capture first.
    pub fn assets_unfingerprinted(&self) -> Result<Vec<AssetId>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT a.id FROM assets a
             WHERE EXISTS (SELECT 1 FROM files f WHERE f.asset_id = a.id AND f.fingerprinted = 0)
             ORDER BY a.capture_ts_ms IS NULL, a.capture_ts_ms DESC, a.id",
        )?;
        let rows = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        Ok(rows)
    }

    /// How many files on each volume are not yet read in full.
    pub fn unfingerprinted_by_volume(&self) -> Result<Vec<(VolumeId, u64)>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT volume_id, count(*) FROM files WHERE fingerprinted = 0 GROUP BY volume_id",
        )?;
        let rows = st
            .query_map([], |r| Ok((r.get::<_, VolumeId>(0)?, r.get::<_, i64>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(v, n)| Ok((v, super::to_u64(n, "count")?)))
            .collect()
    }
}

pub(crate) fn volume_case(
    conn: &rusqlite::Connection,
    volume: VolumeId,
) -> Result<CaseSensitivity, CatalogError> {
    let cs: Option<bool> = conn
        .query_row(
            "SELECT case_sensitive FROM volumes WHERE id = ?1",
            [volume],
            |r| r.get(0),
        )
        .optional()?;
    cs.map(CaseSensitivity::from_bool)
        .ok_or_else(|| CatalogError::not_found("volume", volume))
}

impl Files<'_, Rw> {
    /// Add a file to an asset. The file id is derived from its location, and
    /// the volume's case/Unicode rules decide whether it duplicates another.
    pub fn insert(&self, asset: AssetId, f: &NewFile) -> Result<File, CatalogError> {
        let case = volume_case(self.conn, f.volume_id)?;
        let key = f.rel_path.key(case);
        let id = FileId::for_location(f.volume_id, &key);
        self.conn.prepare_cached(
            "INSERT INTO files (id, asset_id, volume_id, rel_path, rel_path_key, file_name, role,
                                size_bytes, blake3, xxh3, mtime_ms, segment_index,
                                fingerprinted)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
        )?
        .execute(params![
                id,
                asset,
                f.volume_id,
                f.rel_path.as_str(),
                key,
                f.rel_path.file_name_nfc(),
                f.role,
                to_i64(f.size_bytes, "size")?,
                f.hash.key(),
                f.xxh3.map(|v| v as i64),
                f.mtime_ms,
                // Segment positions are only stored for segments (> 0).
                f.segment_index.filter(|i| *i > 0),
                f.hash.is_full()
            ])
        .map_err(|e| match CatalogError::from(e) {
            CatalogError::Duplicate(_) => {
                CatalogError::Duplicate(format!("file {} on volume {}", f.rel_path, f.volume_id))
            }
            other => other,
        })?;
        fts::reindex(self.conn, asset)?;
        Ok(File {
            id,
            asset_id: asset,
            volume_id: f.volume_id,
            rel_path: f.rel_path.clone(),
            role: f.role,
            size_bytes: f.size_bytes,
            hash: f.hash,
            xxh3: f.xxh3,
            mtime_ms: f.mtime_ms,
            segment_index: f.segment_index.filter(|i| *i > 0),
        })
    }

    /// Store the full fingerprint of a file that had a stand-in key; its
    /// renditions move to the real key. Returns `false` if the file is gone
    /// or already fingerprinted.
    pub fn set_fingerprint(&self, id: FileId, hash: &Blake3Hash) -> Result<bool, CatalogError> {
        let Some(old) = self.get(id)? else {
            return Ok(false);
        };
        let FileHash::Pending(key) = old.hash else {
            return Ok(false);
        };
        self.conn.execute(
            "UPDATE files SET blake3 = ?2, fingerprinted = 1 WHERE id = ?1",
            params![id, hash],
        )?;
        // A rendition already made from the same content under the real key
        // wins; the stand-in's is left to the cache's eviction.
        self.conn.execute(
            "UPDATE OR IGNORE renditions SET source_hash = ?2 WHERE source_hash = ?1",
            params![key, hash],
        )?;
        Ok(true)
    }

    pub fn set_role(&self, id: FileId, role: FileRole) -> Result<(), CatalogError> {
        self.conn.execute(
            "UPDATE files SET role = ?2 WHERE id = ?1",
            params![id, role],
        )?;
        Ok(())
    }

    pub fn delete(&self, id: FileId) -> Result<(), CatalogError> {
        let asset: Option<AssetId> = self
            .conn
            .query_row("SELECT asset_id FROM files WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        self.conn.execute("DELETE FROM files WHERE id = ?1", [id])?;
        if let Some(a) = asset {
            fts::reindex(self.conn, a)?;
        }
        Ok(())
    }

    /// Remove every file row on a volume (used by rebuild). Assets left with no
    /// files are not removed here; see [`super::Assets::delete_orphans`].
    pub fn delete_for_volume(&self, volume: VolumeId) -> Result<u64, CatalogError> {
        let mut st = self
            .conn
            .prepare("SELECT DISTINCT asset_id FROM files WHERE volume_id = ?1")?;
        let assets: Vec<AssetId> = st
            .query_map([volume], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        let n = self
            .conn
            .execute("DELETE FROM files WHERE volume_id = ?1", [volume])?;
        for a in assets {
            fts::reindex_if_exists(self.conn, a)?;
        }
        Ok(n as u64)
    }
}
