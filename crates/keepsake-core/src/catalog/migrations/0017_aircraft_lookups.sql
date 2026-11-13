-- Keepsake catalog schema v17: what an online registration lookup said
-- (ADR-041, addition), found or not, so a registration is asked about once.
-- JSON written by `ai::aircraft::lookup::Found`.

CREATE TABLE aircraft_lookups (
  registration TEXT NOT NULL PRIMARY KEY CHECK (length(registration) > 0),
  found        TEXT NOT NULL CHECK (length(found) > 0),
  looked_ms    INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
