-- Keepsake catalog schema v14: keywords given on this computer (ADR-041).
-- `ai_tag_sources.provider` could only be one of the three cloud AIs. The
-- aircraft reader gives keywords too (`aircraft`), so the list is no longer
-- part of the schema: the code decides which sources it knows
-- (`ai::tag_source_id`). The table is made again without the list; no
-- other table refers to it.

CREATE TABLE ai_tag_sources_new (
  asset_id BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  provider TEXT NOT NULL CHECK (length(provider) > 0),
  tag_path TEXT NOT NULL CHECK (length(tag_path) > 0),
  PRIMARY KEY (asset_id, provider, tag_path)
) STRICT, WITHOUT ROWID;

INSERT INTO ai_tag_sources_new (asset_id, provider, tag_path)
  SELECT asset_id, provider, tag_path FROM ai_tag_sources;

DROP TABLE ai_tag_sources;

ALTER TABLE ai_tag_sources_new RENAME TO ai_tag_sources;
