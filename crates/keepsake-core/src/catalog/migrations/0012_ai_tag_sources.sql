-- Keepsake catalog schema v12: which AI gave which keyword (ADR-039).
-- Tag with AI adds keywords to an asset; this records, per provider, the
-- keywords that provider gave, so the inspector can show them. Two
-- providers that give the same keyword each have a row. Also written to the
-- asset's XMP sidecar (ks:AITagSources), so a rebuild restores it.

CREATE TABLE ai_tag_sources (
  asset_id BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  provider TEXT NOT NULL CHECK (provider IN ('claude','openai','gemini')),
  tag_path TEXT NOT NULL CHECK (length(tag_path) > 0),
  PRIMARY KEY (asset_id, provider, tag_path)
) STRICT, WITHOUT ROWID;
