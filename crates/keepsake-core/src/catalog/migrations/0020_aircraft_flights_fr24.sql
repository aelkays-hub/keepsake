-- Keepsake catalog schema v20: what Flightradar24 said a registration flew
-- on a UTC day (ADR-044, Add route), so the same registration and day is
-- asked once. Flightradar24's API terms allow keeping its data 30 days:
-- rows asked longer ago are deleted (`route::forget_expired`).
-- JSON written by `ai::aircraft::route::Flight` (a list; empty: none known).

CREATE TABLE aircraft_flights_fr24 (
  registration TEXT NOT NULL CHECK (length(registration) BETWEEN 1 AND 16),
  day          TEXT NOT NULL CHECK (length(day) = 10),
  flights      TEXT NOT NULL,
  asked_ms     INTEGER NOT NULL,
  PRIMARY KEY (registration, day)
) STRICT, WITHOUT ROWID;

CREATE INDEX aircraft_flights_fr24_asked ON aircraft_flights_fr24 (asked_ms);
