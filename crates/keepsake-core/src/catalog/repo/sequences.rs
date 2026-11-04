use rusqlite::{params, OptionalExtension};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, Rw, SequenceId, SequenceKind, StillSequence, VolumeId};

define_repo!(
    /// Burst, bracket and interval runs of stills.
    Sequences
);

impl<M> Sequences<'_, M> {
    pub fn get(&self, id: SequenceId) -> Result<Option<StillSequence>, CatalogError> {
        let head: Option<(VolumeId, SequenceKind)> = self
            .conn
            .query_row(
                "SELECT volume_id, kind FROM sequences WHERE id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((volume_id, kind)) = head else {
            return Ok(None);
        };
        let mut st = self.conn.prepare(
            "SELECT asset_id FROM asset_sequences WHERE sequence_id = ?1 ORDER BY position",
        )?;
        let members = st
            .query_map([id], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(Some(StillSequence {
            id,
            volume_id,
            kind,
            members,
        }))
    }

    /// The sequence an asset belongs to, with its position.
    pub fn for_asset(&self, asset: AssetId) -> Result<Option<(StillSequence, u32)>, CatalogError> {
        let row: Option<(SequenceId, u32)> = self
            .conn
            .query_row(
                "SELECT sequence_id, position FROM asset_sequences WHERE asset_id = ?1",
                [asset],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match row {
            Some((id, pos)) => Ok(self.get(id)?.map(|s| (s, pos))),
            None => Ok(None),
        }
    }

    pub fn list(&self) -> Result<Vec<StillSequence>, CatalogError> {
        let mut st = self.conn.prepare("SELECT id FROM sequences ORDER BY id")?;
        let ids: Vec<SequenceId> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(s) = self.get(id)? {
                out.push(s);
            }
        }
        Ok(out)
    }
}

impl Sequences<'_, Rw> {
    /// Insert or replace a sequence and its membership. Members leave any
    /// other sequence they were in.
    pub fn upsert(&self, s: &StillSequence) -> Result<(), CatalogError> {
        self.conn.execute(
            "INSERT INTO sequences (id, volume_id, kind) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET volume_id = excluded.volume_id, kind = excluded.kind",
            params![s.id, s.volume_id, s.kind],
        )?;
        self.conn
            .execute("DELETE FROM asset_sequences WHERE sequence_id = ?1", [s.id])?;
        for (pos, a) in s.members.iter().enumerate() {
            self.conn
                .execute("DELETE FROM asset_sequences WHERE asset_id = ?1", [a])?;
            self.conn.execute(
                "INSERT INTO asset_sequences (asset_id, sequence_id, position) VALUES (?1, ?2, ?3)",
                params![a, s.id, pos as i64],
            )?;
        }
        Ok(())
    }

    /// Remove sequences left with fewer than two members.
    pub fn delete_small(&self) -> Result<u64, CatalogError> {
        let n = self.conn.execute(
            "DELETE FROM sequences WHERE (SELECT count(*) FROM asset_sequences m WHERE m.sequence_id = sequences.id) < 2",
            [],
        )?;
        Ok(n as u64)
    }

    pub fn delete_for_volume(&self, volume: VolumeId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM sequences WHERE volume_id = ?1", [volume])?;
        Ok(())
    }
}
