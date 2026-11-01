use thiserror::Error;

use crate::relpath::PathError;

/// Errors raised by the catalog.
#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("SQLite error: {0}")]
    Sqlite(rusqlite::Error),

    #[error("catalog schema version {found} is newer than this build supports ({supported})")]
    SchemaTooNew { found: i64, supported: i64 },

    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    #[error("duplicate {0}")]
    Duplicate(String),

    #[error("invalid data: {0}")]
    Invalid(String),

    #[error(transparent)]
    Path(#[from] PathError),
}

impl CatalogError {
    /// Another connection held the catalog for longer than the busy timeout
    /// ("database is locked"). Trying again later can succeed.
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            CatalogError::Sqlite(rusqlite::Error::SqliteFailure(f, _))
                if matches!(
                    f.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                )
        )
    }
}

impl From<rusqlite::Error> for CatalogError {
    fn from(e: rusqlite::Error) -> Self {
        if let rusqlite::Error::SqliteFailure(f, msg) = &e {
            let ext = f.extended_code;
            if ext == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE
                || ext == rusqlite::ffi::SQLITE_CONSTRAINT_PRIMARYKEY
            {
                return CatalogError::Duplicate(msg.clone().unwrap_or_else(|| f.to_string()));
            }
        }
        CatalogError::Sqlite(e)
    }
}

impl CatalogError {
    pub(crate) fn not_found(entity: &'static str, id: impl ToString) -> Self {
        CatalogError::NotFound {
            entity,
            id: id.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CatalogError;

    fn sqlite(code: std::os::raw::c_int) -> CatalogError {
        CatalogError::from(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(code),
            Some("database is locked".into()),
        ))
    }

    #[test]
    fn locked_and_busy_are_busy() {
        assert!(sqlite(rusqlite::ffi::SQLITE_BUSY).is_busy());
        assert!(sqlite(rusqlite::ffi::SQLITE_LOCKED).is_busy());
    }

    #[test]
    fn other_errors_are_not_busy() {
        assert!(!sqlite(rusqlite::ffi::SQLITE_CORRUPT).is_busy());
        assert!(!CatalogError::Invalid("x".into()).is_busy());
    }
}
