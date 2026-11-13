-- Keepsake catalog schema v16: Read aircraft keeps what it read, for review
-- (ADR-041, addition). Reading no longer adds keywords by itself: each
-- photo's possible marks are kept with their confidence (JSON written by
-- `ai::aircraft::marks::Candidates`), and the user accepts or dismisses
-- them in the review window at the confidence they choose.

CREATE TABLE aircraft_reads (
  asset_id    BLOB NOT NULL PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  candidates  TEXT NOT NULL CHECK (length(candidates) > 0),
  -- The type the picture looks like, when one stands out.
  looks_like  TEXT,
  -- Read from the full-size picture (else from the thumbnail).
  full_size   INTEGER NOT NULL CHECK (full_size IN (0, 1)),
  made_ms     INTEGER NOT NULL,
  review      TEXT NOT NULL DEFAULT 'waiting'
              CHECK (review IN ('waiting', 'accepted', 'dismissed')),
  reviewed_ms INTEGER
) STRICT, WITHOUT ROWID;

CREATE INDEX aircraft_reads_review ON aircraft_reads(review);
