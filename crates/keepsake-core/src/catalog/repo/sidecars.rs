use rusqlite::{params, Row};

use super::{define_repo, rel_path_col};
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Rw, SidecarRecord, VolumeId};

define_repo!(
    /// Where each asset's XMP sidecars were last read or written.
    Sidecars
);

fn map(r: &Row<'_>) -> rusqlite::Result<SidecarRecord> {
    Ok(SidecarRecord {
        asset_id: r.get(0)?,
        volume_id: r.get(1)?,
        location: r.get(2)?,
        rel_path: rel_path_col(r, 3)?,
        blake3: r.get(4)?,
        mtime_ms: r.get(5)?,
    })
}

impl<M> Sidecars<'_, M> {
    pub fn for_asset(&self, asset: AssetId) -> Result<Vec<SidecarRecord>, CatalogError> {
        let mut st = self.conn.prepare(
            "SELECT asset_id, volume_id, location, rel_path, blake3, mtime_ms
               FROM xmp_sidecars WHERE asset_id = ?1 ORDER BY volume_id",
        )?;
        let rows = st.query_map([asset], map)?.collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl Sidecars<'_, Rw> {
    pub fn upsert(&self, s: &SidecarRecord) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO xmp_sidecars (asset_id, volume_id, location, rel_path, blake3, mtime_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(asset_id, volume_id) DO UPDATE SET location = excluded.location,
               rel_path = excluded.rel_path, blake3 = excluded.blake3, mtime_ms = excluded.mtime_ms",
            params![
                s.asset_id,
                s.volume_id,
                s.location,
                s.rel_path.as_str(),
                s.blake3,
                s.mtime_ms
            ],
        )?;
        Ok(())
    }

    pub fn delete_for_volume(&self, volume: VolumeId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM xmp_sidecars WHERE volume_id = ?1", [volume])?;
        Ok(())
    }
}
