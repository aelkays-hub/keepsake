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

**Decision**: Video playback uses the camera-generated proxy file (`PRIVATE/M4ROOT/SUB/*.MP4`) when available. If no proxy exists (e.g. AVCHD or older MP4 cards), the RenderService generates an H.264 1080p proxy via FFmpeg and caches it. The webview plays the proxy via `<video>` with a `blob:` URL.

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
