-- Keepsake catalog schema v9: on-device AI (ADR-035).
--
-- ai_analysis is a cache, made from an asset's thumbnail by the `ai.analyze`
-- job: the image embedding (int8, times emb_scale gives the unit vector) and
-- a 64-bit difference hash. Deleting it only means the work is done again.
--
-- ai_tag_rejections and duplicate_choices are the user's decisions. They are
-- also written to the asset's XMP sidecar, so a rebuild restores them.
-- Rejections are stored by tag path (Lightroom form, 'A|B'), not by tag id,
-- so pruning unused tags never drops them.

CREATE TABLE ai_analysis (
  asset_id   BLOB PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  model      TEXT NOT NULL CHECK (length(model) > 0),
  embedding  BLOB NOT NULL,
  emb_scale  REAL NOT NULL CHECK (emb_scale >= 0),
  dhash      INTEGER NOT NULL,
  made_ms    INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE ai_tag_rejections (
  asset_id BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_path TEXT NOT NULL CHECK (length(tag_path) > 0),
  PRIMARY KEY (asset_id, tag_path)
) STRICT, WITHOUT ROWID;

CREATE TABLE duplicate_choices (
  asset_id BLOB PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  choice   TEXT NOT NULL CHECK (choice IN ('keeper','not_kept','not_duplicate')),
  made_ms  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
