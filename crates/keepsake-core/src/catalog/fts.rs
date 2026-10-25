//! Maintenance of the `asset_fts` index.
//!
//! The index is rebuilt for one asset whenever its files, comment or tags
//! change, inside the same transaction as the change. It is maintained in Rust
//! rather than with triggers because each row draws on three tables.

use std::collections::BTreeSet;

use rusqlite::{params, Connection, OptionalExtension};

use super::error::CatalogError;
use super::ids::AssetId;

/// The text indexed for one asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FtsText {
    pub names: String,
    pub comment: String,
    pub tags: String,
}

pub(crate) fn compute(conn: &Connection, asset: AssetId) -> Result<FtsText, CatalogError> {
    let mut st = conn.prepare_cached("SELECT DISTINCT file_name FROM files WHERE asset_id = ?1")?;
    let names: BTreeSet<String> = st
        .query_map([asset], |r| r.get(0))?
        .collect::<Result<_, _>>()?;

    let comment: Option<String> = conn
        .prepare_cached("SELECT comment FROM assets WHERE id = ?1")?
        .query_row([asset], |r| r.get(0))
        .optional()?
        .flatten();

    // Every tag on the asset plus all its ancestors.
    let mut st = conn.prepare_cached(
        "WITH RECURSIVE up(id, parent_id, name) AS (
           SELECT t.id, t.parent_id, t.name FROM tags t
             JOIN asset_tags at ON at.tag_id = t.id WHERE at.asset_id = ?1
           UNION
           SELECT t.id, t.parent_id, t.name FROM tags t JOIN up ON t.id = up.parent_id
         ) SELECT DISTINCT name FROM up",
    )?;
    let tags: BTreeSet<String> = st
        .query_map([asset], |r| r.get(0))?
        .collect::<Result<_, _>>()?;

    Ok(FtsText {
        names: names.into_iter().collect::<Vec<_>>().join(" "),
        comment: comment.unwrap_or_default(),
        tags: tags.into_iter().collect::<Vec<_>>().join(" "),
    })
}

fn seq_of(conn: &Connection, asset: AssetId) -> Result<Option<i64>, CatalogError> {
    Ok(conn
        .prepare_cached("SELECT seq FROM assets WHERE id = ?1")?
        .query_row([asset], |r| r.get(0))
        .optional()?)
}

/// Recompute the index row of an asset that must exist.
pub(crate) fn reindex(conn: &Connection, asset: AssetId) -> Result<(), CatalogError> {
    let seq = seq_of(conn, asset)?.ok_or_else(|| CatalogError::not_found("asset", asset))?;
    write_row(conn, seq, &compute(conn, asset)?)
}

/// Recompute the index row of an asset if it still exists.
pub(crate) fn reindex_if_exists(conn: &Connection, asset: AssetId) -> Result<(), CatalogError> {
    match seq_of(conn, asset)? {
        Some(seq) => write_row(conn, seq, &compute(conn, asset)?),
        None => Ok(()),
    }
}

pub(crate) fn remove(conn: &Connection, asset: AssetId) -> Result<(), CatalogError> {
    if let Some(seq) = seq_of(conn, asset)? {
        conn.execute("DELETE FROM asset_fts WHERE rowid = ?1", [seq])?;
    }
    Ok(())
}

fn write_row(conn: &Connection, seq: i64, t: &FtsText) -> Result<(), CatalogError> {
    conn.prepare_cached("DELETE FROM asset_fts WHERE rowid = ?1")?
        .execute([seq])?;
    conn.prepare_cached(
        "INSERT INTO asset_fts (rowid, names, comment, tags) VALUES (?1, ?2, ?3, ?4)",
    )?
    .execute(params![seq, t.names, t.comment, t.tags])?;
    Ok(())
}

/// The stored index row for an asset, if any.
pub(crate) fn stored(conn: &Connection, asset: AssetId) -> Result<Option<FtsText>, CatalogError> {
    Ok(conn
        .query_row(
            "SELECT f.names, f.comment, f.tags FROM asset_fts f
               JOIN assets a ON a.seq = f.rowid WHERE a.id = ?1",
            [asset],
            |r| {
                Ok(FtsText {
                    names: r.get(0)?,
                    comment: r.get(1)?,
                    tags: r.get(2)?,
                })
            },
        )
        .optional()?)
}

pub(crate) fn mismatches(conn: &Connection) -> Result<Vec<AssetId>, CatalogError> {
    let mut st = conn.prepare_cached("SELECT id FROM assets ORDER BY id")?;
    let ids: Vec<AssetId> = st.query_map([], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut bad = Vec::new();
    for id in ids {
        if stored(conn, id)?.as_ref() != Some(&compute(conn, id)?) {
            bad.push(id);
        }
    }
    let strays: i64 = conn.query_row(
        "SELECT count(*) FROM asset_fts WHERE rowid NOT IN (SELECT seq FROM assets)",
        [],
        |r| r.get(0),
    )?;
    if strays > 0 {
        return Err(CatalogError::Invalid(format!(
            "{strays} search index rows have no asset"
        )));
    }
    Ok(bad)
}
