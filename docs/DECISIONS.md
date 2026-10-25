# Architecture Decision Records

---

## ADR-001 — Tauri 2 + Rust core

**Status**: Accepted

**Context**: We need a cross-platform desktop app (Windows 11 x64/ARM64, macOS 14+ Apple Silicon) with a native feel, access to low-level filesystem and device APIs, and a modern web UI. Electron was considered.

**Decision**: Use Tauri 2 with a Rust backend (`keepsake-core`) and a React/TypeScript frontend. Rust handles all I/O-intensive and CPU-intensive work; the webview renders the UI only.

**Consequences**:
- Binary is small (no bundled Chromium engine).
- Full access to Rust's async ecosystem (`tokio`) for concurrent ingest and rendering.
- Tauri commands must remain thin; business logic stays in `keepsake-core` and is testable without a GUI.
- ARM64 Windows requires Tauri 2 (Tauri 1 had partial support); confirmed supported.

**Licence**: Tauri 2 is MIT/Apache-2.0. ✓

---

## ADR-002 — Card-first ingest; native OS adapters instead of libgphoto2

**Status**: Accepted

**Context**: Most Sony cameras expose their card as a standard filesystem over USB or via a card reader. libgphoto2 provides a cross-platform MTP/PTP abstraction but introduces a large C dependency and GPL-adjacent complexity.

**Decision**: Primary ingest path reads the card filesystem directly. OS-native device-arrival events (`WM_DEVICECHANGE` on Windows, IOKit on macOS) detect the card. MTP/PTP (for cameras that do not expose a filesystem) is deferred to v1 and implemented with OS-native APIs (`WPD` on Windows, `IOUSBHost` on macOS) wrapped behind a `MtpTransport` trait in `keepsake-platform`.

**Consequences**:
- No libgphoto2 dependency; no GPL risk.
- Card filesystem reads are faster and more reliable than MTP for bulk transfer.
- MTP path is more work but keeps licencing clean.

---

## ADR-003 — SQLite catalog + XMP sidecars as dual source of truth

**Status**: Accepted

**Context**: The library must be browsable offline and must survive catalog corruption without data loss.

**Decision**: SQLite (WAL mode, FTS5) is the **query cache** for fast search, browsing, and virtual list rendering. XMP sidecars and JSON edit recipes are the **durable source of truth** for all user-authored metadata. Deleting the catalog and running `rebuild-catalog` must restore all user data.

**Consequences**:
- Library is interoperable: other tools that read XMP (Lightroom, darktable, digikam) see Keepsake's ratings, labels, and comments.
- Rebuild is a defined, tested operation.
- Every write of user metadata must update both the catalog row and the XMP sidecar.

**Licence**: `rusqlite` (bundled SQLite) is MIT. SQLite itself is public domain. ✓

---

## ADR-004 — Playback via camera proxies or app-generated H.264 proxies

**Status**: Accepted

**Context**: Modern Sony cameras record XAVC HS in 10-bit 4:2:2 or 4:2:0 HEVC, and older models record AVCHD. Neither codec is decodable by the Chromium webview embedded in Tauri without hardware decoder integration that is out of scope.

**Decision**: Video playback uses the camera-generated proxy file (`PRIVATE/M4ROOT/SUB/*.MP4`) when available **and H.264**. Otherwise (AVCHD, older MP4 cards, HEVC camera proxies), the RenderService generates an H.264 proxy via FFmpeg and caches it. *Updated 2026-10-24 (ADR-021): the proxy is 720p, the size proven in the playback spike, not 1080p; camera proxies are copied into the cache, so they play while the media drive is offline.* The webview plays the proxy via `<video>` with a `blob:` URL.

**Consequences**:
- Playback works reliably across all supported Sony formats.
- Proxy generation takes time; progress is shown in the UI.
- Storage cost for proxies: ~1–3 GB per hour of 4K footage at proxy quality.
- Original files are never read by the webview.

---

## ADR-005 — FFmpeg as LGPL sidecar process

**Status**: Accepted (updated 2026-10-15)

**Context**: FFmpeg provides the best cross-platform support for Sony video formats. Linking FFmpeg statically risks pulling in GPL components. Dynamic linking against a system FFmpeg introduces version fragility. Early versions of the setup script used gyan.dev (Windows) and evermeet.cx (macOS), both of which have URL instability issues; gyan.dev also distributes GPL builds alongside LGPL ones.

**Decision**: Ship pinned LGPL builds of `ffmpeg` and `ffprobe` as sidecar binaries in `vendor/` (excluded from the repo; downloaded by `scripts/setup.sh` / `scripts/setup.ps1`). The app locates them by path and invokes them as child processes. No FFmpeg code is linked into the Keepsake binary.

Every pinned binary (FFmpeg, ExifTool) is mirrored as a GitHub Release asset at `aelkays-hub/keepsake` tag `vendor-binaries-v1` so setup scripts are insulated from upstream URL rot. Setup scripts download exclusively from this mirror and verify SHA-256 before use. If a checksum fails the download is deleted and the script exits with a message naming the file and the mirror URL.

After installation, `setup.ps1` (and `setup.sh` once macOS FFmpeg is added) runs `ffmpeg -version` and checks the configuration string:
- **FAIL** if `--enable-gpl` or `--enable-nonfree` is present — these indicate a GPL or proprietary build.
- **PASS** otherwise; the script prints `LGPLv2.1` or `LGPLv3` depending on whether `--enable-version3` is present.

Both LGPLv2.1 and LGPLv3 builds are acceptable. `--enable-version3` enables code licensed under LGPLv3 (and GPLv3 when combined with GPL components, but only LGPL builds are accepted here), which is a looser copyleft than GPL and fully compatible with Keepsake's distribution model.

**Consequences**:
- Licence is LGPL-2.1 or LGPL-3.0 depending on the build. BtbN LGPL builds disable all GPL and nonfree components. Build provenance is documented per platform below.
- Binary size: ~170 MB (win64 ZIP) / ~118 MB (winarm64 ZIP) for the archive; extracted sidecar pair is larger.
- `vendor/` is gitignored; developers run `scripts/setup.ps1` once to populate it.

**FFmpeg build source**:
- **Windows x64**: BtbN/FFmpeg-Builds — `ffmpeg-n9.0-latest-win64-lgpl-9.0.zip`  
  SHA-256: `73950cc6d7548c08d435bfb9c65b60798b6cd59647e59bb41e9af3876c90ebef`  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`  
  Licence: LGPLv3 (corrected 2026-10-23: the build is configured with `--enable-version3`; `setup.ps1` reports "LGPLv3". No `--enable-gpl`, no `--enable-nonfree`.)

- **Windows ARM64**: BtbN/FFmpeg-Builds — `ffmpeg-n9.0-latest-winarm64-lgpl-9.0.zip`  
  SHA-256: `33cb51e1529c1d658eb814c0cd557272f35a83951d7bb8009ac6bbe7854b0cc6`  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`  
  Licence: LGPL-2.1+

- **macOS (Apple Silicon)**: Deferred. No maintained LGPL prebuilt exists for Apple Silicon. A future prompt will add a CI job that compiles FFmpeg from source (VideoToolbox enabled, no GPL components) and uploads the result to the vendor-binaries mirror.

**ExifTool source** (not FFmpeg, documented here for completeness):
- **Windows 64-bit**: ExifTool 13.59 standalone — `exiftool-13.59_64.zip`  
  SHA-256: `44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec`  
  Original source: sourceforge.net/projects/exiftool  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`  
  Licence: Artistic / GPL (Perl; invoked as a sidecar process, never linked)

- **macOS / Linux**: ExifTool 13.59 Unix tarball — `Image-ExifTool-13.59.tar.gz`  
  SHA-256: `668ea3acececb7235fbd0f4900e72d5f12c9b07e5c778fd36cb1e9b5828fd65a`  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`

---

## ADR-007 — clap for CLI argument parsing

**Status**: Accepted (Prompt 1)

**Context**: The CLI needs subcommands (`corpus scan`, `corpus init-expected`) with typed arguments, help text, and version output. Hand-rolling this with `std::env::args()` is brittle.

**Decision**: Use `clap` v4 with the `derive` feature. It is the de facto standard for Rust CLIs and generates `--help` / `--version` automatically.

**Licence**: MIT OR Apache-2.0. ✓

---

## ADR-008 — toml for corpus/expected.toml serialisation

**Status**: Accepted (Prompt 1)

**Context**: `corpus/expected.toml` is a human-edited file (the product owner fills in capture timestamps, camera models, and confirmation flags). TOML is more readable and diff-friendly than JSON for hand-edited config.

**Decision**: Use the `toml` crate v0.8 for serialising and deserialising `Expected` in the CLI and corpus tests. `keepsake-core` itself remains format-agnostic; the TOML dependency lives only in `keepsake-cli` and as a dev-dependency in `keepsake-core`.

**Licence**: MIT. ✓

---

## ADR-006 — Sony Camera Remote SDK as optional plug-in

**Status**: Accepted

**Context**: Sony's Camera Remote SDK enables live-view tethering and remote shutter for supported camera bodies. However, it requires accepting Sony's SDK licence and is not open-source.

**Decision**: Tethering is implemented as an optional plug-in crate (`keepsake-plugin-sony-crs`) that is compiled only when the SDK headers are present and the user opts in. The plug-in communicates with `keepsake-core` through a `TetherAdapter` trait. The core product ships and is fully tested without the plug-in.

**Consequences**:
- No SDK licence burden on users who do not need tethering.
- Sony SDK is not redistributed in the repo.
- Plug-in is not part of the CI matrix; tested separately on hardware.

---

## ADR-009 — Keep Tauri + Rust; defer libmpv (Gate 1 outcome)

**Status**: Accepted (2026-10-16)

**Context**: The Prompt 2 spike tested five playback and performance items on Windows 11
to decide whether Tauri's WebView2 is sufficient or whether we need a native libmpv window
(ADR-004 fallback).

**Findings summary** (`docs/spikes/playback.md`):
- H.264 MP4 (XAVC S corpus clips and generated H.264 proxies) play with seeking in
  WebView2 via the Tauri `asset://` protocol. No extra codec installation required.
- Proxy generation (SW `h264_mf`, no discrete GPU): 5–6× real-time. Acceptable for
  background jobs; GPU path (NVENC/QSV) expected at 10–20×.
- AVCHD `DateTimeOriginal` is readable from in-stream metadata via ExifTool.
- ARW embedded preview via one-shot ExifTool averages 282 ms (target: 150 ms).
  Root cause: Perl process startup, not extraction speed. Solved by ExifTool
  `-stay_open` mode (already in architecture) or LibRaw in-process.
- TanStack Virtual grid of 1,876 items scrolls at 55–60 FPS in WebView2.

**Decision**: Continue with Tauri + WebView2. Do not add libmpv as a dependency.
The ARW timing failure is an ExifTool invocation strategy issue, not a WebView2
limitation. ExifTool stay-open mode resolves it.

**Consequences**:
- libmpv remains deferred. No GPL/LGPL dynamic linking concern from it.
- Proxy generation uses `h264_mf` (SW) on machines without a supported GPU;
  `h264_nvenc` and `h264_qsv` are tried first.
- **The webview must never receive a HEVC or MTS source URL.** Confirmed failures
  on Windows: HEVC seeks take 3 s (software decode, no HEVC Video Extensions
  required); AVCHD MTS audio drifts after 30 s (AC-3/PCR mismatch). Both are
  fixed by always playing H.264 MP4 proxies. This is a non-negotiable constraint.
- The proxy encoder must produce short-GOP H.264 (≤1 s). `h264_mf` does this by
  default (confirmed: 1.00 s avg GOP).
- ExifTool must be used in `-stay_open` mode for all thumbnail/preview extraction
  paths that must meet the 150 ms latency budget.
- HEIF/HIF corpus testing is deferred until source files are available.

---

## ADR-010 — `dirs` crate for OS-standard data directories

**Status**: Accepted (Prompt 3 — packaging)

**Context**: The application needs to store its catalog, cache, and configuration in the
OS-standard locations so the OS file-management tools, backup software, and storage
quotas work correctly. Hard-coding paths (e.g. `~\.keepsake`) is non-portable.

**Decision**: Use the `dirs` crate (v5, MIT) to resolve platform directories:

| Kind | Windows | macOS |
|---|---|---|
| Data (catalog, sidecars cache) | `%LOCALAPPDATA%\keepsake` | `~/Library/Application Support/keepsake` |
| Cache (thumbnails, proxies) | `%LOCALAPPDATA%\keepsake\cache` | `~/Library/Caches/keepsake` |
| Config | `%APPDATA%\keepsake` | `~/Library/Application Support/keepsake` |

These functions live in `keepsake_core::paths`. The Tauri app layer calls them at
startup and passes the resolved `PathBuf`s into `keepsake-core`; `keepsake-core`
itself never depends on Tauri.

**Licence**: `dirs` v5 — MIT. ✓

---

## ADR-011 — Catalog and XMP dependencies

**Status**: Accepted (Prompt 4 — catalog; approved by the product owner 2026-10-23)

| Crate | Version | Licence | Scope | Reason |
|---|---|---|---|---|
| `rusqlite` (`bundled`) | 0.40 | MIT (SQLite: public domain) | runtime | SQLite catalog (ADR-003). The bundled build enables FTS5. |
| `quick-xml` | 0.42 | MIT | runtime | XMP parsing. Keepsake builds its own small tree so foreign properties survive a rewrite. |
| `unicode-normalization` | 0.1 | MIT OR Apache-2.0 | runtime | NFC/NFD handling for paths and tags. |
| `proptest` | 1 | MIT OR Apache-2.0 | dev only | Property tests for paths and rebuild. |
| `tempfile` | 3 | MIT OR Apache-2.0 | dev only | Temporary volumes and libraries in tests. |

No GPL code is linked. Migrations use `PRAGMA user_version` instead of a migration crate.

---

## ADR-012 — Catalog design choices

**Status**: Accepted (Prompt 4)

- **Sidecar naming:** write `<name>.<ext>.xmp` (digiKam/darktable style); read Lightroom's `<stem>.xmp` as a fallback. Lightroom will not see Keepsake edits to raw files until a later option also writes `<stem>.xmp`.
- **Rating and comment are columns on `assets`**, with a `labels` lookup table and `tags` / `asset_tags`. Each is single-valued in XMP, so one-row tables would only add joins.
- **Events live in each member's sidecar** (`keepsake:Event` struct), so a rebuild needs only files and sidecars. Renaming an event rewrites its members' sidecars.
- **Import sessions are catalog-only** and are lost on rebuild (history, not user metadata).
- **Case-insensitive keys use Rust's `to_lowercase` + NFC**, not full Unicode case folding.
- **Non-UTF-8 file names are skipped** and reported by rebuild.
- **Schema:** the final v1 schema and how it differs from the reviewed draft are in ADR-013.
- **Deterministic ids:** file ids are derived from `(volume, rel_path_key)`; assets found without a Keepsake sidecar get an id derived from their original's location. Rebuilds are therefore stable.
- **Volume registry outside the catalog** (`volumes.json`): volume UUIDs must survive catalog deletion, and read-only cards cannot hold a marker file. Until the device watcher exists, `keepsake library add-volume` uses the canonical mount path as the file-system id.

---

## ADR-013 — Catalog schema v1

**Status**: Accepted (Prompt 4). The product owner reviewed and approved the draft on 2026-10-23; the changes below were made during implementation.

**Context**: The catalog is a rebuildable query cache (ADR-003). Schema v1 is the first released migration, so later schema changes must come as new migrations (`0002_*.sql`, …), never as edits to this one.

**Decision**: The schema below is `crates/keepsake-core/src/catalog/migrations/0001_init.sql`. The migration runner sets `PRAGMA user_version = 1` once the script succeeds, and `Catalog::open` sets `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON` and `busy_timeout=5000` on every connection.

### Changes from the reviewed draft

| # | Reviewed draft | Final | Why |
|---|---|---|---|
| 1 | `files_one_original` unique on `(asset_id)` | Unique on `(asset_id, volume_id)` | Ingest copies each original to two or more destinations, and every copy is an original on its own volume. The draft would have rejected the backup copy. |
| 2 | `xmp_sidecars` keyed by `asset_id` alone | Added `volume_id` (FK to `volumes`, `ON DELETE CASCADE`); primary key `(asset_id, volume_id)`; `WITHOUT ROWID` | Each copy has its own sidecar (beside it, or in the mirror for read-only volumes), so rebuilding one volume needs that volume's sidecar. |
| 3 | `PRAGMA user_version = 1` at the end of the SQL | Set by the migration runner after the script, in the same transaction | One place controls versioning for all future migrations. Same result. |

No other tables, columns, constraints or indexes changed.

### Related changes outside the SQL

- `volumes.json` (library registry, not the catalog) gained an optional `last_mount` hint so `keepsake library rebuild` can find volumes after the catalog is deleted. It isn't part of a volume's identity.
- CI creates empty placeholder files in `vendor/`, because `tauri-build` refuses to build when a bundle resource is missing. The release workflow still downloads the real binaries.

### Schema

```sql
-- Keepsake catalog schema v1.
--
-- Conventions: UUIDs are BLOB(16), hashes are BLAKE3 BLOB(32), *_ms columns are
-- Unix epoch milliseconds UTC. Every table is STRICT.
-- The migration runner sets PRAGMA user_version after this script succeeds.

-- ── Library ──────────────────────────────────────────────────────────────
CREATE TABLE library_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
) STRICT, WITHOUT ROWID;

-- ── Volumes ──────────────────────────────────────────────────────────────
CREATE TABLE volumes (
  id             BLOB PRIMARY KEY CHECK (length(id) = 16),
  label          TEXT NOT NULL,
  fs_id          TEXT NOT NULL UNIQUE,
  kind           TEXT NOT NULL CHECK (kind IN ('card','internal','external','network')),
  case_sensitive INTEGER NOT NULL CHECK (case_sensitive IN (0,1)),
  read_only      INTEGER NOT NULL CHECK (read_only IN (0,1)),
  online         INTEGER NOT NULL DEFAULT 0 CHECK (online IN (0,1)),
  mount_path     TEXT,
  last_seen_ms   INTEGER,
  CHECK (online = 0 OR mount_path IS NOT NULL)
) STRICT;

-- ── Events ───────────────────────────────────────────────────────────────
CREATE TABLE events (
  id         BLOB PRIMARY KEY CHECK (length(id) = 16),
  title      TEXT NOT NULL,
  place_name TEXT,
  lat        REAL CHECK (lat BETWEEN -90 AND 90),
  lon        REAL CHECK (lon BETWEEN -180 AND 180),
  created_ms INTEGER NOT NULL
) STRICT;

-- ── Labels ───────────────────────────────────────────────────────────────
CREATE TABLE labels (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  color_hex  TEXT CHECK (color_hex IS NULL OR length(color_hex) = 7),
  sort_order INTEGER NOT NULL
) STRICT;
INSERT INTO labels (id, name, color_hex, sort_order) VALUES
  (1, 'Red',    '#E5484D', 1),
  (2, 'Yellow', '#F5D90A', 2),
  (3, 'Green',  '#30A46C', 3),
  (4, 'Blue',   '#0091FF', 4),
  (5, 'Purple', '#8E4EC6', 5);

-- ── Assets ───────────────────────────────────────────────────────────────
CREATE TABLE assets (
  seq                    INTEGER PRIMARY KEY,
  id                     BLOB NOT NULL UNIQUE CHECK (length(id) = 16),
  media_kind             TEXT NOT NULL CHECK (media_kind IN ('photo','video')),
  capture_ts_ms          INTEGER,
  capture_utc_offset_min INTEGER CHECK (capture_utc_offset_min BETWEEN -1080 AND 1080),
  capture_shift_s        INTEGER NOT NULL DEFAULT 0,
  capture_local_date     TEXT GENERATED ALWAYS AS (
      CASE WHEN capture_ts_ms IS NULL THEN NULL
           ELSE date(capture_ts_ms / 1000 + ifnull(capture_utc_offset_min, 0) * 60, 'unixepoch')
      END) STORED,
  duration_ms            INTEGER CHECK (duration_ms >= 0),
  camera_make            TEXT,
  camera_model           TEXT,
  lens_model             TEXT,
  event_id               BLOB REFERENCES events(id) ON DELETE SET NULL,
  rating                 INTEGER NOT NULL DEFAULT 0 CHECK (rating BETWEEN 0 AND 5),
  label_id               INTEGER REFERENCES labels(id) ON DELETE SET NULL,
  comment                TEXT,
  created_ms             INTEGER NOT NULL,
  metadata_ms            INTEGER NOT NULL
) STRICT;
CREATE INDEX assets_capture_ts ON assets(capture_ts_ms);
CREATE INDEX assets_local_date ON assets(capture_local_date);
CREATE INDEX assets_event      ON assets(event_id);
CREATE INDEX assets_rating     ON assets(rating) WHERE rating > 0;
CREATE INDEX assets_label      ON assets(label_id) WHERE label_id IS NOT NULL;

-- ── Files ────────────────────────────────────────────────────────────────
CREATE TABLE files (
  id           BLOB PRIMARY KEY CHECK (length(id) = 16),
  asset_id     BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  volume_id    BLOB NOT NULL REFERENCES volumes(id) ON DELETE RESTRICT,
  rel_path     TEXT NOT NULL,
  rel_path_key TEXT NOT NULL,
  file_name    TEXT NOT NULL,
  role         TEXT NOT NULL CHECK (role IN
                 ('original','raw_pair','sidecar','camera_proxy','app_proxy','export')),
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  blake3       BLOB NOT NULL CHECK (length(blake3) = 32),
  xxh3         INTEGER,
  mtime_ms     INTEGER,
  UNIQUE (volume_id, rel_path_key)
) STRICT;
CREATE INDEX files_asset  ON files(asset_id);
CREATE INDEX files_blake3 ON files(blake3);
-- One original per asset per volume (backup destinations hold their own copy).
CREATE UNIQUE INDEX files_one_original ON files(asset_id, volume_id) WHERE role = 'original';

-- ── XMP sidecar tracking ─────────────────────────────────────────────────
-- One sidecar per copy of the original: each volume carries its own.
CREATE TABLE xmp_sidecars (
  asset_id  BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  volume_id BLOB NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
  location TEXT NOT NULL CHECK (location IN ('beside','mirror')),
  rel_path TEXT NOT NULL,
  blake3   BLOB NOT NULL CHECK (length(blake3) = 32),
  mtime_ms INTEGER NOT NULL,
  PRIMARY KEY (asset_id, volume_id)
) STRICT, WITHOUT ROWID;

-- ── Tags ─────────────────────────────────────────────────────────────────
CREATE TABLE tags (
  id        INTEGER PRIMARY KEY,
  parent_id INTEGER REFERENCES tags(id) ON DELETE CASCADE,
  name      TEXT NOT NULL,
  name_key  TEXT NOT NULL
) STRICT;
CREATE UNIQUE INDEX tags_unique ON tags(ifnull(parent_id, 0), name_key);

CREATE TABLE asset_tags (
  asset_id   BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  tag_id     INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  source     TEXT NOT NULL CHECK (source IN ('user','ai')),
  confidence REAL CHECK (confidence BETWEEN 0 AND 1),
  PRIMARY KEY (asset_id, tag_id, source),
  CHECK ((source = 'ai') = (confidence IS NOT NULL))
) STRICT, WITHOUT ROWID;
CREATE INDEX asset_tags_tag ON asset_tags(tag_id);

-- ── Edit recipes ─────────────────────────────────────────────────────────
CREATE TABLE edit_recipes (
  id             BLOB PRIMARY KEY CHECK (length(id) = 16),
  asset_id       BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  kind           TEXT NOT NULL,
  schema_version INTEGER NOT NULL,
  rel_path       TEXT NOT NULL UNIQUE,
  content_hash   BLOB NOT NULL CHECK (length(content_hash) = 32),
  created_ms     INTEGER NOT NULL,
  updated_ms     INTEGER NOT NULL
) STRICT;
CREATE INDEX edit_recipes_asset ON edit_recipes(asset_id);

-- ── Renditions (cache) ───────────────────────────────────────────────────
CREATE TABLE renditions (
  id           BLOB PRIMARY KEY CHECK (length(id) = 16),
  source_hash  BLOB NOT NULL CHECK (length(source_hash) = 32),
  recipe_hash  BLOB NOT NULL CHECK (length(recipe_hash) = 32),
  kind         TEXT NOT NULL CHECK (kind IN ('thumbnail','preview','proxy','export')),
  rel_path     TEXT NOT NULL,
  size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
  created_ms   INTEGER NOT NULL,
  last_used_ms INTEGER NOT NULL,
  UNIQUE (source_hash, recipe_hash, kind)
) STRICT;

-- ── Import sessions ──────────────────────────────────────────────────────
CREATE TABLE import_sessions (
  id               BLOB PRIMARY KEY CHECK (length(id) = 16),
  source_volume_id BLOB NOT NULL REFERENCES volumes(id),
  started_ms       INTEGER NOT NULL,
  finished_ms      INTEGER,
  status           TEXT NOT NULL CHECK (status IN ('running','completed','failed','cancelled')),
  verified         INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0,1))
) STRICT;

CREATE TABLE import_destinations (
  session_id BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  volume_id  BLOB NOT NULL REFERENCES volumes(id),
  verified   INTEGER NOT NULL DEFAULT 0 CHECK (verified IN (0,1)),
  PRIMARY KEY (session_id, volume_id)
) STRICT, WITHOUT ROWID;

CREATE TABLE import_items (
  session_id      BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  source_rel_path TEXT NOT NULL,
  dest_volume_id  BLOB NOT NULL REFERENCES volumes(id),
  file_id         BLOB REFERENCES files(id) ON DELETE SET NULL,
  blake3          BLOB CHECK (blake3 IS NULL OR length(blake3) = 32),
  status          TEXT NOT NULL CHECK (status IN ('pending','copied','verified','failed','skipped')),
  error           TEXT,
  PRIMARY KEY (session_id, source_rel_path, dest_volume_id)
) STRICT, WITHOUT ROWID;

-- ── Full-text search (rowid = assets.seq; maintained by catalog::fts) ────
CREATE VIRTUAL TABLE asset_fts USING fts5(
  names,
  comment,
  tags,
  tokenize = 'unicode61 remove_diacritics 2',
  prefix   = '2 3'
);

-- ── Derived view ─────────────────────────────────────────────────────────
CREATE VIEW event_spans AS
  SELECT event_id,
         min(capture_ts_ms) AS start_ms,
         max(capture_ts_ms) AS end_ms,
         count(*)           AS n
  FROM assets
  WHERE event_id IS NOT NULL
  GROUP BY event_id;
```

**Consequences**:
- A backup copy and the card copy of the same photo can both be originals of one asset, each with its own sidecar.
- The `seq INTEGER PRIMARY KEY` on `assets` keeps FTS rowids stable across `VACUUM`.
- Renditions and import history are in the schema but are not rebuilt from sidecars (ADR-012).

---

## ADR-014 — Probing: ExifTool stay-open worker, ffprobe, Sony XML, AVCHD

**Status**: Accepted (Prompt 5, plan approved by the product owner 2026-10-23)

**Decision**:
- **ExifTool** runs as one `-stay_open True -@ -` process owned by a worker thread (`probe::ExifTool`). Requests from any thread queue up and get their own reply (`-execute{N}` / `{readyN}`). Each request has a timeout; on timeout or crash the process is killed and the next request starts a new one (`restarts()` counts this). Common arguments: `-charset filename=utf8 -api largefilesupport=1`. `-n` is not used globally (it would turn `SubSecTimeOriginal = "05"` into `5`); numeric tags are requested with `#`. GPS uses the signed `Composite:` tags.
- **ffprobe** runs once per file with a timeout. Bit depth and chroma come from `pix_fmt`; rotation from the display matrix (reported clockwise); timecode from the `tmcd` track or format tags.
- **Sony XML** (`NonRealTimeMeta`) is parsed with the existing XML tree, matching by local name because the namespace carries a schema version. `Duration` counts frames at `formatFps` (S&Q clips record faster than they play). LTC is decoded from Sony's packed BCD (`FF SS MM HH`, drop-frame flag in bit 6 of the frames byte). GPS comes from the `ExifGPS` item group; this layout follows ExifTool's documentation and has not yet been checked against a real GPS-tagged clip (none in the corpus).
- **AVCHD**: playlists (`MPLS`) and clip info (`HDMV`) are parsed by hand. A PlayItem with connection condition 5 or 6 continues the previous one, so seamless runs are one recording. The capture date comes from ExifTool (MDPM, with its time zone); a built-in MDPM scanner is the fallback. **No real AVCHD card is in the corpus yet**: these parsers are tested with synthesized bytes only.
- **Tool location**: `ToolPaths::discover()` (env `KEEPSAKE_EXIFTOOL` / `KEEPSAKE_FFPROBE` / `KEEPSAKE_VENDOR`, else the nearest `vendor/`); the Tauri app will pass its bundled paths.
- **CI** downloads the real binaries with `scripts/setup.ps1 -VendorOnly` (checksums from ADR-005), cached by the script's hash. Tool tests fail on CI if the tools are missing, and skip locally.
- **No new dependencies.**

**Capture time** (`probe::capture`), in the order ARCHITECTURE.md documents, with a confidence:

| Situation | Confidence |
|---|---|
| Source gives local time and offset; no other source disagrees by > 2 s | high |
| Offset borrowed from the nearest asset of the same camera (±12 h), or derived from a local clock plus a UTC container time, or a high result contradicted by another source | medium |
| Offset unknown | low |

With an unknown offset, `utc_offset_min` is `None` and `utc_ms` holds the best clock reading (the camera's local clock for EXIF/MDPM, UTC for container times). The plan's table said "MDPM without a zone: medium"; it is low unless an offset can be borrowed or derived, the same rule as every other source.

**Grouping** (`grouping`): XAVC clip + `M01.XML` + `SUB/S01` proxy + `THMBNL/T01.JPG` thumbnail (role `sidecar`); stills by stem (JPG, then HEIF, then RAW as original); AVCHD recordings from playlists (first segment `original`, the rest `segment`); card bookkeeping (`AVF_INFO`, `PRIVATE/*` outside the media folders, `BDMV` indexes and playlists, `SONYCARD.IND`) belongs to no asset.

**Sequences** (`grouping::sequences`) are flagged, not merged: Sony `ReleaseMode2` (burst 1, 5, 8, 17, 19, 26, 27, 29; bracket 2, 3, 23) with `SequenceNumber` restarts; otherwise gaps under 1 s make a burst, and three or more frames at least 1 s apart and within 10 % of their median gap make an interval.

**Rebuild** probes assets that have no Keepsake sidecar. Keepsake sidecar values win over probed values (they carry user corrections); a Lightroom/digiKam date is used only when probing finds none (source `sidecar`). Probed values live in the catalog until an edit writes them into the sidecar, so a rebuild without tools (`--no-probe`) cannot restore them for assets nobody has edited.

---

## ADR-015 — Catalog schema v2 (migration 0002)

**Status**: Accepted (Prompt 5)

**Changes from the reviewed plan**:

| # | Reviewed | Final | Why |
|---|---|---|---|
| 1 | Only new columns and tables | Also rebuilds `files` to add role `segment` and a `segment_index` column | Split AVCHD recordings have several `.MTS` files per asset, and the schema allows one `original` per asset per volume. |
| 2 | Migrations run inside one transaction | Migrations that rebuild a table run with foreign keys off, then `foreign_key_check`, then on again | SQLite's documented procedure for changing a table's constraints. Tested: a v1 catalog with an import item pointing at a file upgrades and keeps the link. |

No other changes. Sequence ids are derived from the volume and the first member's original, so rebuilds are stable. The XMP sidecar now also carries `keepsake:CaptureSource`, `keepsake:CaptureConfidence`, `keepsake:Sequence`, segment positions in `keepsake:Files`, and GPS (`exif:GPSLatitude`/`GPSLongitude`/`GPSAltitude` for other apps, plus exact `keepsake:Latitude`/`Longitude`/`Altitude`).

### Schema

```sql
-- Keepsake catalog schema v2: probing results.
--
-- Runs with foreign keys switched off (see migrations.rs) because `files` is
-- rebuilt to widen its role CHECK; the runner verifies foreign keys afterwards.

-- ── Capture provenance and GPS on assets ─────────────────────────────────
ALTER TABLE assets ADD COLUMN capture_source TEXT
  CHECK (capture_source IN ('sony_xml','exif','avchd_mdpm','container','sidecar'));
ALTER TABLE assets ADD COLUMN capture_confidence TEXT
  CHECK (capture_confidence IN ('high','medium','low'));
ALTER TABLE assets ADD COLUMN gps_lat   REAL CHECK (gps_lat BETWEEN -90 AND 90);
ALTER TABLE assets ADD COLUMN gps_lon   REAL CHECK (gps_lon BETWEEN -180 AND 180);
ALTER TABLE assets ADD COLUMN gps_alt_m REAL;

-- ── files: add the 'segment' role (AVCHD split recordings) ───────────────
CREATE TABLE files_v2 (
  id            BLOB PRIMARY KEY CHECK (length(id) = 16),
  asset_id      BLOB NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
  volume_id     BLOB NOT NULL REFERENCES volumes(id) ON DELETE RESTRICT,
  rel_path      TEXT NOT NULL,
  rel_path_key  TEXT NOT NULL,
  file_name     TEXT NOT NULL,
  role          TEXT NOT NULL CHECK (role IN
                  ('original','raw_pair','sidecar','camera_proxy','app_proxy','export','segment')),
  size_bytes    INTEGER NOT NULL CHECK (size_bytes >= 0),
  blake3        BLOB NOT NULL CHECK (length(blake3) = 32),
  xxh3          INTEGER,
  mtime_ms      INTEGER,
  -- Position in the recording: 0 for the original, 1.. for segments.
  segment_index INTEGER CHECK (segment_index >= 0),
  UNIQUE (volume_id, rel_path_key),
  CHECK ((role = 'segment') = (segment_index IS NOT NULL AND segment_index > 0))
) STRICT;
INSERT INTO files_v2 (id, asset_id, volume_id, rel_path, rel_path_key, file_name, role,
                      size_bytes, blake3, xxh3, mtime_ms, segment_index)
  SELECT id, asset_id, volume_id, rel_path, rel_path_key, file_name, role,
         size_bytes, blake3, xxh3, mtime_ms, NULL
  FROM files;
DROP TABLE files;
ALTER TABLE files_v2 RENAME TO files;
CREATE INDEX files_asset  ON files(asset_id);
CREATE INDEX files_blake3 ON files(blake3);
-- One original per asset per volume (backup destinations hold their own copy).
CREATE UNIQUE INDEX files_one_original ON files(asset_id, volume_id) WHERE role = 'original';
CREATE UNIQUE INDEX files_segment_order ON files(asset_id, volume_id, segment_index)
  WHERE segment_index IS NOT NULL;

-- ── Bursts, brackets and interval runs ───────────────────────────────────
CREATE TABLE sequences (
  id        BLOB PRIMARY KEY CHECK (length(id) = 16),
  volume_id BLOB NOT NULL REFERENCES volumes(id) ON DELETE CASCADE,
  kind      TEXT NOT NULL CHECK (kind IN ('burst','interval','bracket'))
) STRICT;

CREATE TABLE asset_sequences (
  asset_id    BLOB PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
  sequence_id BLOB NOT NULL REFERENCES sequences(id) ON DELETE CASCADE,
  position    INTEGER NOT NULL CHECK (position >= 0),
  UNIQUE (sequence_id, position)
) STRICT, WITHOUT ROWID;
CREATE INDEX asset_sequences_seq ON asset_sequences(sequence_id);
```

---

## ADR-016 — `twox-hash` for the quick-diff hash

**Status**: Accepted (Prompt 6, approved by the product owner 2026-10-23)

**Decision**: `twox-hash` 2 (features `xxhash3_64`, `std`), **MIT**, computes xxh3 over a file's length plus its first and last MiB. It is only a quick filter: a match is confirmed with full BLAKE3 before it counts. `xxhash-rust` (Boost licence) was the alternative.

---

## ADR-017 — Ingest engine

**Status**: Accepted (Prompt 6)

**Decision**:
- **State machine per asset**, stored in `import_assets`: `discovered → selected → copying → copied → verified → catalogued`, plus `failed` and `skipped`. Per file × destination, `import_items` goes `pending → copying → copied → verified` (or `reused`, `failed`). `ingest::run` resumes any session.
- **Copy**: the source is read once (BLAKE3 inline) and fanned out to one writer thread per destination. Each writer creates `.<name>.keepsake-partial-<session>` with `create_new` and flushes it (`sync_all`). Then:
  1. The temporary file is re-read past the OS cache (Windows `FILE_FLAG_NO_BUFFERING`; macOS goes through the cache for now) and compared.
  2. The stream hash is recorded.
  3. The file is renamed to its final name, refusing to replace anything, with up to 5 retries when the file is busy.
  4. A separate pass re-reads every final file before the item becomes `verified`.

  A final file that fails that pass is moved to a hidden `.<name>.keepsake-bad-<session>` name, so a bad copy is never left under its final name.
- **Resume**:
  - Temporary files are deleted and those files are copied again from the start.
  - A final file found with a recorded stream hash (crash after the rename) is re-hashed and kept only if it matches; otherwise it's moved aside and copied again.
  - A final file without a recorded hash is reused only if it equals the source.
  - Cataloguing is simply redone (it is idempotent).
- **Source**: only reachable through `SourceVolume`, which can open files for reading and nothing else. The card is registered as a read-only volume, so no sidecar is ever written there.
- **Destinations** are refused when they are inside the source, overlap each other, are on the same file system as the source, or are both on one file system (`GetVolumePathNameW` / `st_dev`). A testing-only flag (`allow_same_filesystem`, CLI `--unsafe-allow-same-disk`, hidden) relaxes this; "safe to format" then counts volumes, not disks. Free space is checked before copying: what's left plus 5 % + 16 MiB (`GetDiskFreeSpaceExW` / `df -Pk`). These two Windows calls are declared directly, so no crate is needed.
- **Quick-diff**: an asset is `already_imported` when its original's size + quick hash match a library file off the card. It is copied only with `--all`. It counts toward safe-to-format only after a full BLAKE3 of every one of its card files matches library files on **two** different volumes. `modified` means an earlier session copied the same card path with a different quick hash. Rebuild now records the quick hash too.
- **Names**: the template engine (`ingest::template`) works as follows:
  - Tokens: `{yyyy} {mm} {dd} {hh} {event} {camera} {lens} {seq} {orig}`; the default is `{yyyy}/{yyyy}-{mm}-{dd}/{orig}`.
  - Missing values: an undated asset goes under `0000-undated`, and missing text becomes `unknown`.
  - Sanitising: names are made safe for Windows and macOS (invalid characters, reserved names, trailing dots/spaces, 200-byte components, NFC).
  - `{seq}` counts per rendered folder, in capture-time order then source path.
  - Companions keep the original's new base name plus their Sony suffix (`M01`, `S01`, `T01`), siblings share it, and AVCHD segments get `_02`, `_03`, ….
  - Collisions are decided while planning, using the destination's case and Unicode rules. An identical existing file is reused; a different one makes the whole asset take ` (2)`, ` (3)` ….
- **Safe to format**, as the prompt defines it: every *selected* asset is catalogued, and each of its files is verified (or reused) on two different destination volumes. Already-imported assets need the BLAKE3 confirmation above. Assets that were not selected are reported prominently ("formatting the card loses them") but don't block the flag, per the prompt's wording.
- **Fault injection**: `ingest::fault::FaultPlan` is passed in explicitly; there are no environment variables in production code. Faults cover source read errors, the source changing mid-read, write errors, corrupted writes, corrupted re-reads, corrupted finals, rename errors and catalog failures. Crashes at three points can either be simulated (return without cleanup) or real (`abort()`); the real-kill test re-runs the test binary as a child process.
- **Read-only proof (Windows)**:
  - The test applies an ACL denying Everyone `WD,AD,WA,WEA,DC` on the tree, plus the specific `DE` right on files.
  - icacls's generic `W` and `D` include SYNCHRONIZE and block reading as well, so they can't be used.
  - The read-only file attribute doesn't help either: Rust's `remove_file` deletes read-only files on Windows.
  - On Unix the test uses `chmod -R a-w`. macOS should use a read-only DMG once macOS CI returns.
- **Known limit**: the final rename checks that the target doesn't exist, then renames (`MoveFileEx` replaces existing files). Another process creating the same name in that instant could be overwritten. Destinations are folders Keepsake owns, and a no-replace rename is a follow-up.

### Changes from the reviewed plan
| # | Reviewed | Final | Why |
|---|---|---|---|
| 1 | `import_items` without segment information | Adds `segment_index` | Cataloguing after a resume must restore AVCHD segment order without the card. |
| 2 | — | `UNIQUE (dest_volume_id, dest_rel_path, session_id)` on `import_items` | A guard against two files of one session being planned onto one name. |
| 3 | Re-read, then rename | Re-read the temporary file, rename, **and** re-read the final file | Covers both points: the temporary copy before it gets its final name, and the final file as it is on disk. |

### Schema (migration 0003)

```sql
-- Keepsake catalog schema v3: resumable ingest.
--
-- Runs with foreign keys switched off (see migrations.rs) because
-- `import_items` is rebuilt; the runner verifies foreign keys afterwards.

-- ── Session settings ─────────────────────────────────────────────────────
ALTER TABLE import_sessions ADD COLUMN template TEXT;
ALTER TABLE import_sessions ADD COLUMN safe_to_format INTEGER NOT NULL DEFAULT 0
  CHECK (safe_to_format IN (0,1));

-- ── One row per asset of a session: the ingest state machine ────────────
CREATE TABLE import_assets (
  session_id  BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  source_key  TEXT NOT NULL,              -- rel_path_key of the source original
  asset_id    BLOB NOT NULL CHECK (length(asset_id) = 16),
  diff        TEXT NOT NULL CHECK (diff IN ('new','already_imported','modified')),
  state       TEXT NOT NULL CHECK (state IN ('discovered','selected','copying','copied',
                                             'verified','catalogued','failed','skipped')),
  error       TEXT,
  attempts    INTEGER NOT NULL DEFAULT 0,
  updated_ms  INTEGER NOT NULL,
  PRIMARY KEY (session_id, source_key)
) STRICT, WITHOUT ROWID;

-- Older rows (none are written by released code) become one asset each.
INSERT INTO import_assets (session_id, source_key, asset_id, diff, state, updated_ms)
  SELECT DISTINCT session_id, lower(source_rel_path), zeroblob(16), 'new',
         CASE WHEN status = 'verified' THEN 'verified' ELSE 'failed' END, 0
  FROM import_items;

-- ── import_items rebuilt: wider status list and copy bookkeeping ────────
CREATE TABLE import_items_v2 (
  session_id      BLOB NOT NULL REFERENCES import_sessions(id) ON DELETE CASCADE,
  source_rel_path TEXT NOT NULL,
  dest_volume_id  BLOB NOT NULL REFERENCES volumes(id),
  source_key      TEXT NOT NULL,
  role            TEXT NOT NULL CHECK (role IN
                    ('original','raw_pair','sidecar','camera_proxy','app_proxy','export','segment')),
  segment_index   INTEGER CHECK (segment_index > 0),
  size_bytes      INTEGER NOT NULL CHECK (size_bytes >= 0),
  quick_hash      INTEGER NOT NULL,
  dest_rel_path   TEXT NOT NULL,
  file_id         BLOB REFERENCES files(id) ON DELETE SET NULL,
  blake3          BLOB CHECK (blake3 IS NULL OR length(blake3) = 32),
  verified_blake3 BLOB CHECK (verified_blake3 IS NULL OR length(verified_blake3) = 32),
  status          TEXT NOT NULL CHECK (status IN
                    ('pending','copying','copied','verified','failed','skipped','reused')),
  error           TEXT,
  PRIMARY KEY (session_id, source_rel_path, dest_volume_id),
  FOREIGN KEY (session_id, source_key) REFERENCES import_assets(session_id, source_key)
    ON DELETE CASCADE,
  UNIQUE (dest_volume_id, dest_rel_path, session_id)
) STRICT, WITHOUT ROWID;
INSERT INTO import_items_v2 (session_id, source_rel_path, dest_volume_id, source_key, role,
                             size_bytes, quick_hash, dest_rel_path, file_id, blake3,
                             verified_blake3, status, error)
  SELECT session_id, source_rel_path, dest_volume_id, lower(source_rel_path), 'original',
         0, 0, source_rel_path, file_id, blake3,
         CASE WHEN status = 'verified' THEN blake3 END, status, error
  FROM import_items;
DROP TABLE import_items;
ALTER TABLE import_items_v2 RENAME TO import_items;
CREATE INDEX import_items_asset ON import_items(session_id, source_key);
```

---

## ADR-018 — `indicatif` and `ctrlc` for the CLI

**Status**: Accepted (Prompt 7, approved by the product owner 2026-10-24)

| Crate | Version | Licence | Scope | Reason |
|---|---|---|---|---|
| `indicatif` | 0.18 | MIT | `keepsake-cli` | Progress bars (stderr, terminals only, never with `--json`) |
| `ctrlc` | 3 | MIT / Apache-2.0 | `keepsake-cli` | Ctrl-C on macOS and Ctrl-C / Ctrl-Break / console close on Windows |

Neither is linked into `keepsake-core`. The CLI also uses the existing workspace crates `serde` (config) and `chrono`.

---

## ADR-019 — CLI behaviour

**Status**: Accepted (Prompt 7)

- **Commands**: `init`, `ingest` (`--dry-run`, `--resume`, `--status`, `--list`), `verify`, `search`, `shift-time`, `backup`, `rebuild-catalog`, `probe` (`corpus` is hidden, for developers). The earlier `library …` group and the `ingest start|resume|status|list` subcommands are gone; this supersedes the `library add-volume` mention in ADR-012.
- **Library selection**: `--library`, then `KEEPSAKE_LIBRARY`, then `default_library` in `config.toml` (written by `init`; folder overridable with `KEEPSAKE_CONFIG_DIR`).
- **Output**: plain text with the verdict first; `--json` prints one document on stdout, errors as `{"error": …}` on stderr.
- **Exit status**:

  | Code | Meaning |
  |---|---|
  | 0 | OK |
  | 1 | Error |
  | 2 | Finished with problems: verify found issues, ingest not safe to format, backup failures or unavailable assets, rebuild problems |
  | 130 | Cancelled |
- **Ctrl-C** finishes the current file and stops (exit 130); a second press exits immediately. This changes ingest, which previously aborted mid-file on cancel. The copy engine no longer looks at the cancel flag inside a file; callers check between files.
- **Ingest dry run** uses the same planning code as a real import, but writes nothing. Destinations are described without being registered or created (`ingest::preview_destination`), and the same-disk rules still apply.
- **Cards are not library storage**: `rebuild-catalog` skips volumes of kind `card`, and `verify` lists only volumes that hold catalogued files. This fixes a bug the new CLI exposed: a full rebuild had catalogued the ingest source card.
- **Backup**:
  - Every asset is copied from the mounted volume holding most of its files, keeping relative paths, through the ingest copy path (temporary file, fsync, cache-bypassing read-back, rename without replacing, final re-read). A collision gets ` (2)`.
  - A copy is made only if the source's BLAKE3 equals the catalog's, so damage is never propagated.
  - A file is skipped when the backup already holds a catalogued copy with the same role, content and name.
  - Leftover backup temporaries (`.keepsake-partial-backup`) are removed at the start of the next run.
  - The library goes to `<dest>/.keepsake-library/`: a `VACUUM INTO` catalog snapshot, the registry, recipes and mirrored sidecars, swapped in atomically.
  - The backup folder may not overlap a library volume or the library folder.
- **`shift-time`** selection is paths (files, or folders meaning "everything inside") and/or the search filters; an empty selection or an unknown path is an error. The shift is written to the sidecars of every copy and to the catalog; originals are untouched.
- **Ctrl-C test on Windows** starts the CLI in its own process group and sends `CTRL_BREAK_EVENT` with `GenerateConsoleCtrlEvent` (test code only). If the console cannot deliver it, the test says so and skips; the cancel rule is also covered by core ingest tests.

---

## ADR-020 — Device detection and card memory

**Status**: Accepted (Prompt 8, approved by the product owner 2026-10-24). Supersedes the `WM_DEVICECHANGE` / IOKit wording in ADR-002.

**OS bindings: none added.**
- Windows calls `cfgmgr32` and `kernel32` directly: `CM_Register_Notification`, `GetVolumeInformationW`, `GetVolumeNameForVolumeMountPointW`, `GetDiskFreeSpaceExW`, `DeviceIoControl(IOCTL_STORAGE_QUERY_PROPERTY)`.
- macOS calls the DiskArbitration and CoreFoundation frameworks directly.
- The declarations are hand-written in `keepsake-platform/src/{windows,macos}.rs`, as `keepsake-core` already does for disk space and cache-bypassing reads. Every `unsafe` block has a `SAFETY` comment.
- `CM_NOTIFY_FILTER`'s size is checked by a test.

**Notifications wake a scanner; they are not trusted for details.**
- One thread lists the mounted volumes (drive letters or `/Volumes`) and diffs the result against the last listing.
- Notifications make it list every 0.5 s for 6 s, which covers the delay before a drive letter or mount point appears. Otherwise it lists every 3 s.
- So a missed notification, a card swapped in a reader, or a letter change is still seen. A swap at the same mount appears as a removal followed by an arrival.
- Volumes mounted in folders instead of drive letters (Windows) are not watched.

**Card identity.**
- `card:<fs type>:<fs serial>:<capacity>`. The serial is the FAT/exFAT volume ID on Windows, and the volume UUID on macOS (derived from that serial).
- The hardware serial is recorded but never used to identify a card, because a card reader reports its own serial for every card put in it.
- Consequences:
  - A card reformatted in the camera is a new card. It has nothing old to import.
  - The same card has different keys on Windows and macOS, so a library moved between them meets its cards as new.
  - Without a serial, a card is remembered by its mount path.
- Ingest registers the card's source volume by this key (`PlanOptions::source_id`), so E: today and F: tomorrow are one volume.

**Sony card** means any of `PRIVATE/M4ROOT`, `PRIVATE/AVCHD`, `MP_ROOT`, or `DCIM/nnnMSDCF`, ignoring case. Other volumes are reported as ignored.

**New since last import.**
- After every import or resume of a remembered card, `cards/<id>.json` records the originals now in the library (catalogued, or found already imported) with their path, size and modified time.
- An item is new if it is not in that list or its size or time changed. Counting is a directory listing, with no hashing.
- Failed or unselected items stay new.

**Card memory and presets are files in the library folder** (`cards.json`, `cards/`, `presets.json`):
- written atomically;
- copied into backups' `.keepsake-library`;
- untouched by `rebuild-catalog --fresh`, because they are user state that cannot be rebuilt from the media.

**Automatic import** happens only when all of these hold:
- `keepsake watch --auto-ingest` is running;
- the card is trusted and has an existing preset;
- the card has new items;
- every preset destination folder exists.

Otherwise the watcher emits `auto_ingest_skipped` with the reason. When an import does run:
- It uses the normal ingest path: new items only, verified copies, the same-disk rules, and the card is never written.
- Imports run one at a time; device events wait meanwhile.
- Ctrl-C finishes the current file. `ingest --resume` continues.
- A card pulled mid-import leaves failed assets, which are still counted as new.

**`watch --json`** prints JSON Lines: one event per line, flushed. It starts with `watching` and ends with `stopped`.

**CI.** A `macos-14` job runs clippy and the tests for the whole workspace except the Tauri app. With no LGPL FFmpeg for macOS yet (ADR-005), it sets `KEEPSAKE_SKIP_MEDIA_TOOLS`, so tests that need ExifTool, ffprobe or ffmpeg skip there instead of failing.

---

## ADR-021 — Renditions and the job queue

**Status**: Accepted (Prompt 9, approved by the product owner 2026-10-24)

**Dependencies**

| Crate | Version | Licence | Scope | Reason |
|---|---|---|---|---|
| `image` | 0.25 (features `jpeg`, `png`, `tiff` only; no rayon) | MIT / Apache-2.0 | `keepsake-core` | Decode JPEG/PNG/TIFF with EXIF orientation; encode JPEG |
| `fast_image_resize` | 6 (feature `image`) | MIT / Apache-2.0 | `keepsake-core` | SIMD Lanczos3 resizing, several times faster than `image`'s |

Their own dependencies are all permissive: `zune-jpeg`/`zune-core` (MIT/Apache/Zlib), `png`, `tiff`, `fdeflate`, `weezl`, `bytemuck` (MIT/Apache). `tempfile` (already approved) is now also used by the CLI binary, for `renders bench`.

**Minimum Rust raised from 1.77 to 1.90.** The old value was already untrue: `uuid` needs 1.89 and Tauri 1.90. Both new crates need 1.88.

**HEIF goes through the FFmpeg sidecar, not libheif.** This needs no native LGPL libraries linked or shipped, and no vcpkg or Homebrew builds; FFmpeg is already shipped as an LGPL sidecar (ADR-005). It is untested on real `.HIF` files because the corpus has none, and our LGPL FFmpeg cannot write HEIF test files.

**Stills**
- One job (`render.stills`) decodes the source once and makes the 2560 px preview, then the 1024 px thumbnail from the preview, then the 256 px thumbnail from that. Images are never enlarged. Output is JPEG q82 with orientation baked in.
- For a RAW + JPEG pair, the JPEG is decoded.
- RAW-only assets use the largest embedded JPEG (`PreviewImage`, `JpgFromRaw`, `OtherImage`). ExifTool stay-open gives the offset and length, and the bytes are read directly. Sony ARWs carry a 1616×1080 preview, so their previews are 1616 px until a LibRaw decode is added (CLAUDE.md stack; not part of this prompt).
- Videos use the frame at 10 % of the duration, with FFmpeg's display rotation applied and AVCHD segments joined.

**Proxies**
- The spike's settings (ADR-009): `scale=-2:720` (never enlarged), AAC 128 kb/s (stereo), `+faststart`.
- Plus `-g <frame rate>` on every encoder, so the GOP is one second by construction rather than by encoder default. Tests check it with ffprobe.
- Interlaced sources are deinterlaced with `yadif`.
- Encoder order: NVENC (`-preset p4 -b:v 4M`), QSV (`-global_quality 25`), AMF (`-quality balanced -b:v 4M`), Media Foundation (`-q:v 20`), then OpenH264 (`-b:v 4M`), which is always in the LGPL build. VideoToolbox (`-b:v 4M`) is used on macOS.
- The first encoder that passes a 10-frame test encode is used for the rest of the process. `--encoder` or `forced_encoder` overrides it.
- A camera SUB proxy is copied as-is **only if it is H.264** (an A7 IV can record HEVC proxies). The webview never gets HEVC.

**Cache**
- `<cache>/<kind>/<aa>/<source>-<recipe>.<ext>`. The source is the original's BLAKE3 from the catalog, so lookups work offline and identical files share renditions. The recipe is the BLAKE3 of canonical JSON with a version.
- Files are written to `.tmp` and renamed. A row whose file vanished is dropped on lookup, and `set_visible` queues it again.
- Eviction is least recently used first. Last-use times are written at most every 10 minutes per rendition.
- A rendition is evictable only if one of its originals is on a mounted volume. Renditions of offline media stay even above the limit, and `renders status` shows them.

**Queue** (catalog schema v4, `jobs` table, generic over `kind`)
- Classes: visible (0), recent (1), backlog (2). Visible is temporary: `set_visible` replaces the set, and jobs fall back to their class.
- Within a class: rank (stills before proxies), then newest capture.
- Retries back off 30 s, then 60 s, then the job fails for good. `Unsupported` (an unrenderable format) fails at once.
- Offline media is deferred 10 minutes without counting an attempt. Running jobs are recovered on start.

**Pauses**
- On battery: Windows `GetSystemPowerStatus`, macOS `IOPSGetProvidingPowerSourceType`, hand-written bindings in `keepsake-platform::power`.
- During an ingest: `ingest::run` keeps `ingest_heartbeat_ms` fresh in `library_meta` every 2 s from its own connection. Workers wait while it is under 10 s old. This works across processes and cannot get stuck after a crash, unlike a lock file.
- Workers finish their current job before pausing or stopping.

**Settings**: `config.toml` moves from the CLI into `keepsake_core::settings`, shared with the app. The `[render]` section has `cache_dir`, `max_bytes` (default 20 GiB), `workers` (default half the threads, 1–4), `proxy_workers` (1) and `pause_on_battery` (true).

**App**: thin Tauri commands `render_status`, `render_set_visible`, `render_lookup`, `render_start` and `render_stop`. `render_start` runs the service on its own thread and emits `render://progress`.

---

## ADR-022 — The main app UI

**Status**: Accepted (Prompt 10, approved by the product owner 2026-10-25)

**Dependencies**

| Dependency | Version | Licence | Scope | Reason |
|---|---|---|---|---|
| `ts-rs` | 12 | MIT | `keepsake-app` | TypeScript types generated from the Rust command types |
| `tauri-plugin-dialog` + `@tauri-apps/plugin-dialog` | 2 | MIT / Apache-2.0 | app | Native folder picker (library, destinations) |
| `@playwright/test` | 1.x | Apache-2.0 | UI dev | UI tests, in the browser and against the real app |
| `vitest` | 3.2 | MIT | UI dev | Unit tests. 3.x because Vitest 4+ needs Vite 6.4+, and the UI is on Vite 5 |

The dialog plugin's 3.0 alphas target future Tauri; we pin 2.x.

**Typed bridge.**
- Every command and event type is a Rust type in `src-tauri/src/api.rs`, with `#[derive(TS)]`.
- `cargo test -p keepsake-app` writes `app/ui/src/bindings/*.ts`. These files are checked in, and CI fails if they change.
- The UI calls the app only through `src/api.ts`.
- Commands stay thin: they validate, call core and convert the result. The work is in core.

**Views are snapshots.**
- `view_open(query, sort)` runs one sorted id scan in core (`Browse::ordered_ids`), keeps the ids in app state and returns `{view, count}`. At 300,000 items this takes 0.16–0.27 s in release.
- `view_rows(view, offset, limit)` looks up the ids it needs, so any page, however deep, takes 4–6 ms.
- Selections are index ranges into the snapshot: select-all is `[[0, 300000]]`, not 300,000 ids. Edits resolve the ranges in the app.
- `LIMIT/OFFSET` paging was rejected: missing-values-last ordering cannot use an index, so every deep page re-sorted the whole library (0.4–0.55 s).

**Schema v5** adds to `assets`:
- `primary_name`, `size_bytes` and `is_raw`, kept current by triggers on `files`, so name, size and type sort from an index;
- indexes for every sort and filter.

`SearchQuery` gains lens, minimum rating, RAW, label, event, volume, import session and undated. The calendar uses date ranges rather than `LIKE`, so it can use the date index.

**Connections.**
- Reads share one connection.
- Commands that write (edits, "these are on screen", status polling) open their own. With WAL, reads then never wait behind a slow write. Before this, the status bar's poll blocked grid scrolling for seconds at 300,000 items.
- Core queues the render backlog 2,000 jobs per transaction.
- Cache stats are two indexed SQL queries rather than a scan of every file.

**Edits.**
- Rating, label and keywords apply to the selection; the comment applies to one item. All go through `MetadataService`, which writes the XMP.
- Selections over 50 items run on a background thread and report `edit://progress`.

**Import window.**
- The app runs the device watcher (prompt 8). A Sony card with new items opens or focuses an `import` window.
- `ingest::scan_card` lists the card from the card memory first and quick-hashes only unknown items.
- The pre-check (`check_destinations_for`, plus an example name) writes nothing.
- The import runs on a thread and reports `import://progress` and `import://done`.
- Thumbnails of items still on the card go to `<cache>/card/`, outside the catalog, and are swept after a week. They are never written to the card.

**Security.** The asset protocol's static scope is empty. At runtime the app allows only the rendition cache, so the webview cannot load arbitrary files.

**Dev builds** optimise dependencies (`[profile.dev.package."*"] opt-level = 2`), so the debug app and UI tests are usable on large libraries. Keepsake's own crates stay unoptimised.

**Tests.**
- **Mocked:** Playwright drives the UI in Chromium against `src/mock/backend.ts` (Tauri's `mockIPC`, 300,000 synthetic items). This runs on Windows and macOS; WKWebView cannot be automated, so it is the macOS UI coverage.
- **Real app:** on Windows, Playwright attaches to the app's WebView2 over its debugging port, using a temporary library, real media, fake device events (`KEEPSAKE_FAKE_EVENTS`) and `KEEPSAKE_UI_TEST=1`. That setting allows test destinations on one disk; nothing in the UI can set it.
- **Seeding:** a hidden `keepsake dev seed` fills an empty library with synthetic assets that share 64 cached pictures.

**The spike page is gone.** Its results are in `docs/spikes/playback.md`.
