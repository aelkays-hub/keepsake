use rusqlite::params;
use rusqlite::types::ToSql;
use serde::{Deserialize, Serialize};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, MediaKind};
use crate::relpath::nfc;

/// A search with optional filters. Every field narrows the result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQuery {
    /// Words matched (as prefixes) against file names, comments and tags.
    pub text: Option<String>,
    /// First local capture day, `YYYY-MM-DD`, inclusive.
    pub from: Option<String>,
    /// Last local capture day, `YYYY-MM-DD`, inclusive.
    pub to: Option<String>,
    /// Camera model contains this (case-insensitive).
    pub camera: Option<String>,
    pub kind: Option<MediaKind>,
    pub limit: Option<u32>,
}

fn valid_day(d: &str) -> bool {
    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok()
}

define_repo!(
    /// Full-text search over file names, comments and tags.
    Search
);

impl<M> Search<'_, M> {
    /// Find assets matching every word of `query` (prefix match), best first.
    ///
    /// User input is never passed to FTS5 as query syntax: each word is quoted,
    /// so characters like `"`, `*`, `-` or `NEAR` are treated as text.
    pub fn text(&self, query: &str, limit: u32) -> Result<Vec<AssetId>, CatalogError> {
        let Some(fts_query) = to_fts_query(query) else {
            return Ok(Vec::new());
        };
        let mut st = self.conn.prepare(
            "SELECT a.id FROM asset_fts f JOIN assets a ON a.seq = f.rowid
              WHERE asset_fts MATCH ?1 ORDER BY bm25(asset_fts), a.id LIMIT ?2",
        )?;
        let rows = st
            .query_map(params![fts_query, limit], |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

impl<M> Search<'_, M> {
    /// Run a filtered search. With text, best matches first; without, by
    /// capture time (undated last).
    pub fn query(&self, q: &SearchQuery) -> Result<Vec<AssetId>, CatalogError> {
        let mut sql = String::from("SELECT a.id FROM assets a");
        let mut args: Vec<Box<dyn ToSql>> = Vec::new();
        let fts = match q.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
            Some(t) => match to_fts_query(t) {
                Some(f) => Some(f),
                None => return Ok(Vec::new()),
            },
            None => None,
        };
        if let Some(f) = &fts {
            sql.push_str(" JOIN asset_fts ON asset_fts.rowid = a.seq AND asset_fts MATCH ?");
            args.push(Box::new(f.clone()));
        }
        let mut wh: Vec<&str> = Vec::new();
        for (val, clause) in [
            (&q.from, "a.capture_local_date >= ?"),
            (&q.to, "a.capture_local_date <= ?"),
        ] {
            if let Some(d) = val {
                if !valid_day(d) {
                    return Err(CatalogError::Invalid(format!(
                        "not a date (YYYY-MM-DD): {d}"
                    )));
                }
                wh.push(clause);
                args.push(Box::new(d.clone()));
            }
        }
        if let Some(c) = q.camera.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            wh.push("instr(lower(ifnull(a.camera_model, '')), lower(?)) > 0");
            args.push(Box::new(c.to_owned()));
        }
        if let Some(k) = q.kind {
            wh.push("a.media_kind = ?");
            args.push(Box::new(k));
        }
        if !wh.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&wh.join(" AND "));
        }
        sql.push_str(if fts.is_some() {
            " ORDER BY bm25(asset_fts), a.id"
        } else {
            " ORDER BY a.capture_ts_ms IS NULL, a.capture_ts_ms, a.id"
        });
        sql.push_str(" LIMIT ?");
        args.push(Box::new(i64::from(q.limit.unwrap_or(u32::MAX))));
        let mut st = self.conn.prepare(&sql)?;
        let rows = st
            .query_map(rusqlite::params_from_iter(args.iter()), |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

/// Turn free text into a safe FTS5 query: `"word1"* "word2"*`.
fn to_fts_query(query: &str) -> Option<String> {
    let terms: Vec<String> = nfc(query)
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::to_fts_query;

    #[test]
    fn quotes_terms() {
        assert_eq!(
            to_fts_query("tokyo  C0001").as_deref(),
            Some("\"tokyo\"* \"C0001\"*")
        );
        assert_eq!(
            to_fts_query("a\"b NEAR(c)").as_deref(),
            Some("\"a\"* \"b\"* \"NEAR\"* \"c\"*")
        );
        assert_eq!(to_fts_query("  ** "), None);
    }
}
