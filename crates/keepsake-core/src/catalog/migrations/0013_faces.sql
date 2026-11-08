-- Keepsake catalog schema v13: faces and people, on this computer only
-- (ADR-040).
--
-- face_scans: the assets whose thumbnail was searched for faces, with which
-- models, and the size of the picture searched (so a scan with no faces is
-- remembered too). faces: each face found, its box and landmarks in 0–1
-- units of the picture, the detector's score and the recogniser's
-- embedding (int8; times emb_scale gives the unit vector). Both are a
-- cache: scanning again makes them again.
--
-- people, and a face's person and state, are the user's decisions:
-- 'suggested' (found by Name and find, not yet confirmed) or 'confirmed'.
-- A face with no person has state 'unknown'. face_rejections: "not this
-- person", so a face is not suggested for them again. The confirmed faces
-- and rejections are also written to the XMP sidecar (ADR-040 step 4).

CREATE TABLE people (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL CHECK (length(trim(name)) > 0),
  -- The name as compared: trimmed, lower case. One person per name.
  name_key   TEXT NOT NULL UNIQUE,
  created_ms INTEGER NOT NULL
) STRICT;

CREATE TABLE face_scans (
  asset_id BLOB PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  model    TEXT NOT NULL CHECK (length(model) > 0),
  width    INTEGER NOT NULL CHECK (width > 0),
  height   INTEGER NOT NULL CHECK (height > 0),
  made_ms  INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE TABLE faces (
  id        INTEGER PRIMARY KEY,
  asset_id  BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  x         REAL NOT NULL CHECK (x BETWEEN 0 AND 1),
  y         REAL NOT NULL CHECK (y BETWEEN 0 AND 1),
  w         REAL NOT NULL CHECK (w > 0 AND w <= 1),
  h         REAL NOT NULL CHECK (h > 0 AND h <= 1),
  -- 10 little-endian f32: the 5 landmarks' x, y, in 0–1 units.
  landmarks BLOB NOT NULL CHECK (length(landmarks) = 40),
  score     REAL NOT NULL CHECK (score BETWEEN 0 AND 1),
  model     TEXT NOT NULL CHECK (length(model) > 0),
  embedding BLOB NOT NULL CHECK (length(embedding) > 0),
  emb_scale REAL NOT NULL CHECK (emb_scale >= 0),
  -- A person is removed only after their faces are made 'unknown' again.
  person_id INTEGER REFERENCES people(id),
  state     TEXT NOT NULL DEFAULT 'unknown'
            CHECK (state IN ('unknown', 'suggested', 'confirmed')),
  -- A suggestion's likeness to the person's confirmed faces (cosine).
  likeness  REAL,
  CHECK ((person_id IS NULL) = (state = 'unknown'))
) STRICT;
CREATE INDEX faces_asset ON faces(asset_id);
CREATE INDEX faces_person ON faces(person_id, state) WHERE person_id IS NOT NULL;

CREATE TABLE face_rejections (
  face_id   INTEGER NOT NULL REFERENCES faces(id) ON DELETE CASCADE,
  person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
  PRIMARY KEY (face_id, person_id)
) STRICT, WITHOUT ROWID;
