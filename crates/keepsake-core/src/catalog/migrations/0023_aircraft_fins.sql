-- Keepsake catalog schema v23: fin (fleet) numbers (ADR-052). An airline
-- paints a number on its aircraft (Air Canada on the tail and the nose
-- gear doors, US airlines on the tail and by the cockpit), unique in that
-- airline at one time. Which registration each one stood for, learned from
-- photos accepted with both read, or imported from a CSV file the user
-- chose. Nothing is shipped: no free source gives fin to registration.

CREATE TABLE aircraft_fins (
  -- The airline as Keepsake names it (`Air Canada`).
  airline      TEXT NOT NULL CHECK (length(airline) > 0),
  fin          TEXT NOT NULL CHECK (length(fin) BETWEEN 2 AND 4),
  registration TEXT NOT NULL CHECK (length(registration) BETWEEN 2 AND 12),
  -- When it was so (photo times, or the file's dates; NULL: open).
  first_ms     INTEGER,
  last_ms      INTEGER,
  source       TEXT NOT NULL CHECK (source IN ('learned', 'imported')),
  -- Photos accepted that taught it (0 for imported).
  seen         INTEGER NOT NULL DEFAULT 0,
  made_ms      INTEGER NOT NULL,
  PRIMARY KEY (airline, fin, registration, source)
) STRICT, WITHOUT ROWID;

-- A folder or dates read again for their fins: a photo already accepted
-- or dismissed keeps that when its new read is kept (then this is reset).
ALTER TABLE aircraft_reads ADD COLUMN keep_review INTEGER NOT NULL DEFAULT 0
  CHECK (keep_review IN (0, 1));
