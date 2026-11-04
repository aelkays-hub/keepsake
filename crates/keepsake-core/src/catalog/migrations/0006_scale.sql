-- Schema v6: per-volume counts for the sidebar read only an index.
-- (At most one original per asset per volume, so count(*) per volume is
-- the number of assets with an original there.)
CREATE INDEX files_original_volume ON files (volume_id) WHERE role = 'original';
