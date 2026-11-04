-- Schema v7: files added in place can wait for their full fingerprint.
--
-- `fingerprinted = 0`: `blake3` holds a stand-in key made from the file's
-- place, size and quick hash (see `FileHash::Pending`), used as its
-- rendition cache key until the background job reads the whole file and
-- stores the real BLAKE3. Every existing row is fingerprinted.
ALTER TABLE files ADD COLUMN fingerprinted INTEGER NOT NULL DEFAULT 1
  CHECK (fingerprinted IN (0, 1));
CREATE INDEX files_unfingerprinted ON files (asset_id) WHERE fingerprinted = 0;
