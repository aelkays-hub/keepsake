-- Keepsake catalog schema v11: which version of the AI tag list an
-- asset's tags came from (ADR-035). 0: not tagged. Photos tagged with an
-- older list are tagged again, from their stored analysis, the next time
-- Tag photos runs on their dates. Rows tagged before v11 used list 1.

ALTER TABLE ai_analysis ADD COLUMN tag_list INTEGER NOT NULL DEFAULT 0 CHECK (tag_list >= 0);
UPDATE ai_analysis SET tag_list = tagged;
