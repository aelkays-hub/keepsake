-- Keepsake catalog schema v18: when each aircraft was built (ADR-044), by
-- registration: from OpenSky's aircraft database, or typed by the user
-- (which wins). A year (`2014`) or a date (`2014-03-05`). The aircraft's
-- age is worked out from it when shown, never stored.

CREATE TABLE aircraft_built (
  registration TEXT NOT NULL PRIMARY KEY CHECK (length(registration) > 0),
  built        TEXT NOT NULL CHECK (length(built) IN (4, 10)),
  source       TEXT NOT NULL CHECK (source IN ('opensky', 'typed')),
  set_ms       INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
