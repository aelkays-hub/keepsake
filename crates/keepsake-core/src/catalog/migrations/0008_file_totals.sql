-- Schema v8: every asset keeps how many files it has and their total size,
-- for the library report (Tools → Report), so its date, camera and lens
-- breakdowns are one scan of `assets` instead of a join over every file.
--
-- One copy of each file counts: copies of the same file on several drives
-- (backups, a second import destination) share a BLAKE3 key and count once.
-- A file not fingerprinted yet has a stand-in key unique to its place, so
-- it always counts. Kept up to date by the triggers below, like
-- `assets.size_bytes` (v6).
ALTER TABLE assets ADD COLUMN file_count INTEGER NOT NULL DEFAULT 0 CHECK (file_count >= 0);
ALTER TABLE assets ADD COLUMN file_bytes INTEGER NOT NULL DEFAULT 0 CHECK (file_bytes >= 0);

-- The totals of the asset in the row being updated (`assets.id`): the
-- first file (lowest id) of each key. `+g.blake3` keeps SQLite on the
-- per-asset index (`files_asset`, a few rows) rather than `files_blake3`,
-- which for a picture held many times would be many rows per insert.
CREATE TRIGGER files_totals_ai AFTER INSERT ON files BEGIN
  UPDATE assets SET (file_count, file_bytes) = (
    SELECT count(*), ifnull(sum(f.size_bytes), 0) FROM files f
     WHERE f.asset_id = assets.id
       AND NOT EXISTS (SELECT 1 FROM files g
                        WHERE g.asset_id = f.asset_id AND +g.blake3 = f.blake3 AND g.id < f.id))
  WHERE id = NEW.asset_id;
END;

CREATE TRIGGER files_totals_ad AFTER DELETE ON files BEGIN
  UPDATE assets SET (file_count, file_bytes) = (
    SELECT count(*), ifnull(sum(f.size_bytes), 0) FROM files f
     WHERE f.asset_id = assets.id
       AND NOT EXISTS (SELECT 1 FROM files g
                        WHERE g.asset_id = f.asset_id AND +g.blake3 = f.blake3 AND g.id < f.id))
  WHERE id = OLD.asset_id;
END;

-- `blake3` changes when a file's real fingerprint replaces its stand-in key.
CREATE TRIGGER files_totals_au AFTER UPDATE OF size_bytes, blake3, asset_id ON files BEGIN
  UPDATE assets SET (file_count, file_bytes) = (
    SELECT count(*), ifnull(sum(f.size_bytes), 0) FROM files f
     WHERE f.asset_id = assets.id
       AND NOT EXISTS (SELECT 1 FROM files g
                        WHERE g.asset_id = f.asset_id AND +g.blake3 = f.blake3 AND g.id < f.id))
  WHERE id IN (NEW.asset_id, OLD.asset_id);
END;

UPDATE assets SET (file_count, file_bytes) = (
  SELECT count(*), ifnull(sum(f.size_bytes), 0) FROM files f
   WHERE f.asset_id = assets.id
     AND NOT EXISTS (SELECT 1 FROM files g
                      WHERE g.asset_id = f.asset_id AND +g.blake3 = f.blake3 AND g.id < f.id));
