-- Keepsake catalog schema v5: browsing large libraries.
--
-- The grid and details views sort and filter 300,000 assets by name, size
-- and type. Those live on the original file, so each asset keeps a copy:
--   primary_name  the original's file name (any volume; all copies match)
--   size_bytes    the original's size
--   is_raw        1 if the asset has a RAW file (original or raw pair)
-- Triggers on `files` keep them current; the catalog code never writes them.

ALTER TABLE assets ADD COLUMN primary_name TEXT;
ALTER TABLE assets ADD COLUMN size_bytes INTEGER;
ALTER TABLE assets ADD COLUMN is_raw INTEGER NOT NULL DEFAULT 0 CHECK (is_raw IN (0,1));

-- Extensions treated as RAW (lower case, with the dot).
CREATE TABLE raw_extensions (ext TEXT PRIMARY KEY) STRICT, WITHOUT ROWID;
INSERT INTO raw_extensions (ext) VALUES
  ('.arw'),('.srf'),('.sr2'),('.dng'),('.cr2'),('.cr3'),('.nef'),('.nrw'),
  ('.raf'),('.orf'),('.rw2'),('.pef'),('.3fr'),('.iiq');

CREATE VIEW file_is_raw AS
  SELECT f.id AS file_id, f.asset_id,
         EXISTS (SELECT 1 FROM raw_extensions r
                  WHERE lower(f.file_name) LIKE '%' || r.ext) AS raw
    FROM files f WHERE f.role IN ('original','raw_pair');

-- Recompute one asset's copies from its files.
-- (SQLite triggers cannot call procedures, so each trigger repeats this.)
CREATE TRIGGER files_ai AFTER INSERT ON files BEGIN
  UPDATE assets SET
    primary_name = (SELECT file_name FROM files WHERE asset_id = NEW.asset_id AND role = 'original' ORDER BY id LIMIT 1),
    size_bytes   = (SELECT size_bytes FROM files WHERE asset_id = NEW.asset_id AND role = 'original' ORDER BY id LIMIT 1),
    is_raw       = EXISTS (SELECT 1 FROM file_is_raw WHERE asset_id = NEW.asset_id AND raw)
  WHERE id = NEW.asset_id;
END;

CREATE TRIGGER files_ad AFTER DELETE ON files BEGIN
  UPDATE assets SET
    primary_name = (SELECT file_name FROM files WHERE asset_id = OLD.asset_id AND role = 'original' ORDER BY id LIMIT 1),
    size_bytes   = (SELECT size_bytes FROM files WHERE asset_id = OLD.asset_id AND role = 'original' ORDER BY id LIMIT 1),
    is_raw       = EXISTS (SELECT 1 FROM file_is_raw WHERE asset_id = OLD.asset_id AND raw)
  WHERE id = OLD.asset_id;
END;

CREATE TRIGGER files_au AFTER UPDATE OF file_name, size_bytes, role, asset_id ON files BEGIN
  UPDATE assets SET
    primary_name = (SELECT file_name FROM files WHERE asset_id = NEW.asset_id AND role = 'original' ORDER BY id LIMIT 1),
    size_bytes   = (SELECT size_bytes FROM files WHERE asset_id = NEW.asset_id AND role = 'original' ORDER BY id LIMIT 1),
    is_raw       = EXISTS (SELECT 1 FROM file_is_raw WHERE asset_id = NEW.asset_id AND raw)
  WHERE id IN (NEW.asset_id, OLD.asset_id);
END;

-- Backfill.
UPDATE assets SET
  primary_name = (SELECT file_name FROM files WHERE asset_id = assets.id AND role = 'original' ORDER BY id LIMIT 1),
  size_bytes   = (SELECT size_bytes FROM files WHERE asset_id = assets.id AND role = 'original' ORDER BY id LIMIT 1),
  is_raw       = EXISTS (SELECT 1 FROM file_is_raw WHERE asset_id = assets.id AND raw);

-- Sort and filter indexes (ties broken by capture time, then seq).
CREATE INDEX assets_name     ON assets (primary_name COLLATE NOCASE, capture_ts_ms, seq);
CREATE INDEX assets_camera   ON assets (camera_model COLLATE NOCASE, capture_ts_ms, seq);
CREATE INDEX assets_lens     ON assets (lens_model COLLATE NOCASE, capture_ts_ms, seq);
CREATE INDEX assets_size     ON assets (size_bytes, seq);
CREATE INDEX assets_duration ON assets (duration_ms, seq);
CREATE INDEX assets_kind     ON assets (media_kind, is_raw, capture_ts_ms, seq);
CREATE INDEX assets_rating_all ON assets (rating, capture_ts_ms, seq);
CREATE INDEX assets_capture_seq ON assets (capture_ts_ms, seq);
CREATE INDEX import_assets_asset ON import_assets (asset_id);
CREATE INDEX files_volume ON files (volume_id, asset_id);
