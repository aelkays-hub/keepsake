-- Keepsake catalog schema v15: edit suggestions (ADR-042).
-- What Keepsake measured on a photo's thumbnail, and what an AI the user
-- asked advised, kept so that opening the suggestions again is instant and
-- free. Both are JSON written by `ai::suggest`. Catalog only: nothing is
-- written next to the original.

CREATE TABLE edit_suggestions (
  asset_id BLOB NOT NULL PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  -- The measurements (`ai::suggest::Measures`).
  measures TEXT NOT NULL CHECK (length(measures) > 0),
  made_ms  INTEGER NOT NULL,
  -- The AI's advice (`ai::suggest::AiAdvice`), once asked for.
  ai       TEXT
) STRICT, WITHOUT ROWID;
