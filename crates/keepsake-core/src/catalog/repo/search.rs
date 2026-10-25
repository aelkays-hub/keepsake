use rusqlite::params;
use rusqlite::types::ToSql;
use serde::{Deserialize, Serialize};

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::{AssetId, EventId, ImportSessionId, MediaKind, VolumeId};
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
    /// Lens model contains this (case-insensitive).
    #[serde(default)]
    pub lens: Option<String>,
    /// At least this many stars (1–5).
    #[serde(default)]
    pub min_rating: Option<u8>,
    /// `Some(true)`: only assets with a RAW file; `Some(false)`: none.
    #[serde(default)]
    pub raw: Option<bool>,
    /// Colour label name (e.g. `Red`).
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub event: Option<EventId>,
    /// Assets with a file on this volume.
    #[serde(default)]
    pub volume: Option<VolumeId>,
    /// Assets of this import session.
    #[serde(default)]
    pub session: Option<ImportSessionId>,
    /// Only assets without a capture time.
    #[serde(default)]
    pub undated: bool,
}

/// SQL for the filters of a [`SearchQuery`] over `assets a`: joins, WHERE
/// clauses and their arguments, in order.
pub(crate) struct FilterSql {
    pub joins: String,
    pub wh: Vec<String>,
    pub args: Vec<Box<dyn ToSql>>,
    /// The text filter matched nothing searchable: the result is empty.
    pub empty: bool,
    pub fts: bool,
}

impl FilterSql {
    pub fn where_sql(&self) -> String {
        if self.wh.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", self.wh.join(" AND "))
        }
    }
}

pub(crate) fn filter_sql(q: &SearchQuery) -> Result<FilterSql, CatalogError> {
    let mut f = FilterSql {
        joins: String::new(),
        wh: Vec::new(),
        args: Vec::new(),
        empty: false,
        fts: false,
    };
    if let Some(t) = q.text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        match to_fts_query(t) {
            Some(m) => {
                f.joins
                    .push_str(" JOIN asset_fts ON asset_fts.rowid = a.seq AND asset_fts MATCH ?");
                f.args.push(Box::new(m));
                f.fts = true;
            }
            None => f.empty = true,
        }
    }
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
            f.wh.push(clause.into());
            f.args.push(Box::new(d.clone()));
        }
    }
    for (val, col) in [(&q.camera, "a.camera_model"), (&q.lens, "a.lens_model")] {
        if let Some(c) = val.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
            f.wh.push(format!("instr(lower(ifnull({col}, '')), lower(?)) > 0"));
            f.args.push(Box::new(c.to_owned()));
        }
    }
    if let Some(k) = q.kind {
        f.wh.push("a.media_kind = ?".into());
        f.args.push(Box::new(k));
    }
    if let Some(r) = q.min_rating.filter(|r| *r > 0) {
        f.wh.push("a.rating >= ?".into());
        f.args.push(Box::new(i64::from(r.min(5))));
    }
    if let Some(raw) = q.raw {
        f.wh.push(if raw { "a.is_raw = 1" } else { "a.is_raw = 0" }.into());
    }
    if let Some(l) = q.label.as_deref().filter(|l| !l.is_empty()) {
        f.wh.push("a.label_id = (SELECT id FROM labels WHERE lower(name) = lower(?))".into());
        f.args.push(Box::new(l.to_owned()));
    }
    if let Some(e) = q.event {
        f.wh.push("a.event_id = ?".into());
        f.args.push(Box::new(e));
    }
    if let Some(v) = q.volume {
        f.wh.push(
            "EXISTS (SELECT 1 FROM files fv WHERE fv.asset_id = a.id AND fv.volume_id = ?)".into(),
        );
        f.args.push(Box::new(v));
    }
    if let Some(s) = q.session {
        f.wh.push(
            "EXISTS (SELECT 1 FROM import_assets ia WHERE ia.asset_id = a.id AND ia.session_id = ?
                     AND ia.state = 'catalogued')"
                .into(),
        );
        f.args.push(Box::new(s));
    }
    if q.undated {
        f.wh.push("a.capture_ts_ms IS NULL".into());
    }
    Ok(f)
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
        let mut f = filter_sql(q)?;
        if f.empty {
            return Ok(Vec::new());
        }
        let mut sql = format!("SELECT a.id FROM assets a{}{}", f.joins, f.where_sql());
        sql.push_str(if f.fts {
            " ORDER BY bm25(asset_fts), a.id"
        } else {
            " ORDER BY a.capture_ts_ms IS NULL, a.capture_ts_ms, a.id"
        });
        sql.push_str(" LIMIT ?");
        f.args
            .push(Box::new(i64::from(q.limit.unwrap_or(u32::MAX))));
        let mut st = self.conn.prepare(&sql)?;
        let rows = st
            .query_map(rusqlite::params_from_iter(f.args.iter()), |r| r.get(0))?
            .collect::<Result<_, _>>()?;
        Ok(rows)
    }
}

/// Turn free text into a safe FTS5 query: `"word1"* "word2"*`.
pub(crate) fn to_fts_query(query: &str) -> Option<String> {
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
