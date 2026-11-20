-- Keepsake catalog schema v21: national aircraft registers (ADR-045):
-- Transport Canada's Canadian Civil Aircraft Register and the FAA's
-- Releasable Aircraft Database, downloaded on demand and imported here.
-- One row per registration (`C-FJZS`, `N492AS`). Only what Keepsake uses:
-- no owner who is a person, and no address, is kept.

CREATE TABLE aircraft_register (
  registration  TEXT NOT NULL PRIMARY KEY CHECK (length(registration) BETWEEN 2 AND 12),
  source        TEXT NOT NULL CHECK (source IN ('tc', 'faa')),
  maker         TEXT,
  model         TEXT,
  -- The type as Keepsake names it (`Boeing 777-300ER`).
  aircraft_type TEXT,
  serial        TEXT,
  -- `2016` or `2016-03-05`.
  built         TEXT CHECK (built IS NULL OR length(built) IN (4, 10)),
  -- The transponder code, hex upper case (`C01A61`).
  mode_s        TEXT CHECK (mode_s IS NULL OR length(mode_s) = 6),
  -- The owner when a company (never a person), and the airline it is.
  owner         TEXT,
  airline       TEXT,
  -- Registered, or not (cancelled, expired, deregistered).
  current       INTEGER NOT NULL CHECK (current IN (0, 1))
) STRICT, WITHOUT ROWID;

CREATE INDEX aircraft_register_mode_s ON aircraft_register (mode_s);

-- When each register was last downloaded, and the file's date.
CREATE TABLE aircraft_register_sources (
  source        TEXT NOT NULL PRIMARY KEY CHECK (source IN ('tc', 'faa')),
  file_modified TEXT,
  imported_ms   INTEGER NOT NULL,
  rows          INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
