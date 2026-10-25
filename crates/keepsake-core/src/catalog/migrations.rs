//! Versioned schema migrations tracked with `PRAGMA user_version`.

use rusqlite::Connection;

use super::error::CatalogError;

/// One schema step. Never edit a released migration; add a new one.
struct Migration {
    version: i64,
    sql: &'static str,
    /// Rebuilds a table: run with foreign keys off (SQLite's documented
    /// procedure for altering constraints), then verify them.
    rebuilds_tables: bool,
}

const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        sql: include_str!("migrations/0001_init.sql"),
        rebuilds_tables: false,
    },
    Migration {
        version: 2,
        sql: include_str!("migrations/0002_probe.sql"),
        rebuilds_tables: true,
    },
    Migration {
        version: 3,
        sql: include_str!("migrations/0003_ingest.sql"),
        rebuilds_tables: true,
    },
    Migration {
        version: 4,
        sql: include_str!("migrations/0004_jobs.sql"),
        rebuilds_tables: false,
    },
    Migration {
        version: 5,
        sql: include_str!("migrations/0005_browse.sql"),
        rebuilds_tables: false,
    },
];

/// The schema version this build creates and understands.
pub const LATEST_VERSION: i64 = 5;

pub(crate) fn apply(conn: &mut Connection) -> Result<(), CatalogError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > LATEST_VERSION {
        return Err(CatalogError::SchemaTooNew {
            found: current,
            supported: LATEST_VERSION,
        });
    }
    for m in MIGRATIONS {
        if m.version <= current {
            continue;
        }
        if m.rebuilds_tables {
            // Must be set outside a transaction.
            conn.pragma_update(None, "foreign_keys", false)?;
        }
        let result = (|| -> Result<(), CatalogError> {
            let tx = conn.transaction()?;
            tx.execute_batch(m.sql)?;
            if m.rebuilds_tables {
                let broken: i64 =
                    tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                        r.get(0)
                    })?;
                if broken > 0 {
                    return Err(CatalogError::Invalid(format!(
                        "migration {} left {broken} broken foreign keys",
                        m.version
                    )));
                }
            }
            // PRAGMA does not accept bound parameters; the version is a constant.
            tx.execute_batch(&format!("PRAGMA user_version = {}", m.version))?;
            tx.commit()?;
            Ok(())
        })();
        if m.rebuilds_tables {
            conn.pragma_update(None, "foreign_keys", true)?;
        }
        result?;
        tracing::info!(version = m.version, "applied catalog migration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_matches_list() {
        assert_eq!(MIGRATIONS.last().map(|m| m.version), Some(LATEST_VERSION));
        for (i, m) in MIGRATIONS.iter().enumerate() {
            assert_eq!(m.version, i as i64 + 1, "migrations must be numbered 1..N");
        }
    }

    #[test]
    fn v1_catalog_upgrades_and_keeps_rows() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.pragma_update(None, "foreign_keys", true).unwrap();
        let tx = conn.transaction().unwrap();
        tx.execute_batch(MIGRATIONS[0].sql).unwrap();
        tx.execute_batch(
            "PRAGMA user_version = 1;
             INSERT INTO volumes (id, label, fs_id, kind, case_sensitive, read_only)
               VALUES (x'00000000000000000000000000000001', 'v', 'fs', 'card', 0, 0);
             INSERT INTO assets (id, media_kind, created_ms, metadata_ms)
               VALUES (x'00000000000000000000000000000002', 'photo', 0, 0);
             INSERT INTO files (id, asset_id, volume_id, rel_path, rel_path_key, file_name, role, size_bytes, blake3)
               VALUES (x'00000000000000000000000000000003', x'00000000000000000000000000000002',
                       x'00000000000000000000000000000001', 'A.JPG', 'a.jpg', 'A.JPG', 'original', 1,
                       zeroblob(32));
             INSERT INTO import_sessions (id, source_volume_id, started_ms, status)
               VALUES (x'00000000000000000000000000000004', x'00000000000000000000000000000001', 0, 'completed');
             INSERT INTO import_items (session_id, source_rel_path, dest_volume_id, file_id, status)
               VALUES (x'00000000000000000000000000000004', 'A.JPG', x'00000000000000000000000000000001',
                       x'00000000000000000000000000000003', 'verified');",
        )
        .unwrap();
        tx.commit().unwrap();

        apply(&mut conn).unwrap();
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, LATEST_VERSION);
        let n: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let fk: bool = conn
            .query_row("PRAGMA foreign_keys", [], |r| r.get(0))
            .unwrap();
        assert!(fk, "foreign keys must be back on");
        // The import item survived both table rebuilds and still points at its file.
        let ok: i64 = conn
            .query_row(
                "SELECT count(*) FROM import_items i JOIN files f ON f.id = i.file_id",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ok, 1);
        // Deleting the asset still cascades to its files.
        conn.execute("DELETE FROM import_items", []).unwrap();
        conn.execute("DELETE FROM assets", []).unwrap();
        let n: i64 = conn
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }
}
