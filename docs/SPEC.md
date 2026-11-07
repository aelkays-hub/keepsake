# Keepsake — Feature Specification

## MVP

The MVP ships a working camera-card ingest and a browsable, searchable library. No video editing, no cloud, no AI.

### Ingest
- Detect Sony camera cards (SD, CFexpress, and cameras that show their card over USB) on insert, recognise each card again by its file-system serial in any reader or slot, and show "N new items since last import". Cameras that only speak MTP are v1 (ADR-002).
- Optionally import cards marked as trusted automatically, with a saved preset (destinations, template, event).
- Or import **every** card by itself (Settings → When a card is inserted): new items go to one folder (e.g. a folder added in place, `E:\Pictures`) in a folder layout (default year › month name › day, `2026\October\2026-10-30\`), with missing folders made. The import window opens, shows where they are going, starts on its own, and closes itself 5 s after a good import (Keep open stops that; it stays open after a problem). A missing drive or a failed check stops it, with the reason.
- Preview new items (items not yet in the catalog) before importing; do not auto-import.
- Hash-verified copy to one or two user-chosen destinations using streaming BLAKE3.
- Re-read and verify every byte after copy; report card safe to format only after two verified destinations.
- Rename and file media by a user-configurable template (e.g. `{YYYY}/{MM}/{DD}/{camera}/{original}`).
- Preserve Sony sidecars and proxy files alongside their clips:
  - `PRIVATE/M4ROOT/CLIP/*.XML` sidecar with each XAVC clip.
  - `PRIVATE/M4ROOT/SUB/*` camera-generated proxy alongside each clip.
  - AVCHD `MPL`/`CLPI` playlist files alongside `BDMV/STREAM/*.MTS` segments.

### Folders already on disk
- Add a folder of existing photos and videos (Folders screen, the empty library's "Add a folder…", `keepsake add-folder`). It is catalogued where it is: nothing is copied, moved or changed, and nothing is written into the folder. Edits go to the library's mirrored sidecars. The folder is in the library (tree, Folders screen) as soon as it is added, and its items are saved 20 files at a time as they are read, so the tree and counts fill in while it runs. If the scan stops or fails, what was saved stays, and adding the folder again carries on from there without reading those files again. Only the start and end of each file is read while adding; each file's full fingerprint is made afterwards in the background, and the Folders screen shows how many files are left.
- Lightroom, digiKam and Keepsake `.xmp` sidecars in the folder are read (ratings, labels, keywords, dates).
- A Keepsake library or the thumbnail cache inside the folder is left out. A folder that overlaps one already in the library is refused.
- Progress, Stop (nothing is saved), and "Scan again" for new, changed and removed files.
- In the sidebar's folder tree, right-click a folder (or press the Menu key or Shift+F10) for **Show** and **Scan again**. Scan again on a folder inside an added folder reads only that folder and the folders in it: new and changed files there come in, files gone from there are removed, and the rest of the added folder is left as it is. The Folders screen shows the progress and outcome.

### Library views
- Layout: sidebar (Library, Calendar, Events, Volumes, Imports), main area, inspector, status bar. Light and dark themes follow the OS. Everything can be done from the keyboard.
- **Calendar view**: year → month → day drill-down, with item counts and cover thumbnails per cell; a day opens its grid. Only the months and days that have items (matching the filters) are shown, in date order; each day reads its weekday and number ("Sat 14").
- **Calendar tree** in the sidebar: Calendar → years → months → days, with counts (respecting the filters), plus "No date". Clicking a node opens that level (a day opens its grid). The arrow only opens or closes; with the keyboard, → opens or goes in and ← closes or goes up. The tree opens to wherever the calendar view is.
- **Folder tree**: a **By date / By folder** switch above the tree (remembered). By folder, the roots are the volumes (folders added in place, import destinations). Below them are their folders as on disk, each with the number of items in it and below that match the filters. Clicking a folder shows everything in it and its sub-folders. The Volumes list is hidden then, since the tree holds it. Same keys as the calendar tree.
- **Grid view**: virtualized thumbnail grid (TanStack Virtual) that stays smooth at 300,000 items. Multi-select by mouse and keyboard: Ctrl/⌘-click, Shift-click, a tick-box on each tile (shown on hover), **Select** mode in the toolbar (a plain click adds or removes; All, None; Esc leaves it), and dragging a rectangle across the grid (Ctrl/Shift or Select mode adds to the selection; the grid scrolls near its edges). Sort by capture time and more. Items on offline drives show their cached thumbnail with an offline badge. Thumbnails come in **Small, Medium and Large** (a switch in the toolbar, or + and − in the grid; remembered). Large tiles use the 1024 px thumbnail.
- **Details view**: virtualized table with sortable columns: name, date, camera, lens, type, size, duration, rating.
- **Inspector**: metadata, rating (1–5), colour label, comment and keywords, written to XMP. Every edit (rating, label, comment, keywords) applies to the whole selection. With several selected, the edit fields show what they share: one rating, label or comment, or "Mixed"; keywords on all of them, then keywords on some with "n of m" and a + to put one on all.
- **All metadata** (inspector, collapsible): every tag ExifTool reads from the item's file (EXIF, GPS, the camera's maker notes, XMP, file facts), grouped, with a filter and Copy, and a JPEG/RAW switch for pairs. Read only when open; the file is only read.
- **Import window**: opens when a Sony card is inserted. It shows new items grouped by day, one tile per asset with a live thumbnail, plus select all/none, a preset or destination and template, a second-destination toggle, a pre-check (space, same-disk rule, example name), progress with Stop, and the safe-to-format result.

### Metadata and search
- Ratings (1–5 stars), colour labels, free-text comments stored in XMP sidecars.
- Search by date range, camera model, lens, media type (photo / video / raw), minimum rating, and free text, from a search bar above the grid.
- Batch time-shift: pick a reference photo, enter the time it really was, preview the offset on the selection, then write the corrected `DateTimeOriginal` to the XMP sidecars (never to the original file). Ctrl+T.

### Viewer
- Full-screen viewer (Enter or double-click; Esc closes, F for full screen). Arrows move through the selection, or through the whole view when one item is selected. Rating and label keys work in the viewer.
- Photos: the preview fits the window. A **zoom bar** under the picture (Fit, −, a slider from Fit to 400%, +, 100%, and the zoom level), the + and − keys and the mouse wheel (around the pointer) zoom; Z or a click switch between Fit and 100%. Zoom uses a full-size picture (JPEG, HEIF, and RAW decoded by LibRaw), made in the background half a second after a photo opens, so zooming is quick; an upright JPEG is used as it is. Zoom and pan move the picture without re-laying out the page, so they keep up with the wheel and slider. Drag or arrows pan.
- RAW + JPEG pairs are one item, with a JPEG | RAW toggle (R).
- **Information panel** (the Info button at the top right, or I): a panel on the right of the viewer with the item's key facts (date and time, camera, lens, exposure, ISO, focal length, duration and frame rate for video, dimensions, file size, location), then every tag ExifTool reads from the file, grouped, with a filter and Copy (the same list as the inspector's All metadata). It follows the item as you move through the viewer, and for a RAW + JPEG pair it shows the file chosen with JPEG | RAW. The picture's area shrinks to make room; Fit and zoom use the room that is left. Shown or hidden is remembered on this computer. An item on an unplugged drive shows what the catalog knows, with a note.
- Video: the proxy with a scrub bar, Space to play, `,` and `.` to step one frame, J and L to jump a second, and the timecode of the frame on screen (start timecode + position, drop-frame aware).
- Renditions for every asset: 256 px and 1024 px thumbnails, a 2560 px preview (never enlarged), and for video a 720p H.264 proxy (or the camera's H.264 SUB proxy). They are made in the background in this order: on screen, then recent imports, then the rest. Work pauses on battery and during imports. The cache has a size limit, and renditions of offline drives are always kept. When it is nearly full, thumbnails are kept before previews and video proxies, and background work makes thumbnails only; opening a photo or video makes its preview or proxy. Thumbnails alone of a camera JPEG come from the small preview the camera stores in the file, so the whole file is not read. Background fingerprints wait while thumbnails are being made. Speeds are in `docs/PERFORMANCE.md`.
- Video playback via camera-generated proxies or app-generated H.264 proxies (webviews cannot decode XAVC HS 10-bit or AVCHD natively).

### Backup and verification
- Backup screen: choose a destination, run an incremental verified backup with progress and Stop, and see every backup and verify in a history (`<library>/history.json`), with the last verification date per destination.
- Verify any previously imported destination or backup on demand (re-hash, compare against catalog).
- Visual report of missing, corrupt, or mismatched files.

### Library report
- Tools → **Report**: how many photos and videos the library holds and the disk space they take, from the catalog alone (no media file is read, so unplugged drives count too).
- Totals (photos, videos, files, size) and one table at a time, broken down by **year, month, day, camera, lens, file type or drive**. Each row: photos, videos, items, files and size (exact bytes on hover). Unknown dates, cameras, lenses and extensions are one **Unknown** group, listed last. Dates are newest first; the others most items first.
- A RAW+JPEG pair is one photo; its files and size count both halves (and sidecars). A file copied to several drives (backups, a second import destination) counts once, except in the **Drive** table, which shows every copy on each drive. File types and drives show items only (no photo/video split); a pair counts under both of its file types.
- Each row's name opens its items in the library grid, with no other filters, so the grid holds the row's item count: a year, month or day; the undated items; one camera or lens (exact name) or the items without one; the items with a file of that type; the items on a drive. The heading reads **Report › Year 2024**, and **Report** goes back to the same table.
- **Export to CSV…** writes every breakdown to one file (UTF-8 for Excel; raw bytes and a readable size). `keepsake report` prints the same and `--csv` writes the file.

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
- CLI commands (manual: `docs/CLI.md`): `init`, `ingest` (with `--dry-run`, `--resume`, `--status`, `--list`), `verify`, `search`, `shift-time`, `backup`, `report` (by year, month, day, camera, lens, type or drive; `--csv`), `rebuild-catalog`, `probe`, `watch` (card events; `--auto-ingest`), `card` (list, trust, untrust, rename, forget), `preset` (save, list, delete), `renders` (status, run, enqueue, show, retry, evict, bench). Every command has `--json`.

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
- **Done early (ADR-035), off until turned on in Settings → On-device AI**, which downloads the model once (372 MB, SigLIP 2 base, Apache-2.0) into this computer's cache folder. It runs on the graphics card where it can, else the processor, from the thumbnails; no photo leaves the computer and no original is changed.
- **Only the dates you ask about, one task at a time:** right-click a year, month or day in the sidebar's calendar tree (or press the Menu key or Shift+F10).
  - **Find duplicates** compares that period's RAW photos and opens the Duplicates screen on it, showing how far it has got.
  - **Tag photos** tags all of that period's photos and shows its progress in the status bar.
  - The two are separate, and nothing is analysed in the background otherwise.
- Auto-tagging: up to 6 tags per photo from a built-in list of 110 (people, beach, dog, food, sunset, moon, mosque, …), shown in the inspector as AI tags. Baby, Child, Portrait and other people tags also give People. A photo with no sure tag gets the model's best guess (one or two).
- The toolbar's **Tag** list shows every keyword and AI tag with its count; choosing one shows only the items that have it.
- Find duplicates and Tag photos also work on a folder and every folder below it: right-click it in the folder tree. A window shows the progress, with Stop and Hide. Search finds them like keywords. ✕ removes one for good (it is not given again); ✓ makes it your own keyword.
- Same-day duplicates (Sidebar → Duplicates): near-identical **RAW** photos taken on one calendar day, grouped: bursts, re-shoots, the same shot twice. A JPEG with the same name as a RAW is an edited copy, so JPEG-only photos (and videos) are never grouped. Click the one to keep; the others are marked "not kept" (catalog and XMP), and the toolbar's **Hide not kept** leaves them out of the library. Not duplicates and Undo per group; keys K, N, U. Nothing is moved or deleted.
- `keepsake ai status | install | run | duplicates [DAY] | retag`.
- Still to come (v2):
  - Face detection and clustering with user-assignable names.
  - Semantic search: natural-language query over embedded vectors.
  - Your own tag words.

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
