use rusqlite::params;

use super::define_repo;
use crate::catalog::error::CatalogError;
use crate::catalog::AssetId;
use crate::relpath::nfc;

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
