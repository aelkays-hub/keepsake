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
  app/ui/src-tauri  keepsake-cli    (`keepsake watch`)
```

All three executables call into `keepsake-core` through the same public API. There is no business logic outside of `keepsake-core` and `keepsake-platform`.

The CLI (`keepsake-cli`, manual in `docs/CLI.md`) parses arguments, picks the library (`--library`, else `KEEPSAKE_LIBRARY`, else the default in `config.toml`), prints text or `--json`, draws progress bars and turns Ctrl-C into a "finish the current file, then stop" flag. The work itself is done in core:
- `ingest::{preview, plan, run, report}` for `ingest`;
- `verify::verify` for `verify`;
- `catalog::Search::query` (`SearchQuery`) for `search`;
- `select::resolve` and `MetadataService::shift_capture_time` for `shift-time`;
- `backup::run` for `backup`;
- `rebuild::rebuild_library_opts` for `rebuild-catalog`;
- `rebuild::add_folder` for `add-folder` (and the app's Folders screen, `folder_add`): register a folder as a read-only volume and catalogue it in place. `RebuildOptions` carries the prober, the cache folders (only Keepsake's own sub-folders in them are left out, `render::cache::CACHE_DIRS`), a stop flag and progress. A library's own files inside a scanned folder are always left out (`library::LIBRARY_ENTRIES`). With `batch_files` (the app and the CLI: 20) the folder is registered first and its items are hashed, probed and saved a batch at a time; unchanged files saved by an earlier scan are skipped, so a stopped scan resumes (ADR-028). The app sends `folder://saved` after each batch. `rebuild-catalog` still reads everything and saves in one transaction;
- `watch::Watcher`, with `keepsake_platform::system_devices()`, for `watch`;
- `cards` and `presets` for `card` and `preset`.

**The app** (`app/ui`, ADR-022):
- **Bridge.** `src-tauri` exposes typed commands and events. The types are in `src-tauri/src/api.rs`, with TypeScript generated into `app/ui/src/bindings/` by `ts-rs`. The React UI calls them only through `app/ui/src/api.ts`.
- **App state** holds the open library, one read connection (writes get their own), the view snapshots, and handles to the background work:
  - the render service (`render://thumbs` when a thumbnail is made);
  - the device watcher (`device://card`, which opens the `import` window);
  - a running import (`import://progress`, `import://done`).
- **Grid and details** show a *view*: `view_open(query, sort)` keeps the sorted ids (`Browse::ordered_ids`), and `view_rows` pages through them.
  - The UI virtualizes with TanStack Virtual and keeps about 30 pages of 200 rows.
  - It reports the tiles on screen with `render_set_visible`.
  - Selections are index ranges into the view.
- **Calendar** buckets come from `Browse::calendar`. The sidebar uses `Browse::{volume,event,session}_counts`.
- **Inspector edits** go through `MetadataService` (XMP), on a thread for large selections (`edit://progress`).
- **The UI** has no router or state library: a reducer (`uiModel.ts`), a context (`state.tsx`) and CSS variables for the light and dark themes.

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
- **Report:** `tx.report().library()` (`catalog/repo/report.rs`) gives the library report: totals and breakdowns by day, month, year, camera, lens, file type and drive, as GROUP BY queries. Dates, cameras and lenses are one scan of `assets` using `assets.file_count` / `file_bytes` (schema v8, kept by triggers: one copy per BLAKE3 key per asset), summed per (day, camera, lens) once and rolled up from that. File types and drives are one scan of `files` each, with no join to `assets`. About 1.5 s at 300,000 items with every breakdown. `keepsake_core::report` writes it as CSV.
- **User edits:** `metadata::MetadataService` updates the rows and rewrites the asset's sidecars in one transaction (see _XMP Sidecars_).

### Probe
Reads media metadata without decoding pixels (`keepsake_core::probe`):
- `ExifTool`: one long-lived `-stay_open` process behind a request queue, with per-request timeouts and automatic restart. It never quits by itself: `shutdown()` (or dropping the handle) stops it. The app does this on exit, and on Windows its helpers share a kill-on-close job, so they end with it even after a crash (ADR-037).
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
  - LRU eviction keeps the cache under `max_bytes`, previews and proxies before thumbnails, but never removes a rendition whose original is not on a mounted volume. The render service trims to 90% of the limit, and while the cache is above 80% its background jobs make thumbnails only; opening an item makes the rest (ADR-031). Thumbnails alone of a camera JPEG are made from its built-in MPF preview, without reading the whole file (ADR-032).
  - The root is `[render] cache_dir` in `config.toml` (default: the OS cache folder).
- **Thumbnail backfill** (`render::backfill`, `keepsake renders thumbnails`, ADR-033): a headless walk, separate from the job queue, that makes every missing 256/1024 px thumbnail (never the preview or a proxy).
  - It walks `assets` in `seq` order from a saved position (`library_meta` key `render.backfill.cursor`), 100 assets per batch.
  - A batch is rendered first (files into the cache through `.tmp` and a rename; `Cache::store_bytes` and `still::make_stills` return the rows unwritten), with `workers` threads that each have their own read connection. Then one short write records all the batch's `renditions` rows and the new position, retried for up to two minutes while another writer holds the catalog. A kill loses at most the batch in progress, whose files are made again next time.
  - A file that fails (or panics in a decoder) is reported (`BackfillEvent::ItemFailed`) and passed; only catalog errors stop the run. A batch whose media is all offline stops the run without saving a position past it, unless `skip_offline`. Ctrl-C saves the finished part of the batch.
  - At the end the position stays on the last asset, so the next run looks only at assets added since; `restart` walks everything again, which retries failures. It does not touch `jobs`: the app's queued `render.stills` jobs for those assets finish at once for the thumbnails and still make the preview.

### VideoOps
Wraps FFmpeg for all video operations: trim (stream copy or smart-cut re-encode), combine, rotate, frame grab, MP4 conversion, proxy generation, time-lapse assembly, deflicker. Every operation produces a new file; no in-place modification.

### ShareConnectors
Pluggable connectors: Flickr (OAuth 2.0), OpenTimelineIO export. Each connector implements a `Connector` trait in `keepsake-core` with send/cancel/progress.

### AIWorker
On-device inference (`keepsake_core::ai`, ADR-035). One extra worker in the render service takes `ai.analyze` jobs when AI is on (`RenderOptions::ai`). Jobs are queued only for dates the user asks about (`ai::analyze::enqueue_range`, from the calendar tree's right-click menu) or by `keepsake ai run`; the worker drops queued background-class AI jobs when it starts. It has the same pauses (battery, import) and waits while requested thumbnail jobs are ready or running.
- The AI worker claims up to 32 jobs in one catalog write, works them out, then saves every result and marks the jobs done in one more write (a write can take seconds on a USB drive). Jobs are queued for a target: a date range or a folder and those below it (`ai::Target`, a library search).
- Each job reads the asset's 1024 px thumbnail from the cache (never the original). It computes a 64-bit difference hash and a 768-number SigLIP 2 embedding (ONNX Runtime, loaded at run time from `vendor/onnxruntime/`; DirectML or CoreML, else CPU). It stores both in `ai_analysis` and gives AI tags (`asset_tags`, source `ai`) by zero-shot scoring against the built-in label vectors (`ai/labels.tsv` + `.bin`): a tag scores its best phrase, and some tags give another (Baby → People). `ai_analysis.tag_list` records the tag-list version, so photos tagged with an older list are re-tagged from their stored vector.
- Duplicates (`ai::duplicates`) are grouped on demand per capture day from the stored vectors.
- The user's decisions (removed AI tags, keeper / not kept / not duplicates) go through `MetadataService`, so they are in the catalog and the XMP sidecars. The model lives in the user's cache folder (`ai::install`), downloaded once with the OS's `curl` and checked by BLAKE3.
- Face clustering and semantic search are not built yet.

### Aircraft (ADR-041)
`keepsake_core::ai::aircraft`, on this computer only.
- `ocr::TextReader`: PP-OCRv5's detector (sides multiples of 32, long side at most `max_side`; blue, green, red planes, ImageNet mean and deviation) gives a text probability map. `ocr::boxes` joins pixels over 0.3 into regions, keeps those averaging 0.5 or more, takes the smallest rectangle around each (convex hull, every edge tried) and grows it (DBNet's unclip, 1.6). `ocr::crop` straightens a rectangle out of the full picture (at most 96 px high); `rec_input` scales it to 48 px high; `ctc_decode` reads the recogniser's output with its dictionary. The detector uses `model::open_session` (graphics card first); the recogniser runs on the processor.
- `marks`: `registration` (country shapes), `Airlines::find` (`airlines.tsv`), `painted_type`, and `interpret`, which turns a picture's lines into `Marks`.
- `types::TypeList`: a second `labels::LabelSet` (`aircraft-types.tsv` and its vectors); `guess` gives the best type only when it stands out.
- `scan`: `ai.aircraft` jobs, queued for the picked items (`enqueue`, `status`, `stop` by ids; the status is `cloud::job_status`). The aircraft worker (`RenderService`, when `[ai] aircraft`; `with_aircraft` gives tests a stand-in `Reader`) runs one job at a time: `picture` decodes the photo at full size (`render::full::decode_full`, nothing cached) or falls back to the thumbnail; `examine` reads it and, without a painted type, asks for the look (the stored `ai_analysis` embedding, else the image encoder if installed); the keywords are added with `MetadataService::add_ai_keywords(asset, "aircraft", …)`.
- A folder or dates at once: `scan::target_items` gives the items of an `ai::Target` that carry one of `AIRCRAFT_TAGS` (one tag search each, joined); `enqueue_target` queues those not read yet, `target_status` and `stop_target` follow and stop them. The app's `ai_analyze_folder` / `ai_analyze_range` and their status and stop commands hand `what` = `aircraft` to `aircraft::target_*`.
- `install`: the two pinned models (`ai::install::ModelFile`, shared with the face models), `KEEPSAKE_TEXT_DETECTOR` / `KEEPSAKE_TEXT_RECOGNIZER`.
- The app's `aircraft_*` commands are thin wrappers; the UI is `AircraftSettings`, the item menu's Read aircraft, the viewer's button, and `AiJobWindow` (the `aircraft` job).

### Edit suggestions (ADR-042)
`keepsake_core::ai::suggest`, for one photo at a time.
- `measure::measure(image)` works on the 1024 px thumbnail (`analyze::thumbnail`): a luma histogram (tones, percentiles), blown and black shares, average saturation, a cast from near-grey pixels, grain (Immerkær's mask in 16 px blocks, 20th percentile), sharpness (99th percentile of the 4-neighbour Laplacian) and tilt (Sobel gradients of a half-size copy, a histogram of angles within 15° of level). It returns `Measures` (serialisable).
- `rules::suggest` / `describe` / `summary` turn `Measures` into `Suggestion`s (title, why, Photoshop, Topaz Photo) and lines of what was measured.
- `ask`: the AI's `Task` (system text, the question with the measurements, the schema), `tidy`, and `CloudAdviser` (an `Adviser` over `cloud::send`). `cloud::Task` and `cloud::send` are shared with tagging; each provider module has `body(…, task)` and `answer(status, body, task)`.
- Storage: `edit_suggestions` (schema v15), through `Ai::{edit_advice, put_edit_measures, put_edit_ai}`. `suggest::{stored, advice, measure_asset}` read and measure; `ask_asset` is one job; `enqueue_ask`, `stop_asking` and `asking` queue, stop and follow the `ai.suggest.<provider>` jobs.
- The render service's cloud worker (`JOB_AI_CLAUDE`, started when it has a tagger or an adviser) claims `CLOUD_JOBS` and `SUGGEST_JOBS`; `with_adviser` gives it the `Adviser` (tests use a stand-in).
- The app's `edit_suggestions*` commands (`src-tauri/src/suggest.rs`) are thin; the UI is `EditAdviceWindow`, opened from the grid's item menu and the viewer (`editAdvice` in the UI state). Open in uses `open_choices` and `open_external`.

### Faces (ADR-040)
`keepsake_core::ai::faces`, on this computer only. Steps 1 (models and pipeline), 2 (searching and storing), 3 (Name and find), 4 (sidecars and search) and 5 (Inspector and viewer) are built; the People screen comes next.
- `FaceModels::find(picture)`: the picture is fitted into YuNet's fixed 640 × 640 input (blue, green, red, 0–255, black padding right and bottom). Its three grids (8, 16, 32 px cells) are decoded as OpenCV does (score √(cls·obj), box from centre offsets and log size, 5 landmarks), faces scoring under 0.8 dropped and overlapping ones merged (IoU 0.3). Boxes come back in the picture's pixels.
- `FaceModels::describe(picture, face)`: the face is straightened onto SFace's 112 × 112 five-point template (closest similarity transform, bilinear) and SFace gives 128 numbers, made unit length (`Embedding`). A cosine similarity of 0.363 or more: likely the same person.
- Both sessions come from `model::open_session` (DirectML / Core ML, else the CPU), like the image encoder. Models: `faces::install` (pinned, BLAKE3-checked, user cache or `vendor/models/`).
- `faces::scan`: `ai.faces` jobs, one per photo. The face worker (`RenderService`, when `[ai] enabled` and `faces`; `with_faces` gives tests a stand-in `FaceFinder`) reads the 1024 px thumbnail, keeps faces at least 40 px wide and stores them with `Faces::save_scan` (catalog `faces`, `face_scans`; `people`, `face_rejections` for the user's decisions). Named faces survive a new search.
- Thumbnail waits: `Shared::stills_live` counts the running thumbnail workers; the AI and face workers wait for thumbnails only while one is running.
- Sidecars and search (step 4): `metadata::project` writes the confirmed faces as MWG face regions (`xmp::FaceRegion`, nested `xmp::Value`), the rejections and the names Keepsake manages; `MetadataService::sync_faces` keeps a photo's `People|<name>` keywords and sidecar in step with its confirmed faces, called by `people::name_face` / `decide` and `scan::delete_all`. A rebuild restores people, named faces (`Faces::restore_confirmed`, model `sidecar` until searched) and rejections (`restore_rejection`, waiting in `face_rejections_pending` for a face).
- Inspector and viewer (step 5): `PeopleBlock` (Inspector) and the viewer's face layer read `faces_of_photo(search: false)`. `PhotoStage` takes an `overlay` drawn in a layer with the picture's own size and transform, so the boxes follow zoom and pan; the box menu calls `faces_not_person` / `faces_not_face` (`people::not_this_person`, `not_a_face`; state `hidden`).
- Matching (tuned): `people::Matcher` keeps a person's confirmed descriptions as sums per year; a candidate is scored against the average of the years within 3 of its photo, skipped under the cut-off (`[ai] face_match_pct`, default 55), under 60 px, or when closer to a face rejected for that person. `rescore` / `rescore_all` re-score waiting suggestions; `check` lists confirmed faces least like the others first.
- `faces::people` (Name and find): `name_face` (confirm a face as a named person), `suggest` (nearest-confirmed-face cosine ≥ 0.363, within 3 years of a confirmed face's date, one face a photo; stored as `state = 'suggested'` with `likeness`), `review`, `decide` (confirm, or reject into `face_rejections`). The app's `faces_*` commands are thin wrappers; the window (`NameFindWindow`) polls `faces_of_photo` and `faces_status` and cuts faces out of thumbnails with CSS (`faces.ts::cropStyle`, the box widened by a fifth, at the face's own shape from the searched picture's size).

### Cloud tagger
Tag with AI (`keepsake_core::ai::cloud`, ADR-036, ADR-038): Claude, ChatGPT or Gemini.
- There is one job per photo the user picked and provider: `ai.claude`, `ai.openai` or `ai.gemini`. One worker in the render service claims all three when the service has a tagger (`RenderService::with_cloud`); the app always gives it one.
- The worker reads the 1024 px thumbnail and re-encodes it as JPEG. It sends it through a `Tagger` (`CloudTagger`, over `Http` = `UreqHttp`), which builds the provider's request and reads its answer (`cloud::{claude, openai, gemini}`).
- The shared parts are in `cloud`: the prompt, the tag schema, tidying the answer (`tidy`) and what an HTTP status means (`http_error`).
- Each provider's key comes from `keepsake_platform::secrets` (the OS credential store, account `<id>-api-key`), and its model from `config.toml` (`[ai] claude_model`, `openai_model`, `gemini_model`). Both are looked up for each photo.
- The tags in the answer are added to the photo's keywords through `MetadataService::add_ai_keywords` (catalog and XMP), which also records which AI gave each one (`ai_tag_sources`, `keepsake:AITagSources`; ADR-039). Removing a keyword forgets who gave it.
- **All** (ADR-039) is the UI queueing the same photos for each AI with a key; each AI's jobs are followed on their own.
- Permanent problems fail the job (`RenderError::Refused`); others are retried.

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
| `file_count`, `file_bytes` | `u64` | Catalog only (v8, triggers): the asset's files and their size, one copy of each BLAKE3 key; for the library report |

### File
One physical file on disk. An Asset has 1-N Files.

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` | |
| `asset_id` | `Uuid` | FK to Asset |
| `volume_id` | `Uuid` | FK to Volume |
| `rel_path` | `RelPath` | Relative to volume root, exact on-disk spelling, `/`-separated |
| `rel_path_key` | `String` | NFC, lowercased on case-insensitive volumes; unique per volume |
| `blake3` | `[u8; 32]` | Content identity hash; a stand-in key while `fingerprinted` is 0 |
| `fingerprinted` | `bool` | 0 for files added in place that the background job has not read in full yet (ADR-029); `File::hash` is `FileHash::Full` or `FileHash::Pending` |
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
| Named faces (ADR-040) | `mwg-rs:Regions` (`AppliedToDimensions`, `RegionList` of `Type` Face, `Name`, `Area` centre x/y, w, h, normalized); `keepsake:FaceNames` (the names Keepsake wrote); `keepsake:FaceRejections` | every named face region, Lightroom's and digiKam's; other regions kept as they are |
| Identity, event, AI tags, recipes, member files | `keepsake:AssetID`, `keepsake:Event`, `keepsake:AITags`, `keepsake:Recipes`, `keepsake:Files` | |
| Which AI gave each keyword (Tag with AI; Read aircraft, ADR-041, as `aircraft`) | `keepsake:AITagSources` (`Name`, `By`) | rows of unknown AIs are left out |

The `keepsake:` namespace is `http://ns.keepsake.app/xmp/1.0/`. `xmp:MetadataDate` decides which sidecar wins when two disagree.

## Library Folder

```
library-<uuid>/
  library.json     identity
  volumes.json     volume registry (ids, labels, case/read-only flags, last mount hint)
  catalog.db       SQLite catalog (rebuildable; -wal / -shm beside it)
  history.json     backup and verify runs (not rebuildable; kept in backups)
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

Adding a folder (`add_folder` with `batch_files`) differs: it saves 20 files at a time and reads only each file's quick hash; files get a stand-in key and `fingerprint::JOB_FINGERPRINT` jobs read them in full later, on the render service's fingerprint worker (ADR-028, ADR-029), which waits while thumbnail jobs are ready (ADR-032).

Not rebuilt: renditions (regenerated on demand), import session history, and labels or tags nothing uses. A test builds a catalog, deletes it, rebuilds, and asserts the two are equal (`crates/keepsake-core/tests/rebuild_tests.rs`).

## Viewer

The viewer is a full-window overlay over the library (`components/Viewer.tsx`). It shows the focused item.

| Shows | Comes from |
|---|---|
| Photo, fitted | The 2560 px preview rendition |
| Photo at 100% | `full_image`: a full-size rendition made on demand (`render::full`), and asked for by the viewer 500 ms after a photo opens. An upright JPEG is copied into the cache as it is; others are decoded in-process, by FFmpeg (HEIF) or by LibRaw `dcraw_emu` (RAW). The viewer places and scales it with a CSS transform (no scrolling, no layout per zoom step) |
| RAW of a RAW + JPEG pair | `full_image` with `member: raw` (key R toggles) |
| Video | The proxy (the camera's H.264 SUB proxy, or Keepsake's 720p), with `video_info` for frame rate and start timecode |

Arrows move through the selection when more than one item is selected, otherwise through the whole view, selecting what they land on. Decoding runs on a blocking worker (`spawn_blocking`), never on a command thread.

## Settings and history

- `config.toml` holds `[render]` (cache folder, limit, workers, battery) and `[open.apps]` (Open in, per type). The Settings screen writes it through `settings_set`, which checks the values, reloads the open library's settings (keeping its views) and restarts the render service.
- `<library>/history.json` holds backup and verify runs (`keepsake_core::history`), from the app and the CLI. The Backup screen's destinations and "last verified" dates are read from it.
- One backup or verify runs at a time in the app, on its own thread, reporting `maint://progress` and `maint://done`.

## Helper processes end with the app

ExifTool (`-stay_open`), FFmpeg, ffprobe and dcraw are child processes (ADR-037).
- On `RunEvent::Exit` the app stops the render service (waiting up to 5 s) and the import ExifTool (`ExifTool::shutdown`).
- On Windows the app is also in a job object that ends every child when the app's process ends, however it ends (`keepsake_platform::close_helpers_with_app`).
- Programs opened for the user ("Open in", Explorer) break away from the job and stay open (`external::launch`).

## Finding the tools

`ToolPaths::discover` looks, in order:
1. `KEEPSAKE_VENDOR` (tests and overrides).
2. The folder the installed app registered with `probe::tools::set_bundled`: its resource folder, laid out like `vendor/` (`tauri.windows.conf.json`).
3. A `vendor/` folder above the current directory, then above the program (development).
4. The program's own folder.

`keepsake-app --self-check <file>` runs every tool and writes what it found (`diag::tools`).

## Reading the catalog in the app

- Read commands borrow a connection from a small pool (`Open::cat`, up to 6 idle). SQLite in WAL mode runs readers side by side, so opening a 300,000-item view never holds up a calendar level or a page of the grid.
- Writes open their own connection (`Open::writer`).

## Logs and crash reports

- `keepsake_core::logs`: one log file per run in the OS log folder (`KEEPSAKE_LOG_DIR` overrides it), the newest 10 kept, 10 MB each at most.
- A panic hook writes `crash-<time>-<pid>.txt`.
- The app logs through `tracing` to stderr and the file. UI errors arrive through `log_ui_error`.
- `diag::report` builds the diagnostic report; the home folder is replaced by `~`.
- Nothing leaves the computer.

See `docs/SAFETY.md` for every write to disk.
