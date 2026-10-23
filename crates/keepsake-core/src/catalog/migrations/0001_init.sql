-- Keepsake catalog schema v1.
--
-- Conventions: UUIDs are BLOB(16), hashes are BLAKE3 BLOB(32), *_ms columns are
-- Unix epoch milliseconds UTC. Every table is STRICT.
-- The migration runner sets PRAGMA user_version after this script succeeds.

-- ── Library ──────────────────────────────────────────────────────────────
CREATE TABLE library_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

-- ── Volumes ──────────────────────────────────────────────────────────────
CREATE TABLE volumes (
  id             BLOB PRIMARY KEY CHECK (length(id) = 16),
  label          TEXT NOT NULL,
  fs_id          TEXT NOT NULL UNIQUE,
  kind           TEXT NOT NULL CHECK (kind IN ('card','internal','external','network')),
  case_sensitive INTEGER NOT NULL CHECK (case_sensitive IN (0,1)),
  read_only      INTEGER NOT NULL CHECK (read_only IN (0,1)),
  online         INTEGER NOT NULL DEFAULT 0 CHECK (online IN (0,1)),
  mount_path     TEXT,
  last_seen_ms   INTEGER,
  CHECK (online = 0 OR mount_path IS NOT NULL)
) STRICT;

-- ── Events ───────────────────────────────────────────────────────────────
CREATE TABLE events (
  id         BLOB PRIMARY KEY CHECK (length(id) = 16),
  title      TEXT NOT NULL,
  place_name TEXT,
  lat        REAL CHECK (lat BETWEEN -90 AND 90),
  lon        REAL CHECK (lon BETWEEN -180 AND 180),
  created_ms INTEGER NOT NULL
) STRICT;

-- ── Labels ───────────────────────────────────────────────────────────────
CREATE TABLE labels (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  color_hex  TEXT CHECK (color_hex IS NULL OR length(color_hex) = 7),
  sort_order INTEGER NOT NULL
) STRICT;
INSERT INTO labels (id, name, color_hex, sort_order) VALUES
  (1, 'Red',    '#E5484D', 1),
  (2, 'Yellow', '#F5D90A', 2),
  (3, 'Green',  '#30A46C', 3),
  (4, 'Blue',   '#0091FF', 4),
  (5, 'Purple', '#8E4EC6', 5);

-- ── Assets ───────────────────────────────────────────────────────────────
CREATE TABLE assets (
  seq                    INTEGER PRIMARY KEY,
  id                     BLOB NOT NULL UNIQUE CHECK (length(id) = 16),
  media_kind             TEXT NOT NULL CHECK (media_kind IN ('photo','video')),
  capture_ts_ms          INTEGER,
  capture_utc_offset_min INTEGER CHECK (capture_utc_offset_min BETWEEN -1080 AND 1080),
  capture_shift_s        INTEGER NOT NULL DEFAULT 0,
  capture_local_date     TEXT GENERATED ALWAYS AS (
      CASE WHEN capture_ts_ms IS NULL THEN NULL
           ELSE date(capture_ts_ms / 1000 + ifnull(capture_utc_offset_min, 0) * 60, 'unixepoch')
      END) STORED,
  duration_ms            INTEGER CHECK (duration_ms >= 0),
  camera_make            TEXT,
  camera_model           TEXT,
  lens_model             TEXT,
  event_id               BLOB REFERENCES events(id) ON DELETE SET NULL,
  rating                 INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
  label_id               INTEGER REFERENCES labels(id) ON DELETE SET NULL,
  comment                TEXT,
  created_ms             INTEGER NOT NULL,
  metadata_ms            INTEGER NOT NULL
) STRICT;
CREATE INDEX assets_capture_ts ON assets(capture_ts_ms);
CREATE INDEX assets_local_date ON assets(capture_local_date);
CREATE INDEX assets_event      ON assets(event_id);
CREATE INDEX assets_rating     ON assets(rating) WHERE rating > 0;
CREATE INDEX assets_label      ON assets(label_id) WHERE label_id IS NOT NULL;

-- ── Files ────────────────────────────────────────────────────────────────
CREATE TABLE files (
  id           BLOB PRIMARY KEY CHECK (length(id) = 16),
  asset_id     BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  volume_id    BLOB NOT NULL REFERENCES volumes(id) ON DELETE RESTRICT,
  rel_path     TEXT NOT NULL,
  rel_path_key TEXT NOT NULL,
  file_name    TEXT NOT NULL,
  role         TEXT NOT NULL CHECK (role IN
                 ('original','raw_pair','sidecar','camera_proxy','app_proxy','export')),
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  blake3       BLOB NOT NULL CHECK (length(blake3) = 32),
  xxh3         INTEGER,
  mtime_ms     INTEGER,
  UNIQUE (volume_id, rel_path_key)
) STRICT;
CREATE INDEX files_asset  ON files(asset_id);
CREATE INDEX files_blake3 ON files(blake3);
-- One original per asset per volume (backup destinations hold their own copy).
CREATE UNIQUE INDEX files_one_original ON files(asset_id, volume_id) WHERE role = 'original';

-- ── XMP sidecar tracking ─────────────────────────────────────────────────
-- One sidecar per copy of the original: each volume carries its own.
CREATE TABLE xmp_sidecars (
  asset_id  BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  volume_id BLOB NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
  location TEXT NOT NULL CHECK (location IN ('beside','mirror')),
  rel_path TEXT NOT NULL,
  blake3   BLOB NOT NULL CHECK (length(blake3) = 32),
  mtime_ms INTEGER NOT NULL,
  PRIMARY KEY (asset_id, volume_id)
) STRICT, WITHOUT ROWID;

-- ── Tags ─────────────────────────────────────────────────────────────────
CREATE TABLE tags (
  id        INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
  name      TEXT NOT NULL,
  name_key  TEXT NOT NULL
) STRICT;
CREATE UNIQUE INDEX tags_unique ON tags(ifnull(parent_id, 0), name_key);

CREATE TABLE asset_tags (
  asset_id   BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_id     INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  source     TEXT NOT NULL CHECK (source IN ('user','ai')),
  confidence REAL CHECK (confidence BETWEEN 0 AND 1),
  PRIMARY KEY (asset_id, tag_id, source),
  CHECK ((source = 'ai') = (confidence IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE INDEX asset_tags_tag ON asset_tags(tag_id);

-- ── Edit recipes ─────────────────────────────────────────────────────────
CREATE TABLE edit_recipes (
  id             BLOB PRIMARY KEY CHECK (length(id) = 16),
  asset_id       BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  rel_path       TEXT NOT NULL UNIQUE,
  content_hash   BLOB NOT NULL CHECK (length(content_hash) = 32),
  created_ms     INTEGER NOT NULL,
  updated_ms     INTEGER NOT NULL
) STRICT;
CREATE INDEX edit_recipes_asset ON edit_recipes(asset_id);

-- ── Renditions (cache) ───────────────────────────────────────────────────
CREATE TABLE renditions (
  id           BLOB PRIMARY KEY CHECK (length(id) = 16),
  source_hash  BLOB NOT NULL CHECK (length(source_hash) = 32),
  recipe_hash  BLOB NOT NULL CHECK (length(recipe_hash) = 32),
  kind         TEXT NOT NULL CHECK (kind IN ('thumbnail','preview','proxy','export')),
  rel_path     TEXT NOT NULL,
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  created_ms   INTEGER NOT NULL,
  last_used_ms INTEGER NOT NULL,
  UNIQUE (source_hash, recipe_hash, kind)
) STRICT;

-- ── Import sessions ──────────────────────────────────────────────────────
CREATE TABLE import_sessions (
  id               BLOB PRIMARY KEY CHECK (length(id) = 16),
  source_volume_id BLOB NOT NULL REFERENCES volumes(id),
  started_ms       INTEGER NOT NULL,
  finished_ms      INTEGER,
  status           TEXT NOT NULL CHECK (status IN ('running','completed','failed','cancelled')),
  verified         INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0,1))
) STRICT;

CREATE TABLE import_destinations (
  session_id BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  volume_id  BLOB NOT NULL REFERENCES volumes(id),
  verified   INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0,1)),
  PRIMARY KEY (session_id, volume_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE import_items (
  session_id      BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  source_rel_path TEXT NOT NULL,
  dest_volume_id  BLOB NOT NULL REFERENCES volumes(id),
  file_id         BLOB REFERENCES files(id) ON DELETE SET NULL,
  blake3          BLOB CHECK (blake3 IS NULL OR length(blake3) = 32),
  status          TEXT NOT NULL CHECK (status IN ('pending','copied','verified','failed','skipped')),
  error           TEXT,
  PRIMARY KEY (session_id, source_rel_path, dest_volume_id)
) STRICT, WITHOUT ROWID;

-- ── Full-text search (rowid = assets.seq; maintained by catalog::fts) ────
CREATE VIRTUAL TABLE asset_fts USING fts5(
  names,
  comment,
  tags,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix   = '2 3'
);

-- ── Derived view ─────────────────────────────────────────────────────────
CREATE VIEW event_spans AS
  SELECT event_id,
         min(capture_ts_ms) AS start_ms,
         max(capture_ts_ms) AS end_ms,
         count(*)           AS n
  FROM assets
  WHERE event_id IS NOT NULL
  GROUP BY event_id;
