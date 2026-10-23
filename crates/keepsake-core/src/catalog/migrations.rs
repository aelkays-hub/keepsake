//! Versioned schema migrations tracked with `PRAGMA user_version`.

use rusqlite::Connection;

use super::error::CatalogError;

/// `(version, sql)` pairs, applied in order. Never edit a released migration;
/// add a new one.
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("migrations/0001_init.sql"))];

/// The schema version this build creates and understands.
pub const LATEST_VERSION: i64 = 1;

pub(crate) fn apply(conn: &mut Connection) -> Result<(), CatalogError> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current > LATEST_VERSION {
        return Err(CatalogError::SchemaTooNew {
            found: current,
            supported: LATEST_VERSION,
        });
    }
    for &(version, sql) in MIGRATIONS {
        if version <= current {
            continue;
        }
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        // PRAGMA does not accept bound parameters; `version` is a compile-time constant.
        tx.execute_batch(&format!("PRAGMA user_version = {version}"))?;
        tx.commit()?;
        tracing::info!(version, "applied catalog migration");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latest_matches_list() {
        assert_eq!(MIGRATIONS.last().map(|m| m.0), Some(LATEST_VERSION));
        for (i, (v, _)) in MIGRATIONS.iter().enumerate() {
            assert_eq!(*v, i as i64 + 1, "migrations must be numbered 1..N");
        }
    }
}
