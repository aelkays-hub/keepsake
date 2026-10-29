# Keepsake — Feature Specification

## MVP

The MVP ships a working camera-card ingest and a browsable, searchable library. No video editing, no cloud, no AI.

### Ingest
- Detect Sony camera cards (SD, CFexpress, and cameras that show their card over USB) on insert, recognise each card again by its file-system serial in any reader or slot, and show "N new items since last import". Cameras that only speak MTP are v1 (ADR-002).
- Optionally import cards marked as trusted automatically, with a saved preset (destinations, template, event).
- Preview new items (items not yet in the catalog) before importing; do not auto-import.
- Hash-verified copy to one or two user-chosen destinations using streaming BLAKE3.
- Re-read and verify every byte after copy; report card safe to format only after two verified destinations.
- Rename and file media by a user-configurable template (e.g. `{YYYY}/{MM}/{DD}/{camera}/{original}`).
- Preserve Sony sidecars and proxy files alongside their clips:
  - `PRIVATE/M4ROOT/CLIP/*.XML` sidecar with each XAVC clip.
  - `PRIVATE/M4ROOT/SUB/*` camera-generated proxy alongside each clip.
  - AVCHD `MPL`/`CLPI` playlist files alongside `BDMV/STREAM/*.MTS` segments.

### Folders already on disk
- Add a folder of existing photos and videos (Folders screen, the empty library's "Add a folder…", `keepsake add-folder`). It is catalogued where it is: nothing is copied, moved or changed, and nothing is written into the folder. Edits go to the library's mirrored sidecars.
- Lightroom, digiKam and Keepsake `.xmp` sidecars in the folder are read (ratings, labels, keywords, dates).
- A Keepsake library or the thumbnail cache inside the folder is left out. A folder that overlaps one already in the library is refused.
- Progress, Stop (nothing is saved), and "Scan again" for new, changed and removed files.

### Library views
- Layout: sidebar (Library, Calendar, Events, Volumes, Imports), main area, inspector, status bar. Light and dark themes follow the OS. Everything can be done from the keyboard.
- **Calendar view**: year → month → day drill-down, with item counts and cover thumbnails per cell; a day opens its grid.
- **Calendar tree** in the sidebar: Calendar → years → months → days, with counts (respecting the filters), plus "No date". Clicking a node opens that level (a day opens its grid). The arrow only opens or closes; with the keyboard, → opens or goes in and ← closes or goes up. The tree opens to wherever the calendar view is.
- **Grid view**: virtualized thumbnail grid (TanStack Virtual) that stays smooth at 300,000 items. Multi-select by mouse and keyboard, sort by capture time and more. Items on offline drives show their cached thumbnail with an offline badge.
- **Details view**: virtualized table with sortable columns: name, date, camera, lens, type, size, duration, rating.
- **Inspector**: metadata, rating (1–5), colour label, comment and keywords, written to XMP. Rating, label and keywords apply to the whole selection.
- **Import window**: opens when a Sony card is inserted. It shows new items grouped by day, one tile per asset with a live thumbnail, plus select all/none, a preset or destination and template, a second-destination toggle, a pre-check (space, same-disk rule, example name), progress with Stop, and the safe-to-format result.

### Metadata and search
- Ratings (1–5 stars), colour labels, free-text comments stored in XMP sidecars.
- Search by date range, camera model, lens, media type (photo / video / raw), minimum rating, and free text, from a search bar above the grid.
- Batch time-shift: pick a reference photo, enter the time it really was, preview the offset on the selection, then write the corrected `DateTimeOriginal` to the XMP sidecars (never to the original file). Ctrl+T.

### Viewer
- Full-screen viewer (Enter or double-click; Esc closes, F for full screen). Arrows move through the selection, or through the whole view when one item is selected. Rating and label keys work in the viewer.
- Photos: the preview fits the window; Z, a click or the wheel zooms to 100% from a full-size picture made on demand (JPEG, HEIF, and RAW decoded by LibRaw). Drag or arrows pan.
- RAW + JPEG pairs are one item, with a JPEG | RAW toggle (R).
- Video: the proxy with a scrub bar, Space to play, `,` and `.` to step one frame, J and L to jump a second, and the timecode of the frame on screen (start timecode + position, drop-frame aware).
- Renditions for every asset: 256 px and 1024 px thumbnails, a 2560 px preview (never enlarged), and for video a 720p H.264 proxy (or the camera's H.264 SUB proxy). They are made in the background in this order: on screen, then recent imports, then the rest. Work pauses on battery and during imports. The cache has a size limit, and renditions of offline drives are always kept. Speeds are in `docs/PERFORMANCE.md`.
- Video playback via camera-generated proxies or app-generated H.264 proxies (webviews cannot decode XAVC HS 10-bit or AVCHD natively).

### Backup and verification
- Backup screen: choose a destination, run an incremental verified backup with progress and Stop, and see every backup and verify in a history (`<library>/history.json`), with the last verification date per destination.
- Verify any previously imported destination or backup on demand (re-hash, compare against catalog).
- Visual report of missing, corrupt, or mismatched files.

### Interop
- Open the item in another application (Ctrl+O): the system default, or an application set per file type in Settings.

### Help and diagnostics
- Logs and crash reports are kept in a local folder only; there is no telemetry. After a crash the app says so once, at the next start.
- "Copy diagnostic report" (Settings, the crash notice and the error screen) copies versions, tools, library counts, settings, the last crash and recent log lines, with the home folder shown as `~`.

### Accessibility
- Every MVP flow works with the keyboard alone.
- Items, controls, progress and errors are announced by screen readers.
- Text meets WCAG 2.1 AA contrast in both themes.

### Settings
- Library (open or create another), cache folder (with removal of the old cache's files after a move), cache size limit, image and video workers, pause on battery, Open in applications (several per file type; the first is the default for Ctrl+O, all are in the Open in… menu of the inspector and the viewer), and import presets (create, edit, delete). Ctrl+,.
- CLI commands (manual: `docs/CLI.md`): `init`, `ingest` (with `--dry-run`, `--resume`, `--status`, `--list`), `verify`, `search`, `shift-time`, `backup`, `rebuild-catalog`, `probe`, `watch` (card events; `--auto-ingest`), `card` (list, trust, untrust, rename, forget), `preset` (save, list, delete), `renders` (status, run, enqueue, show, retry, evict, bench). Every command has `--json`.

---

## v1

Builds on MVP. Adds lossless video tools and basic image adjustments.

### Lossless video tools (all produce new files; originals untouched)
- **Trim**: keyframe-aligned cut via stream copy; smart-cut mode re-encodes only the partial GOP at each cut point to achieve frame-accurate results.
- **Combine**: concatenate clips with matching streams (same codec, resolution, frame rate) via stream copy.
- **Rotate**: write rotation metadata only (no re-encode).
- **Frame grab**: extract a single frame as TIFF; 16-bit output for 10-bit sources.
- **MP4 conversion**: transcode to H.264 or HEVC MP4 with hardware encoders (VideoToolbox on macOS, NVENC / QSV / AMF on Windows) and software fallback.

### Image adjustments (all produce new derivative files)
- Crop, resize, red-eye removal, auto-levels.
- Output formats: JPEG, PNG, TIFF.
- Edit recipe stored as versioned JSON alongside the derivative.

### Library enhancements
- Burst detection and grouping (consecutive frames, same second, same camera).
- Interval/time-lapse grouping.
- Sony face and highlight markers imported from XML sidecar.

### Export
- **Flickr export**: OAuth 2.0, batch upload, album mapping, title/description/tags from metadata.

### Additional import
- MTP import for cameras that do not expose a card filesystem.

---

## v2

Builds on v1. Adds multi-view composition, time-lapse, maps, and AI.

### Multi-view player and export
- Up to 4 clips displayed and played in sync.
- Add music track, captions, per-clip speed adjustment.
- Export as a single MP4 (software or hardware encode).

### Time-lapse
- Assemble interval-grouped stills into a video.
- Deflicker filter (rolling average luminance normalisation).

### Map view
- OpenStreetMap base layer (offline tile cache).
- Plot assets by GPS EXIF or embedded GPX.
- Import GPX track file and correlate with assets by timestamp.

### On-device AI
- Auto-tagging: scene, subject, and activity labels without cloud round-trips.
- Face detection and clustering with user-assignable names.
- Semantic search: natural-language query over embedded vectors.

### Tethering (optional plug-in)
- Sony Camera Remote SDK: live view and remote shutter for supported bodies.
- Shipped as a separate opt-in plug-in to contain the SDK licence requirements.

### Timeline export
- Export edit decisions as OpenTimelineIO for round-tripping with DaVinci Resolve, Final Cut Pro, etc.

### Auto highlight reel
- Heuristic selection of rated, face-detected, and high-motion clips assembled into a short MP4.

---

## Out of Scope (all versions)

- Disc authoring (Blu-ray, DVD).
- Sony Music Transfer.
- GPS assist file download.
- Print date stamps.
- 3D / VR playback.
- Any proprietary cloud service (Sony Imaging Edge Mobile cloud, Google Photos, iCloud, etc.).
