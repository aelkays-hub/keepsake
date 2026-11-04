-- Keepsake catalog schema v2: probing results.
--
-- Runs with foreign keys switched off (see migrations.rs) because `files` is
-- rebuilt to widen its role CHECK; the runner verifies foreign keys afterwards.

-- ── Capture provenance and GPS on assets ─────────────────────────────────
ALTER TABLE assets ADD COLUMN capture_source TEXT
  CHECK (capture_source IN ('sony_xml','exif','avchd_mdpm','container','sidecar'));
ALTER TABLE assets ADD COLUMN capture_confidence TEXT
  CHECK (capture_confidence IN ('high','medium','low'));
ALTER TABLE assets ADD COLUMN gps_lat   REAL CHECK (gps_lat BETWEEN -90 AND 90);
ALTER TABLE assets ADD COLUMN gps_lon   REAL CHECK (gps_lon BETWEEN -180 AND 180);
ALTER TABLE assets ADD COLUMN gps_alt_m REAL;

-- ── files: add the 'segment' role (AVCHD split recordings) ───────────────
CREATE TABLE files_v2 (
  id            BLOB PRIMARY KEY CHECK (length(id) = 16),
  asset_id      BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  volume_id     BLOB NOT NULL REFERENCES volumes(id) ON DELETE RESTRICT,
  rel_path      TEXT NOT NULL,
  rel_path_key  TEXT NOT NULL,
  file_name     TEXT NOT NULL,
  role          TEXT NOT NULL CHECK (role IN
                  ('original','raw_pair','sidecar','camera_proxy','app_proxy','export','segment')),
  size_bytes    INTEGER NOT NULL CHECK (size_bytes >= 0),
  blake3        BLOB NOT NULL CHECK (length(blake3) = 32),
  xxh3          INTEGER,
  mtime_ms      INTEGER,
  -- Position in the recording: 0 for the original, 1.. for segments.
  segment_index INTEGER CHECK (segment_index >= 0),
  UNIQUE (volume_id, rel_path_key),
  CHECK ((role = 'segment') = (segment_index IS NOT NULL AND segment_index > 0))
) STRICT;
INSERT INTO files_v2 (id, asset_id, volume_id, rel_path, rel_path_key, file_name, role,
                      size_bytes, blake3, xxh3, mtime_ms, segment_index)
  SELECT id, asset_id, volume_id, rel_path, rel_path_key, file_name, role,
         size_bytes, blake3, xxh3, mtime_ms, NULL
  FROM files;
DROP TABLE files;
ALTER TABLE files_v2 RENAME TO files;
CREATE INDEX files_asset  ON files(asset_id);
CREATE INDEX files_blake3 ON files(blake3);
-- One original per asset per volume (backup destinations hold their own copy).
CREATE UNIQUE INDEX files_one_original ON files(asset_id, volume_id) WHERE role = 'original';
CREATE UNIQUE INDEX files_segment_order ON files(asset_id, volume_id, segment_index)
  WHERE segment_index IS NOT NULL;

-- ── Bursts, brackets and interval runs ───────────────────────────────────
CREATE TABLE sequences (
  id        BLOB PRIMARY KEY CHECK (length(id) = 16),
  volume_id BLOB NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL CHECK (kind IN ('burst','interval','bracket'))
) STRICT;

CREATE TABLE asset_sequences (
  asset_id    BLOB PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  sequence_id BLOB NOT NULL REFERENCES sequences(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL CHECK (position >= 0),
  UNIQUE (sequence_id, position)
) STRICT, WITHOUT ROWID;
CREATE INDEX asset_sequences_seq ON asset_sequences(sequence_id);
