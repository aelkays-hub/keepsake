-- Keepsake catalog schema v19: what OpenSky said an aircraft flew on a UTC
-- day (ADR-044, Add route), so the same aircraft and day is asked once.
-- JSON written by `ai::aircraft::route::Flight` (a list; empty: none known).

CREATE TABLE aircraft_flights (
  icao24   TEXT NOT NULL CHECK (length(icao24) = 6),
  day      TEXT NOT NULL CHECK (length(day) = 10),
  flights  TEXT NOT NULL,
  asked_ms INTEGER NOT NULL,
  PRIMARY KEY (icao24, day)
) STRICT, WITHOUT ROWID;
