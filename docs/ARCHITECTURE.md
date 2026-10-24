# Keepsake — Architecture

## High-Level Component Map

```
┌─────────────────────────────────────────────────────────┐
│                   keepsake-core                         │
│                                                         │
│  DeviceWatcher → IngestEngine → Catalog                 │
│                      ↓              ↑                   │
│                   Probe        RenderService            │
│                      ↓              ↓                   │
│                  VideoOps     ShareConnectors           │
│                      ↓                                  │
│                   AIWorker                              │
└─────────────────────────────────────────────────────────┘
         ↑                  ↑                ↑
  Tauri app (GUI)    keepsake-cli    background watcher
  app/src-tauri     keepsake-cli    (`keepsake watch`)
```

All three executables call into `keepsake-core` through the same public API. There is no business logic outside of `keepsake-core` and `keepsake-platform`.

The CLI (`keepsake-cli`, manual in `docs/CLI.md`) parses arguments, picks the library (`--library`, else `KEEPSAKE_LIBRARY`, else the default in `config.toml`), prints text or `--json`, draws progress bars and turns Ctrl-C into a "finish the current file, then stop" flag. The work itself is done in core:
- `ingest::{preview, plan, run, report}` for `ingest`;
- `verify::verify` for `verify`;
- `catalog::Search::query` (`SearchQuery`) for `search`;
- `select::resolve` and `MetadataService::shift_capture_time` for `shift-time`;
- `backup::run` for `backup`;
- `rebuild::rebuild_library` for `rebuild-catalog`;
- `watch::Watcher`, with `keepsake_platform::system_devices()`, for `watch`;
- `cards` and `presets` for `card` and `preset`.

---

## Components

### DeviceWatcher
Split in two (ADR-020):

**`keepsake-platform`: device events.** `DeviceEvents` sends `DeviceEvent::Arrived { mount, identity }` and `Removed { mount }`, including an arrival for each volume already mounted at start.
- Both OS backends run one scanner thread. It lists mounted volumes and sends the differences.
- OS notifications only wake the thread:
  - on Windows, `CM_Register_Notification` on the volume interface class;
  - on macOS, DiskArbitration appeared, disappeared and description-changed callbacks on the thread's run loop.
- After a notification the thread lists every 0.5 s for 6 s, while the volume mounts. Otherwise it lists every 3 s, as a safety net.
- `VolumeIdentity` comes from:
  - Windows: `GetVolumeInformationW`, the volume GUID, `GetDiskFreeSpaceExW`, `IOCTL_STORAGE_QUERY_PROPERTY`;
  - macOS: the DiskArbitration description.
- `FakeDevices` replays scripted events for tests.

**`keepsake-core`: what the events mean.**
- `cards::recognize` checks for Sony layouts.
- `cards::identify` finds or creates the card's record by `VolumeIdentity::card_key` (file-system type, serial and size) and counts the items not imported from it before.
- `watch::Watcher` turns device events into `WatchEvent`s, and runs an automatic import, one at a time, for trusted cards with a preset.

Card memory (`cards.json`, `cards/<id>.json`) and presets (`presets.json`) live in the library folder, not the catalog: they cannot be rebuilt from media.

```
OS ──notify──▶ scanner thread ──DeviceEvent──▶ Watcher ──▶ cards::identify ──▶ WatchEvent
                 (platform)        (channel)     (core)        │                (CLI prints / app reads)
                                                               └─ trusted + preset ─▶ ingest::plan + run
```

### IngestEngine
`keepsake_core::ingest`: `plan` (group, probe, quick-diff, name every file on every destination, store the session), `select`, `run` (copy, verify, catalogue, confirm, decide "safe to format"; also resumes), `report`. `run` is blocking and takes a progress callback and a cancel flag. It is meant to run on a worker thread. While it runs, a heartbeat in the catalog pauses render workers (in any process), and it queues renditions for what it catalogued as recent imports. The design and failure handling are in ADR-017.

### Catalog
SQLite database in WAL mode with FTS5 full-text index (`keepsake_core::catalog`). Stores Assets, Files, Volumes, Renditions, Events, EditRecipes, ImportSessions, labels, tags and XMP sidecar locations. The catalog is a **rebuildable cache**: it can be discarded and reconstructed from the original files and XMP sidecars. XMP sidecars and JSON recipes are the authoritative source of truth for all user metadata.

- **Schema:** `crates/keepsake-core/src/catalog/migrations/NNNN_*.sql`, applied in order and tracked with `PRAGMA user_version`. A catalog newer than the build is refused.
- **Access:** all SQL lives in `catalog/`. Callers use `Catalog::read` / `Catalog::write` (a `BEGIN IMMEDIATE` transaction that commits on `Ok` and rolls back on `Err`) and the typed repositories on the transaction (`tx.assets()`, `tx.files()`, `tx.tags()`, ...). Write methods exist only on a write transaction.
- **Search:** `asset_fts` (FTS5, `unicode61 remove_diacritics 2`, prefix index) holds file names, comment and tag names including ancestors. It is updated in Rust inside the same transaction as each change; `tx.fts_mismatches()` checks it.
- **User edits:** `metadata::MetadataService` updates the rows and rewrites the asset's sidecars in one transaction (see _XMP Sidecars_).

### Probe
Reads media metadata without decoding pixels (`keepsake_core::probe`):
- `ExifTool`: one long-lived `-stay_open` process behind a request queue, with per-request timeouts and automatic restart.
- `ffprobe`: streams, duration, codec/profile, bit depth, chroma, rotation, timecode, container creation time.
- `sony_xml`: XAVC `NonRealTimeMeta` sidecars (creation date with offset, frames → duration, timecode, model, serial, lens, recording mode, GPS).
- `avchd`: playlists and clip info (split recordings), MDPM capture date.
- `capture`: resolves one capture time per asset with its source and confidence (see _Capture Date Precedence_).
- `Prober::probe(root, files, grouping)` returns a `ProbedAsset` per group and the detected sequences. It is used by rebuild, `keepsake probe` and the corpus tests.

### Grouping
`keepsake_core::grouping` turns a file list into assets (XAVC clip + XML + proxy + thumbnail; RAW/JPEG/HEIF siblings; AVCHD recordings joined from playlists) and lists card bookkeeping files separately. `grouping::sequences` flags bursts, brackets and interval runs without merging them.

### Job queue
`keepsake_core::jobs` and the catalog's `jobs` table (schema v4). The queue is persistent and generic (`kind` + asset); long-running work goes here, never on the UI or a command thread.

Claim order:
1. Priority: **visible** (on screen; `render::set_visible` replaces the visible set), then **recent import**, then **backlog**.
2. The kind's rank.
3. Newest capture first.

Failures retry after 30 s, then 60 s, then stay failed. Jobs whose media is offline are deferred for 10 minutes, without counting an attempt. On start, running jobs go back to queued. Workers pause on battery (`keepsake_platform::on_battery`) and while an ingest heartbeat is fresh.

### RenderService
`keepsake_core::render` (ADR-021).

- **Workers:** `RenderService` runs `workers` threads for `render.stills` (preview + both thumbnails from one decode) and `proxy_workers` threads for `render.proxy`. Each has its own catalog connection. Events (`RenderEvent`) are delivered to the caller: the CLI prints them, and the app forwards them as `render://progress`.
- **Stills:**
  - JPEG, PNG and TIFF decode in-process (`image`, with EXIF orientation), then resize (`fast_image_resize`, Lanczos3) and encode as JPEG q82.
  - ARW and DNG use the largest embedded JPEG, located by ExifTool (stay-open) and read directly from the file.
  - HEIF and video poster frames (10 % in) come from the FFmpeg sidecar as PPM over a pipe.
- **Proxies:**
  - A camera SUB proxy is copied into the cache if it is H.264.
  - Otherwise an FFmpeg encode: 720p, a GOP of exactly one second, AAC stereo 128 kb/s, `+faststart`, AVCHD segments joined and interlaced sources deinterlaced. The encoder is the first that works of NVENC → QSV → AMF → Media Foundation → OpenH264 (VideoToolbox on macOS).
  - HEVC and MTS never reach the webview (ADR-009).
- **Cache:**
  - Files are named `<root>/<kind>/<aa>/<source BLAKE3>-<recipe hash>.<ext>` (content-addressed), written through `.tmp` and a rename, and indexed by the `renditions` table.
  - Identical originals share renditions. A recipe change gives new names.
  - LRU eviction keeps the cache under `max_bytes`, but never removes a rendition whose original is not on a mounted volume.
  - The root is `[render] cache_dir` in `config.toml` (default: the OS cache folder).

### VideoOps
Wraps FFmpeg for all video operations: trim (stream copy or smart-cut re-encode), combine, rotate, frame grab, MP4 conversion, proxy generation, time-lapse assembly, deflicker. Every operation produces a new file; no in-place modification.

### ShareConnectors
Pluggable connectors: Flickr (OAuth 2.0), OpenTimelineIO export. Each connector implements a `Connector` trait in `keepsake-core` with send/cancel/progress.

### AIWorker
On-device inference: auto-tagging, face detection/clustering, semantic embedding. Runs on a low-priority thread pool. Results written to the catalog; never to original files.

---

## Data Directories

All application data is placed in OS-standard directories via `keepsake_core::paths` (backed by the `dirs` crate — see ADR-010). The Tauri app resolves these paths at startup and passes them into `keepsake-core`; the CLI does the same.

| Kind | Windows | macOS |
|---|---|---|
| **Data** — catalog (SQLite), sidecar metadata cache | `%LOCALAPPDATA%\keepsake` | `~/Library/Application Support/keepsake` |
| **Cache** — thumbnails, generated proxies | `%LOCALAPPDATA%\keepsake\cache` | `~/Library/Caches/keepsake` |
| **Config** — user preferences, library list | `%APPDATA%\keepsake` | `~/Library/Application Support/keepsake` |

The data directory contains one subdirectory per library (`library-<uuid>/`), each holding `catalog.db` and a `sidecars/` tree mirroring the volume layout.

The cache directory is fully regenerable; deleting it forces re-generation of thumbnails and proxies on next access.

---

## Data Model

### Asset
The logical memory of a moment. One asset may have multiple physical files (original + raw pair + sidecar + camera proxy).

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` | Stable, randomly generated at ingest |
| `capture_ts` | `DateTime<Utc>` | Resolved capture timestamp (see precedence below) |
| `duration_ms` | `Option<u64>` | For video assets |
| `event_id` | `Option<Uuid>` | FK to Event |
| `rating` | `u8` | 0–5 |
| `label` | `Option<String>` | Colour label |
| `comment` | `Option<String>` | Free text |
| `tags` | `Vec<String>` | AI and user tags |

### File
One physical file on disk. An Asset has 1-N Files.

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` | |
| `asset_id` | `Uuid` | FK to Asset |
| `volume_id` | `Uuid` | FK to Volume |
| `rel_path` | `RelPath` | Relative to volume root, exact on-disk spelling, `/`-separated |
| `rel_path_key` | `String` | NFC, lowercased on case-insensitive volumes; unique per volume |
| `blake3` | `[u8; 32]` | Content identity hash |
| `size_bytes` | `u64` | |
| `role` | `FileRole` | See below |

`FileRole` variants: `Original`, `RawPair`, `Sidecar`, `CameraProxy`, `AppProxy`, `Export`, `Segment` (2nd and later `.MTS` of a split AVCHD recording, with `segment_index` 1, 2, …).

An asset has at most one `Original` **per volume**: a verified backup copy on a second destination is also an original. File ids are derived from `(volume_id, rel_path_key)`, so a rebuilt catalog gives every file the same id.

### Volume
A storage location (card, internal drive, external drive, NAS).

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` | Stable; assigned at first sight |
| `label` | `String` | Human-readable name |
| `fs_id` | `String` | OS filesystem identifier |
| `online` | `bool` | True if currently mounted |
| `mount_path` | `Option<PathBuf>` | Absolute path when online |

### Rendition
A cached derived image or video file.

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` | |
| `source_hash` | `[u8; 32]` | BLAKE3 of the source file |
| `recipe_hash` | `[u8; 32]` | BLAKE3 of the serialised recipe |
| `kind` | `RenditionKind` | `Thumbnail`, `Preview`, `Proxy`, `Export` |
| `rel_path` | `PathBuf` | Relative to rendition cache root |

### Event
A time-and-place cluster of assets (e.g. "Wedding, 2024-06-15, Tokyo").

### EditRecipe
Versioned JSON document describing a non-destructive edit (crop, levels, etc.). Linked to the Asset it belongs to. Applied by RenderService when generating export files.

### ImportSession
Records one complete import run: card volume, timestamp, destination volumes, file list, verification results.

---

## Sony Card Layouts

### Still photos
```
DCIM/
  100MSDCF/   (or 100SONY, etc.)
    DSC00001.JPG
    DSC00001.ARW    (raw)
    DSC00001.HIF    (HEIF)
```

### XAVC S / XAVC HS (current cameras)
```
PRIVATE/M4ROOT/
  CLIP/
    C0001.MP4
    C0001M01.XML    ← Sony XML sidecar (face markers, highlights, chapter marks)
  SUB/
    C0001S01.MP4    ← camera-generated proxy (lower-resolution H.264)
```

### AVCHD (older cameras)
```
PRIVATE/AVCHD/BDMV/
  STREAM/
    00001.MTS       ← MPEG transport stream segments
  CLIPINF/
    00001.CPI       ← clip info
  PLAYLIST/
    00001.MPL       ← playlist
  INDEX.BDM
  MOVIEOBJ.BDM
```

### Older MP4 (HX, WX series)
```
MP_ROOT/
  100ANV01/
    MAH00001.MP4
```

---

## Capture Date Precedence

When resolving the canonical capture timestamp for an asset, apply in this order (first non-null wins). The result also records its **source** and a **confidence** (ADR-014):

1. **Sony XML sidecar** `CreationDate` attribute.
2. **EXIF `DateTimeOriginal`** combined with `OffsetTimeOriginal` (or `OffsetTime`) for timezone.
3. **AVCHD in-stream MDPM** (Metadata Payload) timestamp from the transport stream.
4. **MP4/MOV container atoms**: `©day` atom or `mvhd` creation time (adjusted for the 1904 epoch).
5. **Filesystem dates are never used** as a fallback — they are too unreliable.

Confidence is **high** when the source gives both local time and UTC offset and no other source disagrees by more than 2 s; **medium** when the offset was borrowed from the nearest asset of the same camera (within 12 h) or derived from a local clock plus a UTC container time, or when sources disagree; **low** when the offset is unknown (`utc_offset_min = None`; `utc_ms` then holds the best clock reading). A Keepsake XMP sidecar (user-corrected) always wins over probed values.

---

## Ingest Pipeline

```
1. Detect volume
   └─ Identify card layout (XAVC / AVCHD / DCIM / MP_ROOT mix)

2. Enumerate and group
   └─ Walk card, group files into logical Assets:
      JPEG + ARW + HIF → one Asset
      MP4 + XML sidecar + SUB proxy → one Asset
      MTS segments sharing a playlist → one Asset

3. Quick-diff
   └─ Compare xxHash3 of each file against catalog
   └─ Mark each Asset as: New / AlreadyImported / Modified

4. User preview and confirmation
   └─ Show new items with thumbnails
   └─ User selects destinations and confirms

5. Copy with streaming BLAKE3
   └─ Read source once, tee to each destination's writer thread, compute BLAKE3 inline
   └─ Write .<name>.keepsake-partial-<session>, fsync
   └─ Re-read the temporary file past the OS cache; compare; record the hash
   └─ Rename to the final name (never replacing a file)

6. Verify
   └─ Re-read every final file from every destination
   └─ Compare BLAKE3 against the stream hash; a mismatch is moved aside, never left
   └─ Each step is stored in import_assets / import_items, so a crash resumes

7. Write catalog
   └─ Insert Asset, File, Volume records
   └─ Write XMP sidecar stub (capture date, camera, lens from EXIF) if not present

8. Queue renditions
   └─ Enqueue preview + thumbnails (render.stills, priority "recent") for each new Asset
   └─ Enqueue a proxy (render.proxy) for video Assets: the camera's H.264 SUB proxy is copied, others are encoded

9. Report
   └─ Summary: imported N assets, M bytes, verified on K destinations
   └─ Card safe to format: only when every selected asset is catalogued and each
      of its files is verified on two different destination volumes (ADR-017)
```

---

## XMP Sidecars

One sidecar per copy of an asset's original, named `<name>.<ext>.xmp` beside it. If the volume is read-only or offline, the sidecar goes to `<library>/sidecars/<volume-uuid>/<rel_path>.xmp`. Lightroom's `<stem>.xmp` is read as a fallback but never written. Writes are atomic (temporary file + rename) and keep every property Keepsake does not manage.

| Data | Written as | Also read |
|---|---|---|
| Rating | `xmp:Rating` | Lightroom `-1` (rejected) is read as 0 |
| Colour label | `xmp:Label` + `digiKam:ColorLabel` | `digiKam:ColorLabel` when `xmp:Label` is absent |
| Comment | `dc:description` `x-default` (other languages kept) | |
| User tags | `dc:subject`, `lr:hierarchicalSubject` (`A&#124;B`), `digiKam:TagsList` (`A/B`) | flat `dc:subject` keywords not in a hierarchy |
| Capture time | `exif:DateTimeOriginal`, `photoshop:DateCreated` (already shifted) | `xmp:CreateDate` |
| Time-shift | `keepsake:CaptureShiftSeconds` | |
| Camera, lens, duration | `tiff:Make`, `tiff:Model`, `exifEX:LensModel`, `xmpDM:duration` | |
| Identity, event, AI tags, recipes, member files | `keepsake:AssetID`, `keepsake:Event`, `keepsake:AITags`, `keepsake:Recipes`, `keepsake:Files` | |

The `keepsake:` namespace is `http://ns.keepsake.app/xmp/1.0/`. `xmp:MetadataDate` decides which sidecar wins when two disagree.

## Library Folder

```
library-<uuid>/
  library.json     identity
  volumes.json     volume registry (ids, labels, case/read-only flags, last mount hint)
  catalog.db       SQLite catalog (rebuildable; -wal / -shm beside it)
  recipes/<asset-uuid>/<recipe-uuid>.json
  sidecars/<volume-uuid>/...   mirrored sidecars for read-only or offline volumes
```

`volumes.json` lives outside the catalog so volume UUIDs survive a catalog deletion.

## Catalog as Rebuildable Cache

The SQLite catalog stores only information that can be derived from files already on disk plus XMP sidecars and the library folder. If the catalog is deleted:

```bash
keepsake rebuild-catalog --fresh
```

`rebuild::rebuild_catalog(catalog, library, volume, mount)` reconstructs one volume and never writes to it:

1. Walk the volume (hidden/system entries skipped) and hash every file with BLAKE3.
2. Read sidecars beside the files and in the mirror; per file, the newest `xmp:MetadataDate` wins.
3. Form assets from Keepsake sidecars' `keepsake:Files` lists, then group the remaining files with the Sony card heuristics (one original per group; ARW/HIF companions become `RawPair`, `SUB/` files `CameraProxy`, XML/CPI/MPL `Sidecar`). Assets without a Keepsake sidecar get an id derived from the original's location.
4. Replace the volume's rows in one transaction. If the asset already exists from another volume, the sidecar wins only when its metadata is newer.

Not rebuilt: renditions (regenerated on demand), import session history, and labels or tags nothing uses. A test builds a catalog, deletes it, rebuilds, and asserts the two are equal (`crates/keepsake-core/tests/rebuild_tests.rs`).
