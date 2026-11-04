-- Keepsake catalog schema v3: resumable ingest.
--
-- Runs with foreign keys switched off (see migrations.rs) because
-- `import_items` is rebuilt; the runner verifies foreign keys afterwards.

-- ── Session settings ─────────────────────────────────────────────────────
ALTER TABLE import_sessions ADD COLUMN template TEXT;
ALTER TABLE import_sessions ADD COLUMN safe_to_format INTEGER NOT NULL DEFAULT 0
  CHECK (safe_to_format IN (0,1));

-- ── One row per asset of a session: the ingest state machine ────────────
CREATE TABLE import_assets (
  session_id  BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  source_key  TEXT NOT NULL,              -- rel_path_key of the source original
  asset_id    BLOB NOT NULL CHECK (length(asset_id) = 16),
  diff        TEXT NOT NULL CHECK (diff IN ('new','already_imported','modified')),
  state       TEXT NOT NULL CHECK (state IN ('discovered','selected','copying','copied',
                                             'verified','catalogued','failed','skipped')),
  error       TEXT,
  attempts    INTEGER NOT NULL DEFAULT 0,
  updated_ms  INTEGER NOT NULL,
  PRIMARY KEY (session_id, source_key)
) STRICT, WITHOUT ROWID;

-- Older rows (none are written by released code) become one asset each.
INSERT INTO import_assets (session_id, source_key, asset_id, diff, state, updated_ms)
  SELECT DISTINCT session_id, lower(source_rel_path), zeroblob(16), 'new',
         CASE WHEN status = 'verified' THEN 'verified' ELSE 'failed' END, 0
  FROM import_items;

-- ── import_items rebuilt: wider status list and copy bookkeeping ────────
CREATE TABLE import_items_v2 (
  session_id      BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  source_rel_path TEXT NOT NULL,
  dest_volume_id  BLOB NOT NULL REFERENCES volumes(id),
  source_key      TEXT NOT NULL,
  role            TEXT NOT NULL CHECK (role IN
                    ('original','raw_pair','sidecar','camera_proxy','app_proxy','export','segment')),
  segment_index   INTEGER CHECK (segment_index > 0),
  size_bytes      INTEGER NOT NULL CHECK (size_bytes >= 0),
  quick_hash      INTEGER NOT NULL,
  dest_rel_path   TEXT NOT NULL,
  file_id         BLOB REFERENCES files(id) ON DELETE SET NULL,
  blake3          BLOB CHECK (blake3 IS NULL OR length(blake3) = 32),
  verified_blake3 BLOB CHECK (verified_blake3 IS NULL OR length(verified_blake3) = 32),
  status          TEXT NOT NULL CHECK (status IN
                    ('pending','copying','copied','verified','failed','skipped','reused')),
  error           TEXT,
  PRIMARY KEY (session_id, source_rel_path, dest_volume_id),
  FOREIGN KEY (session_id, source_key) REFERENCES import_assets(session_id, source_key)
    ON DELETE CASCADE,
  UNIQUE (dest_volume_id, dest_rel_path, session_id)
) STRICT, WITHOUT ROWID;
INSERT INTO import_items_v2 (session_id, source_rel_path, dest_volume_id, source_key, role,
                             size_bytes, quick_hash, dest_rel_path, file_id, blake3,
                             verified_blake3, status, error)
  SELECT session_id, source_rel_path, dest_volume_id, lower(source_rel_path), 'original',
         0, 0, source_rel_path, file_id, blake3,
         CASE WHEN status = 'verified' THEN blake3 END, status, error
  FROM import_items;
DROP TABLE import_items;
ALTER TABLE import_items_v2 RENAME TO import_items;
CREATE INDEX import_items_asset ON import_items(session_id, source_key);
