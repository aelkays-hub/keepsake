-- Keepsake catalog schema v22: Flickr upload (ADR-046).
--
-- flickr_uploads: every photo uploaded, so one is not uploaded twice
-- unknowingly (a photo can be uploaded again on purpose: one row each).
-- flickr_queue: what the user chose for an upload still to be done (the
-- `flickr.upload` job of the same asset): privacy, title and tags.
-- Nothing here is a secret: the API key and the sign-in stay in the OS's
-- credential store.

CREATE TABLE flickr_uploads (
  id          INTEGER PRIMARY KEY,
  asset_id    BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  -- Flickr's id of the account (`21207597@N07`) and of the photo.
  user_nsid   TEXT NOT NULL CHECK (length(user_nsid) > 0),
  photo_id    TEXT NOT NULL CHECK (length(photo_id) > 0),
  url         TEXT NOT NULL,
  privacy     TEXT NOT NULL
              CHECK (privacy IN ('public','friends_family','friends','family','private')),
  -- The tags sent, a JSON array of strings.
  tags        TEXT NOT NULL DEFAULT '[]',
  uploaded_ms INTEGER NOT NULL,
  UNIQUE (user_nsid, photo_id)
) STRICT;

CREATE INDEX flickr_uploads_asset ON flickr_uploads (asset_id);

CREATE TABLE flickr_queue (
  asset_id    BLOB NOT NULL PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  privacy     TEXT NOT NULL
              CHECK (privacy IN ('public','friends_family','friends','family','private')),
  title       TEXT NOT NULL,
  description TEXT NOT NULL DEFAULT '',
  -- The tags to send, a JSON array of strings.
  tags        TEXT NOT NULL DEFAULT '[]',
  queued_ms   INTEGER NOT NULL
) STRICT;
