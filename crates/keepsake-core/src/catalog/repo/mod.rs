//! Typed repositories. Read methods are available on every transaction; write
//! methods only on repositories from a [`super::WriteTx`].

mod assets;
mod browse;
mod events;
mod files;
mod imports;
mod jobs;
mod labels;
mod meta;
mod recipes;
mod renditions;
mod search;
mod sequences;
mod sidecars;
mod tags;
mod volumes;

pub use assets::{Assets, DayCount};
pub use browse::{
    Browse, BrowseRow, CalendarBucket, CalendarLevel, Facet, FacetColumn, Sort, SortField,
};
pub use events::{EventSpan, Events};
pub use files::Files;
pub use imports::Imports;
pub use jobs::{Job, JobCounts, JobState, Jobs, NewJob, Priority};
pub use labels::Labels;
pub use meta::Meta;
pub use recipes::Recipes;
pub use renditions::Renditions;
pub use search::{Search, SearchQuery};
pub use sequences::Sequences;
pub use sidecars::Sidecars;
pub use tags::Tags;
pub use volumes::Volumes;

use super::error::CatalogError;
use crate::relpath::RelPath;

macro_rules! define_repo {
    ($(#[$m:meta])* $name:ident) => {
        $(#[$m])*
        pub struct $name<'a, M> {
            pub(crate) conn: &'a rusqlite::Connection,
            _mode: std::marker::PhantomData<M>,
        }

        impl<'a, M> $name<'a, M> {
            pub(crate) fn new(conn: &'a rusqlite::Connection) -> Self {
                Self { conn, _mode: std::marker::PhantomData }
            }
        }
    };
}
pub(crate) use define_repo;

pub(crate) fn to_i64(v: u64, what: &str) -> Result<i64, CatalogError> {
    i64::try_from(v).map_err(|_| CatalogError::Invalid(format!("{what} {v} exceeds i64")))
}

pub(crate) fn to_u64(v: i64, what: &str) -> Result<u64, CatalogError> {
    u64::try_from(v).map_err(|_| CatalogError::Invalid(format!("negative {what}: {v}")))
}

/// Parse a stored path, mapping failures to a rusqlite conversion error so it
/// can be used inside row-mapping closures.
pub(crate) fn rel_path_col(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<RelPath> {
    let s: String = row.get(idx)?;
    RelPath::parse(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

pub(crate) fn u64_col(row: &rusqlite::Row<'_>, idx: usize) -> rusqlite::Result<u64> {
    let v: i64 = row.get(idx)?;
    u64::try_from(v).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Integer, Box::new(e))
    })
}
