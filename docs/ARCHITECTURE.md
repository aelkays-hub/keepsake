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
  app/src-tauri     keepsake-cli    (future daemon)
```

All three executables call into `keepsake-core` through the same public API. There is no business logic outside of `keepsake-core` and `keepsake-platform`.

---

## Components

### DeviceWatcher
Monitors OS device-arrival events (Windows: `WM_DEVICECHANGE` / `RegisterDeviceNotification`; macOS: IOKit `IOServiceMatching`). On card insert or camera connect it enumerates volumes, identifies Sony card layouts, and emits an event to the IngestEngine.

Lives in `keepsake-platform` (OS-specific implementations behind a trait) and is called from `keepsake-core`.

### IngestEngine
Orchestrates the full import pipeline (see _Ingest Pipeline_ below). Emits structured progress events over a `tokio` broadcast channel so the UI and CLI can both subscribe.

### Catalog
SQLite database in WAL mode with FTS5 full-text index. Stores Assets, Files, Volumes, Renditions, Events, EditRecipes, and ImportSessions. The catalog is a **rebuildable cache**: it can be discarded and reconstructed from the original files and XMP sidecars. XMP sidecars and JSON recipes are the authoritative source of truth for all user metadata.

### Probe
Reads media metadata without decoding pixels:
- Calls ExifTool in `-stay_open` mode for EXIF/XMP/IPTC metadata.
- Calls ffprobe for container/stream information.
- Reads Sony XML sidecars directly.
- Returns a `MediaInfo` struct consumed by IngestEngine and Catalog.

### RenderService
Generates and caches Renditions (thumbnails, previews, proxies, export frames). Runs on a bounded async worker pool. All work is keyed by `(source_hash, recipe_hash)` so results are deduplicated and reusable across library rebuilds.

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
| `rel_path` | `PathBuf` | Relative to volume root |
| `blake3` | `[u8; 32]` | Content identity hash |
| `size_bytes` | `u64` | |
| `role` | `FileRole` | See below |

`FileRole` variants: `Original`, `RawPair`, `Sidecar`, `CameraProxy`, `AppProxy`, `Export`.

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

When resolving the canonical capture timestamp for an asset, apply in this order (first non-null wins):

1. **Sony XML sidecar** `CreationDate` attribute.
2. **EXIF `DateTimeOriginal`** combined with `OffsetTimeOriginal` (or `OffsetTime`) for timezone.
3. **AVCHD in-stream MDPM** (Metadata Payload) timestamp from the transport stream.
4. **MP4/MOV container atoms**: `©day` atom or `mvhd` creation time (adjusted for the 1904 epoch).
5. **Filesystem dates are never used** as a fallback — they are too unreliable.

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
   └─ Read source, tee to each destination stream, compute BLAKE3 inline
   └─ Write rendition-cache entry for each file as it lands

6. Verify
   └─ Re-read every written file from every destination
   └─ Compare BLAKE3 against recorded hash
   └─ Mark ImportSession.verified = true when all files pass on both destinations

7. Write catalog
   └─ Insert Asset, File, Volume records
   └─ Write XMP sidecar stub (capture date, camera, lens from EXIF) if not present

8. Queue renditions
   └─ Enqueue thumbnail and preview generation for each new Asset
   └─ Enqueue proxy generation for video Assets lacking a camera proxy

9. Report
   └─ Summary: imported N assets, M bytes, verified on K destinations
   └─ Card safe to format: only when verified == true on ≥ 2 destinations
```

---

## Catalog as Rebuildable Cache

The SQLite catalog stores only information that can be derived from files already on disk plus XMP sidecars. If the catalog is deleted:

```bash
keepsake rebuild-catalog --library /path/to/library
```

This re-scans all volumes, re-reads EXIF/XMP, and reconstructs the catalog. User metadata (ratings, labels, comments, tags, edit recipes) is preserved because it lives in XMP sidecars and JSON recipe files, not in the catalog.

Renditions (thumbnails, proxies) are regenerated on demand after a rebuild.
