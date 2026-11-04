use std::path::{Path, PathBuf};

use rusqlite::{params, OptionalExtension, Row};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{Rw, Volume, VolumeId};
use crate::relpath::CaseSensitivity;

define_repo!(
    /// Storage volumes.
    Volumes
);

const COLS: &str =
    "id, label, fs_id, kind, case_sensitive, read_only, online, mount_path, last_seen_ms";

fn map(r: &Row<'_>) -> rusqlite::Result<Volume> {
    let mount: Option<String> = r.get(7)?;
    Ok(Volume {
        id: r.get(0)?,
        label: r.get(1)?,
        fs_id: r.get(2)?,
        kind: r.get(3)?,
        case: CaseSensitivity::from_bool(r.get(4)?),
        read_only: r.get(5)?,
        online: r.get(6)?,
        mount_path: mount.map(PathBuf::from),
        last_seen_ms: r.get(8)?,
    })
}

fn path_text(p: &Path) -> Result<&str, CatalogError> {
    p.to_str()
        .ok_or_else(|| CatalogError::Invalid(format!("mount path is not UTF-8: {}", p.display())))
}

impl<M> Volumes<'_, M> {
    pub fn get(&self, id: VolumeId) -> Result<Option<Volume>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM volumes WHERE id = ?1"),
                [id],
                map,
            )
            .optional()?)
    }

    /// Like [`Self::get`] but errors when missing.
    pub fn require(&self, id: VolumeId) -> Result<Volume, CatalogError> {
        self.get(id)?
            .ok_or_else(|| CatalogError::not_found("volume", id))
    }

    pub fn by_fs_id(&self, fs_id: &str) -> Result<Option<Volume>, CatalogError> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {COLS} FROM volumes WHERE fs_id = ?1"),
                [fs_id],
                map,
            )
            .optional()?)
    }

    pub fn list(&self) -> Result<Vec<Volume>, CatalogError> {
        let mut st = self
            .conn
            .prepare(&format!("SELECT {COLS} FROM volumes ORDER BY label, id"))?;
        let rows = st.query_map([], map)?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }
}

impl Volumes<'_, Rw> {
    /// Insert or fully replace a volume row.
    pub fn upsert(&self, v: &Volume) -> Result<(), CatalogError> {
        let mount = v.mount_path.as_deref().map(path_text).transpose()?;
        self.conn.execute(
            "INSERT INTO volumes (id, label, fs_id, kind, case_sensitive, read_only, online, mount_path, last_seen_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               label = excluded.label, fs_id = excluded.fs_id, kind = excluded.kind,
               case_sensitive = excluded.case_sensitive, read_only = excluded.read_only,
               online = excluded.online, mount_path = excluded.mount_path,
               last_seen_ms = excluded.last_seen_ms",
            params![
                v.id,
                v.label,
                v.fs_id,
                v.kind,
                v.case.is_sensitive(),
                v.read_only,
                v.online,
                mount,
                v.last_seen_ms
            ],
        )?;
        Ok(())
    }

    /// Mark a volume online at `mount` (or offline when `None`).
    pub fn set_mount(
        &self,
        id: VolumeId,
        mount: Option<&Path>,
        now_ms: i64,
    ) -> Result<(), CatalogError> {
        let mount = mount.map(path_text).transpose()?;
        let n = self.conn.execute(
            "UPDATE volumes SET online = ?2, mount_path = ?3,
               last_seen_ms = CASE WHEN ?2 THEN ?4 ELSE last_seen_ms END
             WHERE id = ?1",
            params![id, mount.is_some(), mount, now_ms],
        )?;
        if n == 0 {
            return Err(CatalogError::not_found("volume", id));
        }
        Ok(())
    }

    /// Remove a volume. Fails while files still reference it.
    pub fn delete(&self, id: VolumeId) -> Result<(), CatalogError> {
        self.conn
            .execute("DELETE FROM volumes WHERE id = ?1", [id])?;
        Ok(())
    }
}
