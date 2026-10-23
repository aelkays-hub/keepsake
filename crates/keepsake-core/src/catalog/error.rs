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
