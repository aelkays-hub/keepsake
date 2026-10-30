# Keepsake — Progress Log

| Prompt | Description | Status |
|--------|-------------|--------|
| 0 | Repository bootstrap: CLAUDE.md, docs, workspace scaffold, setup scripts, CI, TEST_ME.md | ✅ Done |
| 1 | Test corpus: `keepsake corpus scan/init-expected`, fixtures, test harness | ✅ Done |
| 2 (spike) | Playback Gate 1: Tauri asset protocol, proxy timing, AVCHD date, grid scroll | ✅ Done |
| 3 | Packaging: release CI (Windows x64 + ARM64 installers), OS data dirs, release.md | ✅ Done |
| 4 | Catalog: SQLite + migrations, typed repositories, XMP sidecars, rebuild, path property tests | ✅ Done (Windows CI; macOS CI still deferred) |
| 5 | Probing: ExifTool stay-open, ffprobe, Sony XML, AVCHD, grouping, capture-time confidence | ✅ Done (corpus entries still to be confirmed; no AVCHD media in corpus) |
| 6 | Ingest engine: resumable state machine, verified copies to two drives, templates, safe-to-format, fault injection | ✅ Done (Windows; macOS read-only proof via DMG pending macOS CI) |
| 7 | CLI: init, ingest, verify, search, shift-time, backup, rebuild-catalog; --json, progress, Ctrl-C | ✅ Done (Windows; macOS CI pending) |
| 8 | Device detection: Windows CM notifications + macOS DiskArbitration, card memory, `keepsake watch` with trusted auto-ingest | ✅ Done (Windows; macOS backend in CI only) |
| 9 | Renditions: persistent priority job queue, thumbnails/previews/proxies, content-addressed LRU cache, `keepsake renders`, benchmarks | ✅ Done (Windows; macOS video needs FFmpeg) |
| 10 | Main app UI: calendar, 300k-item grid, details, inspector (XMP edits), search, import window; typed Tauri API; Playwright | ✅ Done (real-app tests Windows; macOS mocked) |
| 11 | Viewer (100% zoom, LibRaw RAW, video frame stepping and timecode), batch time shift, Open in, Backup screen with history, Settings | ✅ Done (real-app tests Windows; macOS mocked) |
| 12 | Hardening and release 0.1.0: installed-app tools, scale at 300,000, data-safety audit, local logs and crash reports, accessibility | ✅ Done (release Windows only; macOS later) |
| — | Every card imports by itself into year › month › day (Settings → When a card is inserted) | ✅ Done (Windows; macOS via CI) |
| — | Sidebar tree by folder (By date / By folder switch) | ✅ Done (Windows; macOS via CI) |
| — | All metadata in the inspector: every ExifTool tag, grouped, filter, Copy, JPEG/RAW | ✅ Done (Windows; macOS via CI) |
| — | Add a folder in place: existing photos catalogued where they are, read-only (Folders screen, `keepsake add-folder`) | ✅ Done (Windows; macOS via CI) |
| — | Open in: several applications per file type, and an Open in… menu (inspector, viewer) | ✅ Done (Windows; macOS via CI) |
| — | Calendar tree in the sidebar: years, months, days with counts | ✅ Done (Windows; macOS via CI) |
| 2 | Device watcher — card/camera detection on Windows and macOS | ✅ Done in prompt 8 (MTP cameras: v1) |
| 2 | Sony card layout parser and asset grouper | ✅ Done in prompt 5 |
| 3 | Probe: ExifTool integration and Sony XML sidecar reader | ✅ Done in prompt 5 |
| 4 | Ingest engine: copy with streaming BLAKE3 and verification | ✅ Done in prompt 6 |
| 5 | Catalog: SQLite schema, migrations, WAL/FTS5 setup | ✅ Done in prompt 4 |
| 6 | XMP sidecar read/write (ratings, labels, comments, date shift) | ✅ Done in prompt 4 |
| 7 | Thumbnail and preview generation via FFmpeg/LibRaw | ✅ Done in prompts 9 and 11 (LibRaw full-size decode) |
| 8 | Tauri command layer: ingest, catalog queries, progress events | ✅ Done in prompt 10 |
| 9 | UI: calendar view | ✅ Done in prompt 10 |
| 10 | UI: grid view with TanStack Virtual | ✅ Done in prompt 10 |
| 11 | UI: details view and metadata panel | ✅ Done in prompt 10 |
| 12 | UI: ingest wizard (preview, destination, confirm) | ✅ Done in prompt 10 (import window) |
| 13 | Search: date, camera, lens, type, text (FTS5) | ✅ Done in prompts 7 and 10 |
| 14 | Video proxy generation and playback | ✅ Done in prompts 9 and 11 |
| 15 | Backup verification command (CLI + UI) | ✅ Done in prompts 7 and 11 |
| 16 | Lossless trim (keyframe-cut and smart-cut) | ⬜ Pending |
| 17 | Combine, rotate, frame grab, MP4 conversion | ⬜ Pending |
| 18 | Image adjustments (crop, resize, red-eye, auto-levels) | ⬜ Pending |
| 19 | Flickr export connector | ⬜ Pending |
| 20 | Map view (OpenStreetMap + GPX) | ⬜ Pending |
| 21 | On-device AI: tagging, face clustering, semantic search | ⬜ Pending |
| 22 | Multi-view player and time-lapse | ⬜ Pending |

---

## Every card imports by itself (2026-10-30)

### What was built
- **Core:**
  - The `{month}` template token, and `YEAR_MONTH_DAY_TEMPLATE` (`2026/October/2026-10-30/…`).
  - `settings::ImportSettings` (`[import] auto, destination, template`).
  - `destination_volume` reuses the library's volume for a folder already in it, keeping its name and read-only flag. It refuses a destination inside or around one.
- **App:**
  - When a card with new items arrives and the setting is on, the import window opens with `auto=1`. If it is already open, it gets `import://auto`.
  - Settings carry `import` (`ImportPrefs`), and saving checks a full destination path and a valid layout.
- **UI:**
  - Settings → **When a card is inserted**: ask, or copy automatically; where to; the folder layout (Year › Month › Day, Year › Day, or custom) with an example ("A photo taken today goes to E:\Pictures\2026\October\2026-10-30\A7400123.ARW").
  - In automatic mode the import window shows where items are going and starts by itself. "Nothing new" and failed checks are explained.
  - After a good import (or nothing new) it says "Copied N items to …" and closes itself in 5 s; **Keep open** stops that. After an error, a failed file or Stop, it stays open.
- ADR-027.

### Tests
- `auto_import_tests.rs` (real ExifTool and FFmpeg):
  - two dated photos land in `2026/October/2026-10-30` and `2025/December/2025-12-31` of a folder added in place (folders made);
  - no sidecar next to them, and what was there is untouched;
  - the same volume, tree and counts;
  - the card again → already imported;
  - a destination inside an added folder is refused.
- `template.rs::year_month_name_day_template`, `settings.rs::import_settings_default_off_and_round_trip`, and `prefs.rs::automatic_import_needs_a_full_destination_and_a_valid_layout`.
- `importLayouts.test.ts`.
- `auto-import.spec.ts`: on → starts by itself with only the new items; off → waits; a missing drive → stops with the reason; the Settings section.

---
## The sidebar tree by folder (2026-10-29)

### What was built
- **Core:**
  - `SearchQuery.folder` narrows to a folder of `volume` and everything below it. It matches `rel_path` in the range `folder/` to `folder0`, so `2025` never matches `2025-old`.
  - `Browse::folders(volume, parent, q)` lists a folder's sub-folders A–Z. Each has the count of matching originals in it and below, and whether it has sub-folders. Files directly in `parent` are in no sub-folder. Filters apply; empty folders drop out.
- **App:** `folder_tree(volume, parent, query)`; `Query.folder` (optional in TypeScript).
- **UI:**
  - A **By date / By folder** switch under Library, remembered in the browser storage.
  - `useFolderTree` loads a folder's sub-folders on first expand, and again when filters or the library change.
  - The roots are the volumes, with their connected dot. The Volumes list is hidden in this mode.
  - A folder is a new section kind (`folder`: volume + path). Its heading reads "Pictures › 2025 › May".
  - Both trees share the keys (→ opens or goes in, ← closes or goes up) and the twisty.
  - Pressed segmented buttons are now highlighted too (`aria-pressed`). That fixes the JPEG/RAW switch in All metadata.

### Tests
- `folder_tree_tests.rs`: counts below, nesting, filters, files at the top level, a trailing slash, the folder filter and its boundaries, no volume → error.
- `uiModel.test.ts`.
- `folder-tree.spec.ts`: switch, roots, open a year and a month, heading, count, query, filters, kept after reload, keyboard.
- The axe check covers the folder tree.

### Not done
- The tree is not revealed to a folder opened from elsewhere.
- Not measured at 300,000 items: a level scans the volume's originals' paths once.

---
## All metadata in the inspector (2026-10-29)

### What was built
- **Core:** `ExifTool::read_all(path)` runs `exiftool -j -G1 -a` and returns every tag in groups (`probe::exiftool::MetaGroup`). Groups are ordered main EXIF (IFD0, ExifIFD), GPS, maker notes (Sony…), computed values, XMP, IPTC, then the file's inner structure (IFD1, SubIFD, Sony's SR2…), then File and System. Tags are A–Z within a group. ExifTool's own version is left out. The file is only read.
- **App:** `metadata_all(id, member)` reads the file "Open in" would hand over (the JPEG of a pair, or the RAW when asked), with the app's one ExifTool process, on a blocking worker.
- **UI:** an **All metadata** section at the foot of the inspector.
  - It is collapsible, remembers whether it was open, and reads only while open.
  - Groups get names for people ("Exposure (EXIF)", "Sony maker notes"), and tag names are split into words ("Exposure Time").
  - It has a filter (name or value), Copy (all tags as text), and a JPEG/RAW switch for pairs.
  - An unplugged drive gets a note instead.
- Checked on a real ARW from the user's library: 323 tags, 0.34 s with a cold ExifTool.

### Tests
- `exiftool.rs::meta_tests` (group order, sorting, values, bad output).
- `probe_tests.rs::every_tag_of_a_photo_is_read_without_changing_it` (real ExifTool; the file's bytes are unchanged).
- `metaNames.test.ts`.
- `metadata.spec.ts`: grouped, filter, Copy, stays open, RAW switch, offline.
- The axe check now includes the open section.

---
## Calendar tree in the sidebar (2026-10-29)

### What was built
- **`useCalendarTree`** (`app/ui/src/useCalendarTree.ts`) loads the calendar's levels (years; a year's months; a month's days) with the existing `calendar` command, when each is first opened. It reloads when the filters or the library change, and drops answers for an older query.
- **Sidebar.** "Calendar" is a tree:
  - years (and "No date"), months and days, with counts;
  - a node opens its level in the main area and opens in the tree;
  - the ▸/▾ arrow only opens or closes;
  - the tree opens to the calendar view's position, and the current node is marked;
  - keyboard: ↑/↓ as before, → opens or goes to the first child, ← closes or goes to the parent;
  - sidebar buttons take the roving focus when focused (Tab, a click), not only through the arrows.
- No backend change.

### Tests
- `calendar-tree.spec.ts`: years, months and days; a day's grid and count; the arrow alone; following the calendar view; keyboard; No date.
- `keyboard.spec.ts`: the backup flow now arrows past the years.
- The axe check covers the tree.

---

## Open in: several applications and a menu (2026-10-29)

### Why
The user wants to open photos in Photoshop or Topaz Photo. Open in allowed one application per file type. ADR-026.

### What was built
- **Core:** `settings::OpenApps`: one application or a list per type (first = default); `OpenSettings::apps_for`. Old `config.toml` files load unchanged.
- **App:**
  - `open_choices` lists an item's applications, then the system default.
  - `open_external` takes the one chosen, refusing any program not set for the type.
  - Settings rows allow repeats, and a row can name several types (`arw, jpg, dng`).
- **UI:**
  - `OpenInMenu` (a menu button) in the inspector and the viewer. The default is marked Ctrl+O; Ctrl+O still opens it directly.
  - Escape closes the menu, not the viewer.

### Tests
- `settings.rs`: lists, blanks and repeats, plain string kept.
- `prefs.rs`: rows grouped in order, comma-separated types.
- `viewer.rs`: choices, and only configured apps can be picked.
- `open-in.spec.ts`: menu, keyboard, Ctrl+O, Escape in the viewer.
- The menu is in the axe check.

---

## Add a folder in place (2026-10-28)

### Why
A first-time user created a library inside `E:\Pictures\2025` (about 10,000 files, 246 GB, sorted by month, with Lightroom `.xmp` files) and saw nothing. A library filled only through a card import, which copies to another drive, and the app had no way to add a folder. ADR-025.

### What was built
- **Core: `rebuild::add_folder`.** Catalogues a folder where it is, as a read-only volume, so nothing is written into it (not even the writability probe). It reads Lightroom, digiKam and Keepsake sidecars, probes dates, camera and lens, and groups files as for a card.
  - A folder that overlaps one already in the library is refused, and so is one inside the library folder. The same folder again scans it again under the same volume.
  - It reports progress, can be stopped, and changes nothing unless it finishes.
- **Keepsake's own files are left out of every scan.** A library's `catalog.db`, `sidecars/`, `recipes/` and so on (`library::LIBRARY_ENTRIES`), wherever such a library sits. In the cache folder, only Keepsake's own sub-folders (`thumbnail`, `preview`, `proxy`, …): photos in a folder that is also the cache folder are still added. Found when a user's cache was set to `E:\Pictures\2026`: the first version skipped that whole folder.
- **`RebuildOptions`**: prober, cache folders, a stop flag and progress (`Listed`, `File`, `Phase`). `rebuild_catalog_with` and `rebuild_library` keep their signatures.
- **App.**
  - `folder_add` runs as the maintenance job (one of backup, verify or folder scan at a time). It sends `maint://progress` (kind `folder`), then `folder://done`, and queues thumbnails.
  - A **Folders** screen (Tools): add with Browse… or by typing, progress and Stop, the outcome with "Show them", and every folder or drive in the library with "Scan again".
  - The empty library offers "Add a folder…".
  - The grid and sidebar refresh when a scan finishes.
- **CLI: `keepsake add-folder <folder> [--no-probe]`**, with a progress bar and Ctrl-C.

### Tests
- `add_folder_tests.rs` (8 tests):
  - the folder is never written, including when write-locked at the OS level;
  - Lightroom ratings are read;
  - a library and cache inside the folder are left out, and stay left out on a rescan;
  - photos in a folder that is also the cache folder are added, its thumbnails are not;
  - a rescan finds new and removed files;
  - overlapping folders are refused;
  - a stopped scan leaves the library unchanged;
  - progress counts.
- `cli_tests.rs::add_folder_catalogues_in_place_and_writes_nothing_into_the_folder`.
- `folders.spec.ts` (empty library → add → sidebar, Scan again, Show them; a missing folder; keyboard only), and the Folders screen in the axe check (both themes).

### Not done
- Added folders are not watched for changes ("Scan again" is manual), and a folder cannot be removed from the library yet.
- A rescan hashes every file again.
- No real-app (WebView2) test of the Folders screen. The mocked UI tests and the core and CLI tests cover it.

---

## Prompt 12 — Hardening and release 0.1.0 (2026-10-26)

### What was built

**The installed app works** (found before planning)
- An installed copy looked for its tools only in a `vendor/` folder beside the source, so it found none: no metadata, thumbnails, proxies or full-size pictures.
- The installer now lays its resources out like `vendor/` (`tauri.windows.conf.json`), and the app registers that folder with core at start-up.
- `keepsake-app --self-check <file>` finds and runs every tool. The release smoke test runs it on the installed app, and also runs each installed tool.
- LibRaw and the FFmpeg licence are bundled.
- The release jobs use the setup script (`-Arch ARM64` for the cross build) instead of their own copies of the download code.
- The release is created as a **draft**.
- The LibRaw check looked for "libraw" in the program's *path*, so it could never fail. It now checks the program's own banner.

**Scale: 300,000 items** (`docs/PERFORMANCE.md`)
- A core scale test (`scale_tests.rs`, ignored by default, run in release; CI runs it with doubled limits) and a real-app test (`perf.spec.ts`).
- The seeder adds keywords and comments, so text search is realistic.
- Fixed:

  | Problem | Before | After |
  |---|---:|---:|
  | Sidebar per-volume count (partial index, schema v6) | 1.9 s | 31 ms |
  | Calendar with a word filter (counts and covers in one pass, not one search per cell) | 1.8 s | 63 ms |

- Also fixed:
  - the camera and lens lists were fetched with every new view (200 ms), which held up the next search;
  - all reads shared one connection behind a lock, so opening a 300,000-item view held up the calendar. Reads now use a small connection pool;
  - the years calendar uses indexes (170 ms → 65 ms); browsing word filters are `seq IN (matches)`;
  - the typing pause is now 80 ms (was 250), and a new view opens at once (was after 120 ms).
- In the release app at 300,000: start-up 0.98 s, search 109–233 ms from the last keystroke, calendar 93–137 ms, scrolling with no dropped frames (details in PERFORMANCE.md).

**Data safety** (`docs/SAFETY.md`)
- Every code path that writes to disk, with what, where and its test: 58 functions.
- `safety_audit_tests.rs` scans every crate's code and fails on a write that is not listed, or on a named test that does not exist.
- `source_safety_tests.rs` locks a card and a read-only library volume at the OS level (deny ACL / `chmod a-w`) and runs every path that reads them. Afterwards every file is byte-for-byte unchanged.
- New tests for recipes, a backup's stale temporaries, the corpus CLI and `--self-check`.

**Logs and crash reports, local only**
- One log file per run in the OS log folder (the newest 10, 10 MB cap).
- Panics in the app or the CLI write `crash-….txt` with a backtrace. The app shows a one-time notice at the next start.
- UI errors are logged; a render failure shows an error screen with Reload and Copy diagnostic report.
- Settings → **Copy diagnostic report** (version, OS, tools, library counts, settings, last crash, last 200 log lines; the home folder shown as `~`) and **Open log folder**.
- No telemetry; nothing is sent.

**Accessibility**
- Roving focus in the grid and details: the focused item has keyboard focus and a spoken description ("DSC01234.ARW, RAW photo, 2024-06-15 14:02, 4 stars, Red label").
- Dialogs trap Tab and return focus when they close.
- Contrast fixes in both themes:
  - the accent and warning colours, with separate fill colours for white text;
  - grey text on selected rows.
- Table structure fixed.
- Bugs found by the keyboard tests:
  - one Enter in the calendar opened two levels;
  - focus was lost when moving between calendar levels and after End in a large grid.
- Tests: `keyboard.spec.ts` (every MVP flow without a mouse) and `a11y.spec.ts` (axe, WCAG 2.1 AA, every screen, light and dark).

**Release 0.1.0**
- `docs/RELEASE_NOTES.md` is used as the release body.
- ADR-024 records the decisions and the new test-only dependency `@axe-core/playwright` 4.13.0 (MPL-2.0).

### Found and not fixed
- Re-importing to new drives a card whose photos are byte-for-byte identical under different names reports "not safe to format" (a duplicate-file check while cataloguing). Keepsake errs on the safe side and no data is at risk. Real cameras do not write identical files under different names.

### Not done / next
- macOS release: it needs a mirrored LGPL FFmpeg for macOS (ADR-005).
- Code signing: there is no certificate yet.

---

## Prompt 11 — Viewer, time shift, Open in, Backup and Settings (2026-10-25)

### What was built

**Viewer** (Enter or double-click; Esc closes)
- Photos fit the window from the preview. Z, a click or the mouse wheel zooms to **100%** from a full-size picture made on demand: JPEG/PNG/TIFF decoded in-process, HEIF by FFmpeg, **RAW by LibRaw** (`dcraw_emu` sidecar, ADR-023). Drag or arrows pan.
- **RAW + JPEG pairs** are one item with a JPEG | RAW toggle (R).
- **Video** plays the proxy, with a scrub bar, Space play/pause, `,`/`.` one frame back/forward, J/L one second, and the **timecode** of the frame on screen (start timecode from ffprobe plus position, drop-frame aware).
- Arrows move through the selection when several items are selected, otherwise through the whole view. Rating (0–5) and label (6–9) keys, F full screen, Ctrl+O Open in.
- Focus returns to the grid on close.

**Batch time shift** (Ctrl+T, or "Shift time…" in the inspector)
- Pick a reference photo from the selection (the focused one by default), enter the time it really was, and see the offset and the first 10 items before → after, with the total and how many have no date.
- Apply writes the catalog and the XMP sidecars only, in the background for large selections.

**Open in** (Ctrl+O): the system default, or the program set per file type in Settings (`[open.apps]` in `config.toml`). No new dependency.

**Backup screen** (sidebar → Backup)
- Choose a folder or drive and run an incremental, verified backup with progress and Stop.
- Each destination shows its last backup and **last verification** date, with Back up now and Verify now.
- A history of every backup and verify, kept in `<library>/history.json` (survives a catalog rebuild; copied into backups). The CLI's `backup` and `verify` record there too.

**Settings screen** (sidebar → Settings, or Ctrl+,)
- Library: open another or create a new one.
- Cache folder (default or chosen), with **Remove old files** after a move; size limit; image and video workers; pause on battery.
- Open in applications per type.
- Import presets: create, edit, delete.
- Saved to `config.toml`; rendering restarts with the new values.

**Core**
- `render::full`: `full_image`, `members` and `video_info`; `Variant::Full`/`FullRaw`; `RenderTools.dcraw`; `Source.raw`.
- `select::shift_for` (whole seconds), `history`, `external`, `OpenSettings`, `Cache::remove_files`.

**App commands:** `full_image`, `asset_members`, `video_info`, `open_external`, `time_shift_preview`, `time_shift_apply`, `backup_overview`, `backup_run`, `verify_run`, `maint_cancel`, `settings_get`, `settings_set`, `cache_remove_old`, `preset_delete`. Events: `maint://progress`, `maint://done`.

**Setup:** the setup scripts download LibRaw 0.22.2 (mirrored, SHA-256 checked) into `vendor/libraw`.

**Tests**
- Core:
  - full-size pictures (sizes, orientation, made once, originals untouched);
  - RAW pairs and the toggle;
  - `video_info` with a 29.97 drop-frame timecode;
  - `shift_for`, including sub-second times;
  - history, Open in commands, settings;
  - removing an old cache;
  - a corpus test decoding a real ARW at full size with LibRaw.
- CLI: backup and verify are recorded in the history and the history is in the backup.
- UI unit (14): drop-frame timecode, frame at a time, offset text, viewer navigation.
- Playwright **mocked** (23, 9 new): viewer navigation through the view and the selection, 100% zoom and pan, the JPEG | RAW toggle, video frame stepping and timecode (a 26 KB synthetic WebM), rating and Open in from the viewer, time shift, backup and verify with history, settings and presets.
- Playwright **real app**, Windows (8, 4 new):
  - a real JPEG at 100% and frame stepping on a real proxy;
  - time shift checked in the XMP files, with the originals byte-for-byte unchanged;
  - a real backup and verify, with `history.json`;
  - Open in, through a test program that records what it was given;
  - settings surviving a restart.

### Bug found and fixed
- A capture time with fractions of a second made the time-shift offset one second off. The offset now compares whole seconds, as times are shown.

### Not done / next
- The viewer has no slideshow, compare view or rotation.
- The HEIF full-size path has no automated test: FFmpeg cannot write HEIF files to make one, and the corpus has none.
- Windows ARM64 runs LibRaw's x64 build under emulation.
- On macOS the UI is tested only against the mocked backend.

---

## Prompt 10 — The main app UI (2026-10-25)

### What was built

**The app** (`app/ui`): a sidebar (Library, Calendar, Events, Volumes, Imports), a main area, an inspector and a status bar. Light and dark themes follow the OS, and everything works from the keyboard.
- **Calendar:** years, then months, then days, with counts and cover thumbnails. A day opens its grid; Backspace goes up a level.
- **Grid:** virtualized, smooth at 300,000 items.
  - Multi-select is stored as index ranges, so select-all is instant.
  - Mouse: Shift/Ctrl-click. Keyboard: arrows, PgUp/PgDn, Home/End.
  - Ratings, labels, video durations, RAW and **offline** badges; offline items keep their cached thumbnails.
- **Details:** a virtualized table with sortable columns (name, date, camera, lens, type, size, duration, rating).
- **Inspector:** metadata, rating (keys 1–5), colour label (keys 6–9), comment and keywords, written to XMP through core. Rating, label and keywords apply to the selection; large selections run in the background with progress.
- **Search bar:** words, date range, camera, lens, type (photo/RAW/video), minimum rating.
- **Import window:** opened by the device watcher when a Sony card is inserted.
  - New items grouped by day, one tile per asset, with live thumbnails made from the card (read only).
  - Select all, none or new only; presets (and save as preset); destination and template; a second-destination toggle.
  - A pre-check: space, the same-disk rule, an example name.
  - Progress with Stop, and the SAFE TO FORMAT result.
- **First run:** create or open a library, with a native folder picker.

**Typed bridge** (ADR-022):
- `src-tauri/src/api.rs` defines every command and event type; `ts-rs` generates `app/ui/src/bindings/`.
- Commands: library, views, calendar, sidebar, details, edits, renders, import. Events: `render://thumbs`, `device://card`, `edit://progress`, `import://progress`, `import://done`.
- The asset protocol may load only the rendition cache.

**Core**
- `Browse` repository: count, `ordered_ids` (view snapshots), `rows`, `ids_range`, `index_of`, calendar buckets with covers, facets, per-volume/event/import counts.
- `SearchQuery`: lens, minimum rating, RAW, label, event, volume, import, undated.
- Catalog schema v5: name, size and RAW copied onto assets by triggers, plus sort and filter indexes.
- `ingest::scan_card`, `check_destinations_for`, `example_name`; `render::card` (card thumbnails).
- `seed` (hidden `keepsake dev seed`).
- Faster inserts: cached statements in the text index and the inserts.
- The backlog is queued in chunks of 2,000.
- Cache stats are two indexed queries.

**Performance at 300,000 items** (release core):

| Operation | Time |
|---|---|
| Count | 8 ms |
| Open a view | 0.16–0.27 s |
| Any page | 4–6 ms |
| Years calendar | 0.19 s |
| Seed | about 70 s |

In the debug app's UI test, End reaches the last tile with its thumbnail in about 0.2 s.

**Tests**
- Core: `browse_tests` (10, plus 2 timing probes ignored), and a card-thumbnail render test.
- UI unit tests (Vitest, 8): selection ranges, keyboard map, formatting.
- Playwright **mocked** (14): first run, 300,000-item virtualization, multi-select and rating, keyboard rating and label, comment and keywords, calendar drill-down, search filters, details sort, offline badges, sidebar and view keys, themes, and the import window (grouping, select all/none, presets, progress, SAFE TO FORMAT, one destination, same disk).
- Playwright **real app**, Windows (4): first run; the library with real thumbnails, keyboard rating checked **in the XMP file**, filters, details sort, calendar and offline; card insert → import window → SAFE TO FORMAT; 300,000 items.

**CI.** Windows runs the unit, mocked and real-app suites and checks that the bindings are up to date. macOS runs the unit and mocked suites.

### Also changed
- Dev builds optimise dependencies (SQLite, image, BLAKE3).
- The spike page was removed (its results are in `docs/spikes/playback.md`).

### Not done / next
- No viewer or player yet: Enter opens the inspector.
- Events cannot be created from the UI yet.
- On macOS the UI is tested only against the mocked backend.

---

## Prompt 9 — Thumbnails, previews and proxies (2026-10-24)

### What was built

**Job queue** (`keepsake_core::jobs`, catalog schema v4 `jobs` table)
- Persistent and generic over `kind`.
- Order: visible on screen, then recent import, then backlog; within a class, cheaper kinds first, then newest capture.
- Retries with backoff (30 s, then 60 s), then failed. Offline media is deferred without counting an attempt.
- Running jobs are recovered after a restart.
- Workers pause on battery and while an ingest runs: ingest keeps a heartbeat in the catalog, which works across processes.

**Renditions** (`keepsake_core::render`, ADR-021)
- Thumbnails at 256 and 1024 px and a 2560 px preview, made from one decode, never enlarged, orientation baked in, JPEG q82.
- JPEG/PNG/TIFF decode in-process (`image`, `fast_image_resize`).
- ARW/DNG use the largest embedded JPEG, found with ExifTool offsets.
- HEIF and video poster frames (10 % in) come from the FFmpeg sidecar.
- 720p H.264 proxies use the spike settings, plus an explicit one-second GOP (checked by tests), yadif for interlaced sources, and AVCHD segments joined. Encoder order: NVENC → QSV → AMF → Media Foundation → OpenH264 (VideoToolbox on macOS).
- The camera's SUB proxy is copied into the cache when it is H.264; an HEVC SUB proxy is re-encoded.

**Cache**
- Content-addressed: `<kind>/<aa>/<source BLAKE3>-<recipe hash>`.
- Configurable folder and size limit (`config.toml [render]`, now shared by the CLI and the app via `keepsake_core::settings`).
- LRU eviction that never removes renditions of offline media.

**Integration**
- `ingest` queues what it catalogues as recent imports.
- `keepsake-platform::power`: on battery or not (Windows and macOS).
- CLI: `keepsake renders status | run | enqueue | show | retry | evict | bench`.
- Tauri: `render_status`, `render_set_visible`, `render_lookup`, `render_start`, `render_stop`, plus `render://progress` events.

**Benchmarks**: `docs/PERFORMANCE.md`. On a Ryzen 7 7735HS with 4 workers:

| Type | Items/min |
|---|---:|
| JPEG | 700 |
| ARW | 1,194 |
| PNG/TIFF | 672 |
| Video posters | 295 |

Proxies run at 9.0× real time with AMF, 8.6× with `h264_mf` and 4.0× with OpenH264.

**Tests**
- 18 render tests, all passing, covering:
  - queue order, visibility, backoff, deferral, restart;
  - the ingest heartbeat;
  - cache keys, and eviction that keeps offline renditions;
  - stills from JPEG/PNG/16-bit TIFF, including EXIF rotation;
  - RAW + JPEG pairs;
  - video posters, including rotated ones;
  - proxies from H.264, HEVC and small sources: 720p H.264 + AAC, GOP ≤ 1 s, with the detected encoder and with OpenH264;
  - an H.264 SUB proxy copied byte for byte, and an HEVC one re-encoded;
  - AVCHD segment joins;
  - the service end to end: pauses, cancel, offline, re-render after deletion;
  - ingest queuing;
  - originals unchanged byte for byte.
- One corpus test (`#[ignore]`) on real ARW, JPEG and AVCHD M2TS (AC-3 is transcoded to AAC).
- 3 CLI tests for `renders`.

### Also changed
- Minimum Rust raised to 1.90. The old 1.77 was already untrue, because of `uuid` and Tauri. Newer clippy suggestions were applied to older code.
- ADR-004 updated: the proxy is 720p (proven in the spike), and camera proxies are used only if they are H.264, copied into the cache.

### Not done / next
- Sony ARW embedded previews are 1616 px. A full 2560 px preview needs LibRaw (in the stack, not added yet).
- HEIF is untested on real files; please add a few `.HIF` files to the corpus.
- macOS benchmarks need the Mac. macOS video renders need an LGPL FFmpeg for macOS (ADR-005).
- No UI yet. The grid will call `render_set_visible` and `render_lookup`.

---

## Prompt 8 — Device detection (2026-10-24)

### What was built

**`keepsake-platform`** (hand-written OS bindings, no new dependencies; ADR-020)
- `DeviceEvents` trait: `Arrived { mount, identity }` and `Removed { mount }`. Volumes already mounted at start arrive first.
- Windows backend:
  - `CM_Register_Notification` on the volume interface class wakes a scanner thread;
  - the thread lists drive letters, with identity from `GetVolumeInformationW` (serial, file system, label), the volume GUID, capacity, and `IOCTL_STORAGE_QUERY_PROPERTY` (model, serial, bus).
- macOS backend:
  - DiskArbitration callbacks on the scanner's run loop;
  - the thread lists `/Volumes`, with identity from the disk description.
  - Type-checked on Windows; compiled and tested only by the new macOS CI job.
- Both backends list again every 0.5 s for 6 s after a notification, and every 3 s otherwise. This catches card swaps and missed notifications.
- `FakeDevices`: scripted events, also from a JSON Lines file.

**`keepsake-core`**
- `devices`: the event and identity types. A card's key is its file-system type, serial and capacity.
- `cards`:
  - Sony layout recognition (`PRIVATE/M4ROOT`, `PRIVATE/AVCHD`, `MP_ROOT`, `DCIM/nnnMSDCF`);
  - card records in `<library>/cards.json`;
  - imported-item lists in `cards/<id>.json`, giving "N new items since last import" from a directory listing;
  - trust and preset per card.
- `presets`: named destinations, template and event, in `presets.json`.
- `watch::Watcher`: device events become card events. With auto-ingest, trusted cards are imported with their preset, one at a time, through the normal ingest path. It reports why it skipped (no preset, nothing new, destination missing), and it handles Ctrl-C and a card pulled mid-import.
- Ingest registers a card's source volume by its key, so a new drive letter is the same card.
- Backups include the card memory and presets.

**CLI**
- `keepsake watch [--auto-ingest]`: text, or JSON Lines with `--json`.
- `keepsake card list | trust | untrust | rename | forget`
- `keepsake preset save | list | delete`
- `ingest` remembers the card it imports from.

**Tests**
- Core: 12 watcher and card tests. They cover:
  - recognition;
  - a known card in another slot;
  - a reformatted card;
  - untrusted cards and the skip reasons;
  - import, then nothing new;
  - a card pulled mid-import;
  - Ctrl-C, then resume;
  - forget.
- Platform:
  - the scanner diff, the fake backend driving the watcher, and JSONL scripts;
  - the real backend listing this machine's volumes and stopping within 2 s;
  - `CM_NOTIFY_FILTER` layout and the system drive's identity on Windows.
- CLI: 3 tests drive `watch --fake-events` end to end, plus `card` and `preset`.

**CI**: a new `macos-14` job runs clippy and the tests for the workspace except the Tauri app. Media-tool tests skip there (`KEEPSAKE_SKIP_MEDIA_TOOLS`).

### Not done / next
- MTP-only cameras (ADR-002, v1).
- Windows volumes mounted in folders rather than drive letters are not watched.
- A card has different keys on Windows and macOS.
- The Tauri app does not run the watcher yet. It can read `watch --json` or call `keepsake_core::watch` directly.

---

## Prompt 7 — CLI (2026-10-24)

### What was built

**`keepsake-cli`** is now a real tool. Every command has text output and `--json` (manual: `docs/CLI.md`).
- `init <path>`: creates or adopts a library and makes it the default in `config.toml`.
- `ingest <source> --dest A [--dest B] [--template] [--event] [--dry-run]`, plus `--resume`, `--status`, `--list`. A dry run writes nothing.
- `verify [--volume X]`: re-hashes past the OS cache and reports missing, changed and unreadable files.
- `search "<words>" [--from] [--to] [--camera] [--type]`.
- `shift-time <paths…> --by ±H:MM[:SS]` (or filters), plus `--dry-run`. It writes XMP sidecars and the catalog only.
- `backup <dest>`: incremental and verified, refuses to copy files that no longer match the catalog, and writes a library snapshot to `<dest>/.keepsake-library/`.
- `rebuild-catalog [--fresh]`.
- `probe <folder>`.
- Library selection: `--library`, then `KEEPSAKE_LIBRARY`, then the `init` default.
- Exit codes: 0 OK, 1 error, 2 finished with problems, 130 cancelled.
- Progress bars (`indicatif`) on stderr; terminals only, never with `--json`.
- Ctrl-C (`ctrlc`): the first press finishes the current file and stops cleanly; the second exits at once.

**`keepsake-core` additions**
- `catalog::SearchQuery` and `Search::query`: words (FTS5), capture-day range, camera, kind.
- `Catalog::snapshot_to` (`VACUUM INTO`).
- `ingest::preview` and `preview_destination`: a dry run sharing the planning code.
- `verify` module.
- `backup` module.
- `select` module: resolve paths/filters to assets, and parse the shift offset.
- Ingest cancel now happens between files only. `copy::tee` has no cancel argument.
- `rebuild_library` skips card volumes. This fixes a bug the CLI exposed: the source card was being catalogued.

**Tests**
- `crates/keepsake-cli/tests/cli_tests.rs`: 12 integration tests that run the real binary against the fixture card, covering every command.
- The Ctrl-C test sends a real Ctrl-Break (Windows) or SIGINT (macOS), then checks that it exited with 130, that every final file is complete, that no temporaries are left, and that resume reaches "safe to format".
- Core: `search_query_filters_combine`; the ingest cancel test now asserts that the in-flight file is completed.

**Docs**
- `docs/CLI.md` (new).
- ADR-018 (indicatif MIT, ctrlc MIT/Apache-2.0) and ADR-019 (CLI behaviour).
- SPEC and ARCHITECTURE updated.

### Removed
The `keepsake library …` group and the `ingest start|resume|status|list` subcommands; they are replaced by the commands above.

### Not done / next
- macOS CI is still deferred, so the macOS Ctrl-C path has only been run in CI on Windows.
- `backup` copies whole assets from one volume; it does not yet restore a library from a backup (a manual copy of `.keepsake-library` works).

---

## Prompt 6 — Ingest engine (2026-10-23)

### What was built

**`keepsake_core::ingest`**
- `plan`:
  - groups and probes the card, then quick-diffs each asset (size + xxh3 of the first and last MiB) against the library: new, already imported, or modified;
  - names every file on every destination with the template engine;
  - resolves collisions deterministically;
  - stores the session, its assets and its file items.
- `select`: choose or drop assets before copying.
- `run`:
  1. recovers anything left by an interrupted run;
  2. checks free space;
  3. per file, reads the source once and writes all destinations in parallel to hidden temporary files;
  4. flushes them, re-reads them past the OS cache, compares, and renames;
  5. re-reads every final file;
  6. catalogues (probing the destination copies, sidecars on both drives, sequences);
  7. confirms already-imported assets with full BLAKE3;
  8. stores "safe to format".
- `report`, `leftovers`, `destination_volume`.
- `SourceVolume`: read-only by construction. Destinations inside the source, overlapping, or on the same disk are refused.
- `ingest::template`: `{yyyy} {mm} {dd} {hh} {event} {camera} {lens} {seq} {orig}`; Windows/macOS-safe names; companions follow their original.
- `ingest::fault::FaultPlan`: test-only fault and crash injection, passed in explicitly.

**Catalog v3 (migration 0003)**: `import_assets` (state machine), `import_items` rebuilt (role, segment, size, quick hash, final path, stream and verified hashes, statuses `copying`/`reused`), `template` and `safe_to_format` on sessions. Rebuild now records quick hashes.

**CLI**: `keepsake ingest start <library> <card> --to A [--to B] [--template …] [--event …] [--all] [--no-probe]`, `ingest resume`, `ingest status`, `ingest list`.

**Dependency**: `twox-hash` 2 (MIT), ADR-016.

### Tests (`ingest_tests.rs`, 24 + 1 child)
- Flows:
  - two destinations: safe to format, card unchanged, companions follow, sidecars on both drives;
  - one destination: never safe;
  - second import finds everything already imported (confirmed by BLAKE3, nothing copied);
  - unselected assets are reported;
  - collisions get ` (2)` and identical files are reused;
  - templates incl. `{seq}` and `{event}`;
  - probing end to end (dated folders, capture source, GPS, burst).
- Refusals: destination inside the card; same disk; three destinations; missing card.
- Faults, each followed by a clean resume:
  - card removed mid-copy; file changing while read;
  - destination full; **real** permission denied (ACL);
  - corrupted write (retried); persistent read-back mismatch (never renamed); final failing verification (moved aside);
  - rename busy (retried / failed); catalog failure;
  - cancel;
  - simulated crashes mid-copy, after rename and after verify; tampered final after a crash.
- **Real process kill**: the test re-runs itself as a child, which `abort()`s mid-copy, after a rename or after verify. The parent checks nothing passes as complete, then resumes.
- **Source made read-only by the OS** (Windows ACL; `chmod` on Unix): writes, renames, deletes and new folders are proven to fail; two full ingests succeed; bytes, sizes and modification times are unchanged.

Verified by hand on a 1.9 GB copy of the ILCE-7M4 card: 47 assets safe to format in 19 s. Killing the CLI after 3 s left two hidden temporaries and "NOT safe to format"; `ingest resume` finished, and the result was byte-identical to an uninterrupted run.

### Changes from the reviewed plan (ADR-017)
`import_items` also has `segment_index` and a `UNIQUE (dest_volume_id, dest_rel_path, session_id)` guard. Verification re-reads both the temporary file (before the rename) and the final file (after it).

### Not done / known limits
- macOS: re-reads go through the OS cache (no `F_NOCACHE` without `libc`); the read-only proof should use a read-only DMG; macOS CI still deferred.
- Rename-no-replace is "check, then rename"; a no-replace system call is a follow-up.
- No job queue yet: `run` is blocking (worker-thread ready). Ctrl+C in the CLI is the same as a kill; `ingest resume` recovers.
- Renditions (pipeline step 8) are not queued yet.

### Files changed
- `crates/keepsake-core/src/ingest/{mod,copy,source,template,fault,sys}.rs` — NEW
- `crates/keepsake-core/src/catalog/migrations/0003_ingest.sql`, `migrations.rs`, `model.rs`, `repo/{imports,files}.rs`
- `crates/keepsake-core/src/rebuild.rs` (quick hash), `lib.rs`
- `crates/keepsake-core/tests/ingest_tests.rs`, `tests/common/readonly.rs` — NEW; `tests/catalog_tests.rs`, `tests/common/mod.rs`
- `crates/keepsake-cli/src/commands/ingest.rs` — NEW; `main.rs`, `commands/mod.rs`, `Cargo.toml`
- `Cargo.toml`, `crates/keepsake-core/Cargo.toml` — `twox-hash`
- `docs/ARCHITECTURE.md`, `docs/DECISIONS.md` (ADR-016, ADR-017), `docs/SPEC.md`, `TEST_ME.md`

---

## Prompt 5 — Probing (2026-10-23)

### What was built

**Tools (`keepsake_core::probe`)**
- `ExifTool`: one `-stay_open` process on a worker thread; queued requests from any thread; per-request timeout; kill-and-restart on timeout or crash; UTF-8 file names; large-file support.
- `ffprobe`: codec/profile, size, fps, duration, bit depth + chroma (from `pix_fmt`), clockwise rotation, timecode, container creation time.
- `ToolPaths::discover()` finds the vendored tools; `scripts/setup.ps1 -VendorOnly` downloads only them (used by CI).

**Parsers**
- `sony_xml`: `NonRealTimeMeta` creation date with offset, frame-accurate duration (format fps, so S&Q is right), LTC timecode with drop-frame flag, make/model/serial, lens, recording mode, GPS (`ExifGPS`).
- `avchd`: `MPLS` playlists → recordings (seamless PlayItems joined), `HDMV` clip info, MDPM date scanner; byte builders in `avchd::synth` for tests.

**Grouping (`keepsake_core::grouping`)** replaces the old path-stem grouper and rebuild's role logic: XAVC + XML + SUB proxy + THMBNL thumbnail; JPG > HEIF > RAW stills; AVCHD recordings (`segment` role); card bookkeeping files listed separately. `grouping::sequences` flags bursts, brackets and intervals.

**Capture time (`probe::capture`)**: documented precedence, UTC offset handling, offset borrowing from same-camera neighbours, offset derived from local clock + UTC container, conflict detection, confidence high/medium/low.

**Catalog v2 (migration 0002)**: `capture_source`, `capture_confidence`, GPS on assets; `segment` role and `segment_index` on files (table rebuilt with foreign keys off, then checked); `sequences` and `asset_sequences`. Sidecars carry the new fields.

**Rebuild** probes assets without a Keepsake sidecar (`rebuild_catalog_with(.., Some(&prober))`); the CLI does this by default (`--no-probe` to skip).

**CLI**: `keepsake probe <folder> [--json]`; `keepsake corpus init-expected --probe` pre-fills unconfirmed entries (`prefilled = true`).

**Corpus**: `corpus/expected.toml` regenerated with the new grouping (48 assets: 45 stills, 3 XAVC clips; card files no longer listed) and pre-filled from the probe. **No entry is confirmed yet.**

**Tests**
- Unit: ffprobe parsing, pixel formats, rotation; Sony LTC, fps strings, DMS; MPL/CPI/MDPM; recordings; capture precedence, conflicts, borrowing, EXIF dates; grouping layouts; sequences; v1→v2 migration keeps rows and foreign keys.
- `probe_tests.rs` (12): Sony XML fixtures (`tests/fixtures/sony_xml/`); ExifTool on Unicode/space paths, concurrency, missing files, timeout-then-restart (with a hanging stand-in); ffprobe on generated H.264 (rotation, timecode, creation time) and 10-bit 4:2:2 ProRes; an end-to-end generated card (two XAVC clips incl. a 1-hour conflict, a timing burst, a borrowed offset, a two-segment AVCHD recording with MDPM); rebuild with probing is stable.
- `corpus_tests.rs`: `corpus_matches_every_confirmed_entry` (exact grouping, capture time incl. offset, model, duration; **fails while nothing is confirmed**) and `corpus_probes_cleanly` (passes on the ILCE-7M4 card).

### Verified on the real corpus (ILCE-7M4)
All 45 stills: EXIF time with sub-seconds and −05:00, high confidence. 3 XAVC clips: XML time, high; durations from frames (9509 / 13513 / 6506 ms); timecode continues across clips. Sequences: bursts of 18 and 5, five 3-frame brackets.

### Not done / known limits
- Corpus entries must be checked against the camera and set `confirmed = true`; until then the confirmed-entry test fails by design.
- No AVCHD, HEIF, SUB-proxy or GPS media in the corpus: those paths are tested with fixtures and generated files only.
- Sony XML GPS layout follows ExifTool's documentation, unverified on a real clip.
- macOS CI still deferred.
- ADR-005 corrected: the vendored x64 FFmpeg is LGPLv3 (`--enable-version3`), not LGPL-2.1.

### Files changed
- `crates/keepsake-core/src/probe/**` — NEW: exiftool, ffprobe, sony_xml, avchd, capture, asset prober, tools
- `crates/keepsake-core/src/grouping.rs`, `grouping/sequences.rs` — NEW
- `crates/keepsake-core/src/catalog/migrations/0002_probe.sql`, `migrations.rs`, `model.rs`, `ids.rs`, `repo/{assets,files,sequences}.rs`, `snapshot.rs` — schema v2
- `crates/keepsake-core/src/rebuild.rs`, `metadata.rs`, `xmp/mod.rs`, `corpus/{grouper,probed,types}.rs`
- `crates/keepsake-cli/src/commands/{probe,library,corpus}.rs`, `main.rs`
- `crates/keepsake-core/tests/{probe_tests,corpus_tests,rebuild_tests,fixture_tests,xmp_tests}.rs`, `tests/common/media.rs`
- `tests/fixtures/sony_xml/*.xml` — NEW; `tests/fixtures/xavc/**/C000?M01.XML`, `scripts/make-fixtures.*` — frame-count durations
- `scripts/setup.ps1` (`-VendorOnly`), `.github/workflows/ci.yml` (real binaries, cached)
- `corpus/expected.toml` — regenerated and pre-filled
- `docs/ARCHITECTURE.md`, `docs/DECISIONS.md` (ADR-005 fix, ADR-014, ADR-015), `docs/SPEC.md`, `TEST_ME.md`

---

## Prompt 4 — Catalog (2026-10-23)

### What was built

**Catalog (`keepsake_core::catalog`)**
- SQLite in WAL mode, `foreign_keys=ON`, `busy_timeout=5s`. Schema v1 in `catalog/migrations/0001_init.sql`, tracked with `PRAGMA user_version`; a newer catalog is refused.
- Tables: `library_meta`, `volumes`, `events`, `labels` (Lightroom's five colours seeded), `assets`, `files`, `xmp_sidecars`, `tags`, `asset_tags`, `edit_recipes`, `renditions`, `import_sessions`, `import_destinations`, `import_items`, FTS5 `asset_fts`, view `event_spans`. All `STRICT`.
- `assets.capture_local_date` is a generated column for the calendar view.
- Typed repositories on `Catalog::read` / `Catalog::write` transactions; write methods only exist on write transactions. No SQL outside `catalog/`.
- FTS5 index over file names, comment and tags (with ancestors); maintained in the same transaction; `tx.fts_mismatches()` self-check. User search text is always quoted (no FTS syntax injection).
- `tx.snapshot()`: canonical dump used to compare catalogs.

**Paths (`keepsake_core::relpath`)**
- `RelPath`: `/`-separated, accepts `\`, rejects `..`, absolute, drive and UNC paths. Keeps the exact on-disk spelling; `key()` gives NFC (+ lowercase on case-insensitive volumes) for identity.

**XMP (`keepsake_core::xmp`)**
- Namespace-aware XML tree that preserves foreign properties, comments and the xpacket wrapper.
- Lightroom/digiKam mapping: `xmp:Rating`, `xmp:Label` + `digiKam:ColorLabel`, `dc:description`, `dc:subject` + `lr:hierarchicalSubject` + `digiKam:TagsList`, `exif:DateTimeOriginal`, plus `keepsake:*` for asset id, time-shift, event, AI tags, recipes and member files.
- Sidecars at `<name>.<ext>.xmp`; read-only or offline volumes use `<library>/sidecars/<volume-uuid>/`. Atomic writes.

**Library, metadata and rebuild**
- `Library`: `library.json`, `volumes.json` registry, `recipes/`, `sidecars/`.
- `MetadataService`: rating, label, comment, user/AI tags, capture-time shift, events, recipes. Each edit updates rows and sidecars in one transaction.
- `rebuild_catalog(cat, lib, volume, mount)`, `rebuild_library`, `add_volume`.

**CLI**: `keepsake library init | add-volume | rebuild [--fresh] | stats | search | edit`.

**Tests** (all run in CI, none need the corpus):
- `catalog_tests.rs` (15): WAL, migrations, rollback, NFC/NFD and case duplicates, labels, tags, search, events, renditions, imports.
- `xmp_tests.rs` (10): full round-trip, Lightroom and digiKam fixtures (`tests/fixtures/xmp/`), foreign-field preservation, prefix clashes, atomic write, ExifTool cross-check (skipped when `vendor/exiftool` is absent).
- `rebuild_tests.rs` (9): build from fixtures → delete → rebuild, checked against independently written expected values (assets, file grouping and roles, BLAKE3 against disk, capture time with UTC offset and shift, ratings, labels, comments, keywords, AI tags) plus full-snapshot equality across a writable and a read-only volume, idempotence, read-only volume never written, originals never modified, newest sidecar wins, Sony-layout inference with foreign sidecars, missing recipes reported, random-layout property test.
- `path_props.rs` (6 property tests): separators, round trips, escapes, NFC/NFD, case rules, catalog uniqueness.

**Dependencies**: `rusqlite` 0.40, `quick-xml` 0.42, `unicode-normalization` 0.1; dev: `proptest` 1, `tempfile` 3 (ADR-011).

### Change from the reviewed schema
`files_one_original` is unique on `(asset_id, volume_id)` and `xmp_sidecars` is keyed by `(asset_id, volume_id)`, so a backup copy on a second volume can be an original with its own sidecar (ADR-012).

### Not done / known limits
- macOS CI is still deferred; tests simulate NFD names and case-insensitive volumes on Windows.
- Capture time, camera and duration come only from sidecars until the Probe (ExifTool) lands, so a fresh card shows no calendar days.
- Volume `fs_id` is the canonical mount path until the device watcher provides real ids.
- Lightroom won't see edits to raw files (it reads only `<stem>.xmp`).

### Files changed
- `Cargo.toml`, `crates/keepsake-core/Cargo.toml` — new dependencies
- `crates/keepsake-core/src/catalog/**` — NEW: catalog, migrations, repositories, FTS, snapshot
- `crates/keepsake-core/src/relpath.rs`, `xmp/**`, `library.rs`, `metadata.rs`, `rebuild.rs` — NEW
- `crates/keepsake-core/src/corpus/scanner.rs` — `skip_reason` made crate-visible
- `crates/keepsake-core/tests/{catalog_tests,xmp_tests,rebuild_tests,path_props}.rs`, `tests/common/mod.rs` — NEW
- `tests/fixtures/xmp/{lightroom,digikam}.xmp` — NEW hand-written sidecar fixtures
- `tests/fixtures/names/Exports/Été 2024/Fête du lac 01.JPG` — NEW fixture with spaces and non-ASCII characters in its path
- `crates/keepsake-cli/src/commands/library.rs` — NEW CLI commands
- `docs/ARCHITECTURE.md`, `docs/DECISIONS.md` (ADR-011, ADR-012), `docs/SPEC.md`, `TEST_ME.md`

---

## Spike 2 — Playback Gate 1 (2026-10-16)

### What was built

Proved five playback/performance items on Windows 11; macOS deferred.

**Item 1 (SUB proxy):** Corpus SUB dir is empty. Inferred PASS from identical H.264 MP4 container working in Item 2.

**Item 2 (generated proxy + timing):**
- XAVC S (H.264) → proxy via `h264_mf`: 4.5–5.2× real-time (1.4–2.6 s per 6–14 s clip).
- Synthetic 4K → proxy: 5.5× real-time; synthetic 1080p60-intra → proxy: 6.2× real-time.
- Proxy plays with seeking in WebView2 via Tauri `asset://` protocol. ✅

**Item 3 (AVCHD date):**
- `DateTimeOriginal` present in MTS in-stream metadata via ExifTool.
- `CreateDate`/`TrackCreateDate` absent on test clip (fallback path needed).
- Multi-segment join not testable (single MTS file in corpus). ✅ (partial)

**Item 4 (ARW preview):**
- ExifTool one-shot: avg 282 ms — **FAILS 150 ms target**.
- Root cause: Perl startup. Solution: ExifTool `-stay_open` mode (already in architecture). Not a blocker.

**Item 5 (thumbnail grid):**
- 1,876 files, 5-column TanStack Virtual grid, 55–60 FPS in WebView2. ✅

**Recommendation:** Keep Tauri + Rust. Libmpv fallback not needed. See ADR-009.

### Files changed
- `app/ui/src/SpikePlayback.tsx` — spike test page (video + seek test + virtual grid)
- `app/ui/src/App.tsx` — hash router for `#spike`
- `app/ui/src-tauri/src/commands.rs` — `spike_list_files` command
- `app/ui/src-tauri/tauri.conf.json` — asset protocol enabled
- `app/ui/src-tauri/Cargo.toml` — `protocol-asset` feature
- `scripts/spike-playback.ps1` — backend timing script
- `docs/spikes/playback.md` — full spike results
- `docs/DECISIONS.md` — ADR-009

---

## corpus: init-expected merge, zero-entry guard, sample command (2026-10-16)

### What was built

**`keepsake corpus init-expected --force` merge behaviour**
- Zero-entry guard: bails with a clear error if the manifest has no entries.
- With `--force`: if an existing `expected.toml` is present, merges instead of overwriting. Confirmed entries preserved verbatim; entries no longer in the manifest appended with `removed = true`. New assets get blank metadata.
- `removed = true` omitted from TOML when false (`skip_serializing_if`).
- Placeholder-only TOML (comment header, missing `asset` key) tolerated and treated as empty.

**`keepsake corpus sample --n N --mixed`**
- Reads manifest + optional expected.toml; generates assets via grouper.
- Prints table: `#  camera  type  key file  capture_ts`. `--mixed` round-robins across (camera, type) buckets.
- `type` and `key file` inferred from `detect_role` on file paths.

### Files changed
- `crates/keepsake-core/src/corpus/types.rs` — added `removed: bool` to `AssetGroup`.
- `crates/keepsake-core/src/corpus/grouper.rs` — initialises `removed: false`.
- `crates/keepsake-cli/src/commands/corpus.rs` — `run_init_expected` (merge + guard), `merge_with_existing`, `run_sample`, `sample_mixed`, helpers.
- `corpus/manifest.json` and `corpus/expected.toml` — regenerated from live ILCE-7M4 corpus (60 files → 56 assets).

---

## corpus: scan --summary and skip tracking (2026-10-15)

### What was built

**`keepsake corpus scan --summary`**: per-camera type counts, filesystem mismatch check, skipped-file list.
**Hidden/system file detection**: Windows file-attribute bits (0x2/0x4) + dot-files on all platforms. Stored in `CameraCard::skipped`.

### Files changed
- `crates/keepsake-core/src/corpus/types.rs` — added `SkipReason`, `SkippedFile`; `skipped` field on `CameraCard`.
- `crates/keepsake-core/src/corpus/scanner.rs` — `skip_reason()`, updated `collect_files`.
- `crates/keepsake-cli/src/commands/corpus.rs` — `--summary` flag, `print_camera_summary()`, `collect_all_fs_paths()`.

---

## Prompt 1 — Test corpus infrastructure (2026-10-15)

### What was built

**`keepsake corpus scan <path>`** (`keepsake-cli`)
- Walks `<path>/cards/<camera>/` subdirectories.
- Per file: relative path, size, BLAKE3 (hex), detected kind (FileKind), folder role (FolderRole).
- Writes `corpus/manifest.json` (default) — no media bytes.
- Path defaults to `KEEPSAKE_CORPUS` env var.

**`keepsake corpus init-expected`** (`keepsake-cli`)
- Reads `corpus/manifest.json`, groups files into logical assets using path-only heuristics.
- Writes `corpus/expected.toml` with `confirmed = false` placeholders.
- Safe by default: errors if output exists; pass `--force` to overwrite.

**`keepsake-core::corpus`** module
- `types`: `FileKind`, `FolderRole`, `FileEntry`, `CameraCard`, `Manifest`, `AssetGroup`, `Expected`.
- `scanner`: walks a corpus root, hashes files with BLAKE3, assigns folder roles.
- `grouper`: groups FileEntries into AssetGroups by path stem heuristics (XAVC, AVCHD, DCIM, MP_ROOT).

**Fixture generator** (`scripts/make-fixtures.ps1`, `scripts/make-fixtures.sh`)
- Generates real 1-second FFmpeg lavfi MP4/MTS/JPEG fixtures when `vendor/ffmpeg` is present.
- Falls back to text stubs (path tests only) when FFmpeg is absent (`-Stubs` / `--stubs`).

**Committed fixtures** (`tests/fixtures/`)
- Text stubs covering XAVC (2 clips + proxies), AVCHD (2 segments + playlist + BDM), DCIM (JPEG+ARW+HIF), MP_ROOT.
- Run `scripts/make-fixtures.ps1` locally to replace with real FFmpeg-generated media.

**Fixture tests** (`crates/keepsake-core/tests/fixture_tests.rs`) — always run in CI
- `detect_role` unit tests for all nine role variants.
- Scanner integration: file counts and role distributions for all four layouts.
- Grouper integration: XAVC two-asset split, AVCHD three-group split, DCIM raw-pair joining, MP_ROOT one-per-file.
- BLAKE3 determinism across two independent scans.

**Corpus tests** (`crates/keepsake-core/tests/corpus_tests.rs`) — `#[ignore]`, run with `--include-ignored corpus`
- `corpus_scan_finds_cameras`: verifies corpus root has at least one non-empty camera card.
- `corpus_scan_roundtrip`: JSON serialise → deserialise preserves all hashes.
- `corpus_grouper_matches_expected`: confirmed assets in `corpus/expected.toml` are reproduced by grouper.

**New dependencies**: `clap` v4 (ADR-007), `toml` v0.8 (ADR-008).

**Also fixed**: CI `tauri.conf.json` path (`app/src-tauri` → `app/ui/src-tauri` after Prompt 0b restructure).

---

## Prompt 0b — Fix vendor binary pipeline (2026-10-15)

Fixed broken setup scripts and established a durable vendor-binary mirror:

- `scripts/setup.ps1` — rewrote FFmpeg section: replaced broken gyan.dev (GPL) URL with BtbN/FFmpeg-Builds LGPL builds for Windows x64 and ARM64; real SHA-256 checksums; architecture auto-detection; LGPL verification (`ffmpeg -version` asserts no `--enable-gpl`, no `--enable-version3`); clear error messages naming file + URL on any failure.
- `scripts/setup.sh` — updated ExifTool to SourceForge mirror (exiftool.org now redirects); macOS FFmpeg section replaced with an explicit deferred stub pending a custom LGPL compile workflow.
- Created GitHub Release `vendor-binaries-v1` at `aelkays-hub/keepsake/releases/tag/vendor-binaries-v1` containing all four pinned assets: FFmpeg n9.0 LGPL win64, FFmpeg n9.0 LGPL winarm64, ExifTool 13.59 Windows 64-bit, ExifTool 13.59 Unix tarball. All SHA-256s verified against upstream sources.
- `docs/DECISIONS.md` ADR-005 — updated with BtbN as the authoritative FFmpeg source, full SHA-256 checksums, mirror URL, and notes on the deferred macOS LGPL compile workflow.

---

## Prompt 0 — Bootstrap (2026-10-14)

Created the full repository skeleton:

- `CLAUDE.md` — engineering guide with product description, non-negotiables, stack, layout, conventions, commands, and definition of done.
- `docs/SPEC.md` — feature specification across MVP, v1, and v2.
- `docs/ARCHITECTURE.md` — components, data model, Sony card layouts, capture-date precedence, ingest pipeline.
- `docs/DECISIONS.md` — ADRs 001–006.
- `docs/PROGRESS.md` — this file.
- Cargo workspace with `keepsake-core`, `keepsake-platform`, `keepsake-cli`.
- `app/src-tauri` — Tauri 2 app showing version on placeholder screen.
- `app/ui` — React 18 + TypeScript + Vite scaffold.
- `scripts/setup.sh` and `scripts/setup.ps1` — idempotent dev-environment setup.
- `.github/workflows/ci.yml` — matrix CI for `windows-latest` and `macos-14`.
- `TEST_ME.md` — manual test steps for the bootstrap build.
- Branch `bootstrap`, PR opened.

---

## Prompt 3 — Packaging (2026-10-16)

### What was built

**Release CI workflow (`.github/workflows/release.yml`)**
- Triggers on tags matching `v<MAJOR>.<MINOR>.<PATCH>`.
- `build-win-x64` job: downloads x64 FFmpeg + ExifTool from the vendor-binaries-v1 mirror, verifies SHA-256, builds MSI + NSIS via `pnpm tauri build --bundles msi,nsis`.
- `build-win-arm64` job: downloads ARM64 FFmpeg, cross-compiles for `aarch64-pc-windows-msvc`.
- Signing step: runs `signtool.exe` only when `WINDOWS_CERTIFICATE` secret is present; skipped otherwise. Release notes include a clear unsigned-build warning when secrets are absent.
- `smoke-test-win` job: downloads x64 MSI from the artifact store, installs silently with `msiexec /quiet`, verifies installed file paths, runs `keepsake --version` and `keepsake corpus scan tests/fixtures/` against the CLI binary.
- `release` job: creates a GitHub Release with all installer artifacts and signing status in the body.
- macOS DMG job: scaffolded as a commented-out block; deferred until LGPL macOS FFmpeg binary is available (ADR-005).

**FFmpeg + ffprobe declared as bundle resources**
- Added `"../../../vendor/ffmpeg.exe": "ffmpeg/ffmpeg.exe"` and ffprobe to `tauri.conf.json`.
- CI resource-declaration check updated to include ffmpeg.exe and ffprobe.exe.

**`keepsake_core::paths` module**
- `data_dir()` → `%LOCALAPPDATA%\keepsake` (Windows) / `~/Library/Application Support/keepsake` (macOS)
- `cache_dir()` → `%LOCALAPPDATA%\keepsake\cache` (Windows) / `~/Library/Caches/keepsake` (macOS)
- `config_dir()` → `%APPDATA%\keepsake` (Windows) / `~/Library/Application Support/keepsake` (macOS)
- Uses `dirs` v5 (MIT, ADR-010). All three functions have unit tests (pass on CI).

**`scripts/release.md`**
- Plain-language steps: bump version in `Cargo.toml`, commit, push to `main`, create and push tag, watch release workflow, publish the draft.
- Includes signing setup instructions (Windows PFX → base64 secret, macOS deferred).

**`docs/ARCHITECTURE.md`** updated with Data Directories section.  
**`docs/DECISIONS.md`** updated with ADR-010 (`dirs` crate).

### Files changed
- `.github/workflows/release.yml` — NEW: release build workflow
- `.github/workflows/ci.yml` — updated resource-declaration check (adds ffmpeg/ffprobe)
- `app/ui/src-tauri/tauri.conf.json` — added ffmpeg.exe and ffprobe.exe bundle resources
- `Cargo.toml` — added `dirs = "5"` to workspace deps
- `crates/keepsake-core/Cargo.toml` — added `dirs.workspace = true`
- `crates/keepsake-core/src/lib.rs` — exposed `pub mod paths`
- `crates/keepsake-core/src/paths.rs` — NEW: `data_dir`, `cache_dir`, `config_dir`
- `scripts/release.md` — NEW: release instructions
- `docs/ARCHITECTURE.md` — Data Directories section
- `docs/DECISIONS.md` — ADR-010
- `docs/PROGRESS.md` — this entry
