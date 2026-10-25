//! The SQLite catalog: a rebuildable query cache over files and XMP sidecars.
//!
//! All SQL in Keepsake lives in this module. Callers use [`Catalog::read`] and
//! [`Catalog::write`] to get a transaction, then the typed repositories on it
//! (`tx.assets()`, `tx.files()`, …). Write methods only exist on repositories
//! obtained from a [`WriteTx`].

mod error;
mod fts;
mod ids;
mod migrations;
mod model;
mod repo;
mod snapshot;

use std::marker::PhantomData;
use std::path::Path;
use std::time::Duration;

use rusqlite::{Connection, TransactionBehavior};

pub use error::CatalogError;
pub use ids::{
    AssetId, Blake3Hash, EventId, FileId, ImportSessionId, RecipeId, RenditionId, SequenceId,
    VolumeId,
};
pub use migrations::LATEST_VERSION;
pub use model::*;
pub use repo::{
    Assets, Browse, BrowseRow, CalendarBucket, CalendarLevel, DayCount, EventSpan, Events, Facet,
    FacetColumn, Files, Imports, Job, JobCounts, JobState, Jobs, Labels, Meta, NewJob, Priority,
    Recipes, Renditions, Search, SearchQuery, Sequences, Sidecars, Sort, SortField, Tags, Volumes,
};
pub use snapshot::{AssetSnapshot, CatalogSnapshot, FtsRow};

/// Marker: read-only transaction.
#[derive(Debug)]
pub struct Ro;
/// Marker: read-write transaction.
#[derive(Debug)]
pub struct Rw;

/// A catalog transaction. `M` is [`Ro`] or [`Rw`].
pub struct Tx<'c, M> {
    tx: rusqlite::Transaction<'c>,
    _mode: PhantomData<M>,
}

pub type ReadTx<'c> = Tx<'c, Ro>;
pub type WriteTx<'c> = Tx<'c, Rw>;

macro_rules! repo_accessor {
    ($fn:ident, $ty:ident) => {
        pub fn $fn(&self) -> $ty<'_, M> {
            $ty::new(&self.tx)
        }
    };
}

impl<M> Tx<'_, M> {
    repo_accessor!(meta, Meta);
    repo_accessor!(volumes, Volumes);
    repo_accessor!(assets, Assets);
    repo_accessor!(files, Files);
    repo_accessor!(labels, Labels);
    repo_accessor!(tags, Tags);
    repo_accessor!(events, Events);
    repo_accessor!(recipes, Recipes);
    repo_accessor!(renditions, Renditions);
    repo_accessor!(imports, Imports);
    repo_accessor!(jobs, Jobs);
    repo_accessor!(sidecars, Sidecars);
    repo_accessor!(search, Search);
    repo_accessor!(browse, Browse);
    repo_accessor!(sequences, Sequences);

    /// The raw connection, for bulk paths inside the crate (seeding).
    pub(crate) fn conn(&self) -> &rusqlite::Connection {
        &self.tx
    }

    /// Canonical dump of everything a rebuild must reproduce.
    pub fn snapshot(&self) -> Result<CatalogSnapshot, CatalogError> {
        snapshot::take(&self.tx)
    }

    /// Check that the full-text index matches the tables. Returns the ids of
    /// assets whose index row is missing or stale.
    pub fn fts_mismatches(&self) -> Result<Vec<AssetId>, CatalogError> {
        fts::mismatches(&self.tx)
    }
}

/// An open catalog database.
pub struct Catalog {
    conn: Connection,
}

impl Catalog {
    /// Open (creating if needed) the catalog at `path` and apply pending migrations.
    pub fn open(path: &Path) -> Result<Self, CatalogError> {
        let conn = Connection::open(path)?;
        Self::init(conn, true)
    }

    /// An in-memory catalog (tests and scratch use).
    pub fn open_in_memory() -> Result<Self, CatalogError> {
        let conn = Connection::open_in_memory()?;
        Self::init(conn, false)
    }

    fn init(mut conn: Connection, wal: bool) -> Result<Self, CatalogError> {
        conn.busy_timeout(Duration::from_secs(5))?;
        conn.pragma_update(None, "foreign_keys", true)?;
        if wal {
            let mode: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
            if !mode.eq_ignore_ascii_case("wal") {
                return Err(CatalogError::Invalid(format!(
                    "could not enable WAL journal mode (got {mode})"
                )));
            }
            conn.pragma_update(None, "synchronous", "NORMAL")?;
        }
        migrations::apply(&mut conn)?;
        Ok(Self { conn })
    }

    /// Current schema version (`PRAGMA user_version`).
    pub fn schema_version(&self) -> Result<i64, CatalogError> {
        Ok(self
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))?)
    }

    /// The SQLite journal mode in use (e.g. `"wal"`).
    pub fn journal_mode(&self) -> Result<String, CatalogError> {
        Ok(self
            .conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))?)
    }

    /// Run `f` in a read transaction (consistent snapshot of the database).
    pub fn read<T, E>(&self, f: impl FnOnce(&ReadTx<'_>) -> Result<T, E>) -> Result<T, E>
    where
        E: From<CatalogError>,
    {
        let tx = self
            .conn
            .unchecked_transaction()
            .map_err(CatalogError::from)?;
        let tx = Tx {
            tx,
            _mode: PhantomData,
        };
        let out = f(&tx)?;
        tx.tx.finish().map_err(CatalogError::from)?;
        Ok(out)
    }

    /// Run `f` in a write transaction (`BEGIN IMMEDIATE`). Commits when `f`
    /// returns `Ok`, rolls back when it returns `Err`.
    pub fn write<T, E>(&mut self, f: impl FnOnce(&WriteTx<'_>) -> Result<T, E>) -> Result<T, E>
    where
        E: From<CatalogError>,
    {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(CatalogError::from)?;
        let tx = Tx {
            tx,
            _mode: PhantomData,
        };
        let out = f(&tx)?;
        tx.tx.commit().map_err(CatalogError::from)?;
        Ok(out)
    }

    /// Write a consistent, compacted copy of the whole catalog to `path`
    /// (`VACUUM INTO`). `path` must not exist.
    pub fn snapshot_to(&self, path: &Path) -> Result<(), CatalogError> {
        let p = path.to_str().ok_or_else(|| {
            CatalogError::Invalid(format!("path is not UTF-8: {}", path.display()))
        })?;
        self.conn.execute("VACUUM INTO ?1", [p])?;
        Ok(())
    }

    /// Delete a catalog database and its WAL/SHM companions. The catalog must
    /// not be open.
    pub fn delete_files(path: &Path) -> std::io::Result<()> {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = path.as_os_str().to_owned();
            p.push(suffix);
            match std::fs::remove_file(&p) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}

/// Current wall-clock time in Unix milliseconds.
pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
