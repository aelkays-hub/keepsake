-- Keepsake catalog schema v10: finding duplicates and tagging are separate
-- (ADR-035). An analysis made to find duplicates has no AI tags yet;
-- `tagged` says whether AI tags were given from it. Rows from v9 were
-- always tagged.

ALTER TABLE ai_analysis ADD COLUMN tagged INTEGER NOT NULL DEFAULT 1 CHECK (tagged IN (0,1));
