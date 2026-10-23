# Keepsake — Feature Specification

## MVP

The MVP ships a working camera-card ingest and a browsable, searchable library. No video editing, no cloud, no AI.

### Ingest
- Detect Sony camera cards (SD, CFexpress) and MTP-connected cameras on insert/connect.
- Preview new items (items not yet in the catalog) before importing; do not auto-import.
- Hash-verified copy to one or two user-chosen destinations using streaming BLAKE3.
- Re-read and verify every byte after copy; report card safe to format only after two verified destinations.
- Rename and file media by a user-configurable template (e.g. `{YYYY}/{MM}/{DD}/{camera}/{original}`).
- Preserve Sony sidecars and proxy files alongside their clips:
  - `PRIVATE/M4ROOT/CLIP/*.XML` sidecar with each XAVC clip.
  - `PRIVATE/M4ROOT/SUB/*` camera-generated proxy alongside each clip.
  - AVCHD `MPL`/`CLPI` playlist files alongside `BDMV/STREAM/*.MTS` segments.

### Library views
- **Calendar view**: year → month → day drill-down.
- **Grid view**: thumbnail grid with TanStack Virtual for smooth scrolling of large libraries.
- **Details view**: full metadata panel for the selected asset.

### Metadata and search
- Ratings (1–5 stars), colour labels, free-text comments stored in XMP sidecars.
- Search by date range, camera model, lens, media type (photo / video / raw), and free text.
- Batch time-shift: write corrected `DateTimeOriginal` to XMP sidecar (never to the original file).

### Viewer
- JPEG and HEIF display.
- ARW raw preview (embedded JPEG or LibRaw decode).
- Video playback via camera-generated proxies or app-generated H.264 proxies (webviews cannot decode XAVC HS 10-bit or AVCHD natively).

### Backup and verification
- Verify any previously imported destination on demand (re-hash, compare against catalog).
- Visual report of missing, corrupt, or mismatched files.

### Interop
- Open selected file in the system default application.
- CLI commands: `ingest start | resume | status | list`, `verify`, `library rebuild` (rebuild the catalog from files and sidecars), `probe` (show what Keepsake reads from a card).

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
