-- Keepsake catalog schema v4: the persistent job queue.
--
-- One row per (kind, asset). Jobs are claimed in order of priority
-- (0 visible on screen, 1 recent import, 2 backlog), then rank (cheaper
-- kinds first), then sort_key (newest capture first), then age.

CREATE TABLE jobs (
  id            INTEGER PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (length(kind) > 0),
  asset_id      BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  priority      INTEGER NOT NULL CHECK (priority IN (0,1,2)),
  -- The class to fall back to when no longer visible.
  base_priority INTEGER NOT NULL CHECK (base_priority IN (1,2)),
  rank          INTEGER NOT NULL DEFAULT 0,
  sort_key      INTEGER NOT NULL DEFAULT 0,
  state         TEXT NOT NULL CHECK (state IN ('queued','running','done','failed')),
  attempts      INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  error         TEXT,
  enqueued_ms   INTEGER NOT NULL,
  not_before_ms INTEGER NOT NULL DEFAULT 0,
  started_ms    INTEGER,
  finished_ms   INTEGER,
  UNIQUE (kind, asset_id)
) STRICT;

CREATE INDEX jobs_next ON jobs (state, priority, rank, sort_key, id);
CREATE INDEX jobs_asset ON jobs (asset_id);

-- Eviction walks renditions oldest-use first.
CREATE INDEX renditions_lru ON renditions (last_used_ms);
