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
| — | Select many to edit: tick-box on tiles, Select mode, drag a rectangle; inspector shows what the selection shares; comment for all | ✅ Done (Windows; macOS via CI) |
| — | Library report (Tools → Report, `keepsake report`): photos, videos, files and size by year, month, day, camera, lens, file type and drive; Export to CSV | ✅ Done (Windows; macOS via CI) |
| — | Report rows open their photos in the library (Report › Year 2024, back to the report) | ✅ Done (Windows; macOS via CI) |
| — | Headless thumbnail backfill (`keepsake renders thumbnails`): every missing thumbnail, 100 per saved batch, resumes after a kill, passes bad files | ✅ Done (Windows; macOS via CI) |
| — | Faster thumbnails on a network drive: fingerprints wait for thumbnails; camera JPEG thumbnails from the built-in preview | ✅ Done (Windows; macOS via CI) |
| — | Thumbnail sizes (Small, Medium, Large) in the grid, and a zoom slider in the viewer | ✅ Done (Windows; macOS via CI) |
| — | Adding a folder saves 20 files at a time, shows it at once and resumes where it stopped | ✅ Done (Windows; macOS via CI) |
| — | Adding a folder reads only the start and end of each file; full fingerprints are made in the background | ✅ Done (Windows; macOS via CI) |
| — | Information panel in the viewer: key facts and every metadata tag (Info button, I) | ✅ Done (Windows; macOS via CI) |
| — | Calendar view shows only the months and days that have pictures | ✅ Done (Windows; macOS via CI) |
| — | Faster viewer zoom: full size made in the background, upright JPEGs copied, transform-based zoom; labelled thumbnail sizes | ✅ Done (Windows; macOS via CI) |
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
| 21 | On-device AI: tagging, face clustering, semantic search | 🟡 Tagging and same-day duplicates done (ADR-035); faces and semantic search pending |
| 22 | Multi-view player and time-lapse | ⬜ Pending |

---

## Tag the photos with no date (2026-11-08)

- **Asked:** "I have around 1700 pictures that have no date but I can't tag them". Right-clicking **No date** in the calendar did nothing: Tag photos took only a date range.
- **Built:** `ai::Target::Undated` (items with no capture time); app `ai_analyze_undated`, `ai_undated_status`, `ai_undated_stop`; the sidebar's **No date** has a menu with Show and Tag photos (not Find duplicates: those are of one day; "Calendar" still has no menu); the progress window says "Photos and videos with no date."
- **Tests, run here:** `the_photos_with_no_date_can_be_tagged_and_stopped` (ai_tests); mocked Playwright `right-click No date: tag the photos with no date` and `the Calendar entry has no date menu` (ai.spec), with the calendar-tree, keyboard and a11y specs; app and core tests; clippy, ESLint, Prettier.


## Helpers end with the app (2026-11-08)

- **Asked:** "got an error on install". The installer could not replace `exiftool.exe`: two ExifTool processes from earlier sessions were still running. Then "Yes, fix it" (ADR-037).
- **Why they were left:**
  - A `-stay_open` ExifTool does not end when its input closes.
  - Closing the window ends the app without running destructors.
  - The import ExifTool lives in a `static`, which is never dropped.
- **Built:**
  - `ExifTool::shutdown` and `ExifTool::pid`.
  - The app stops the render service and the import ExifTool on exit.
  - On Windows, the app joins a kill-on-close job object (`keepsake_platform::close_helpers_with_app`). Programs opened with "Open in" or in Explorer break away from it and stay open.
- **Tests:**
  - `shutdown_ends_the_process_even_when_the_handle_is_kept` (real ExifTool).
  - `closing_the_job_ends_its_processes` and `a_process_outside_the_job_keeps_running` (Windows).
- **Not verified:** closing the installed app and checking Task Manager. That is a manual step in TEST_ME.md.

## Tag with Claude (2026-11-08)

- **Asked:** "for individual photos, I would like to add another tagging option using the best external LLM that can do it". The product owner chose (ADR-036):
  - the model in Settings: Opus 5.5 by default, Fable 5.1 or Haiku 5.5;
  - `ureq` + `keyring` as the new dependencies;
  - tags from the list plus up to 5 new ones;
  - a right-click menu on photos.
- **Built:**
  - **Core, `ai::cloud`:**
    - The request: photo, tag list, JSON schema, `effort: low`, fallback.
    - Reading the answer: list tags only, new tags tidied, refusals and errors sorted into retry or not.
    - Keyword paths, with new ones under `AI|`.
    - The `ai.claude` job, with `enqueue`, `status` and `stop`. The render service runs it on its own worker (`with_claude`). `RenderError::Refused` is not retried.
  - **Platform, `secrets`:** the API key in Windows Credential Manager / the macOS Keychain. `KEEPSAKE_CLAUDE_KEY` overrides it.
  - **App:** `ai_claude_info`, `ai_claude_set_key`, `ai_claude_set_model`, `ai_claude_tag` (a selection), `ai_claude_status` and `ai_claude_stop`. A new setting, `[ai] claude_model`.
  - **UI:**
    - A right-click menu on the grid: Open and Tag with Claude… (also the Menu key or Shift+F10).
    - A Tag with Claude button in the viewer.
    - Settings → Tag with Claude: model and key.
    - The progress window asks first (count, model, cost) and shows problems.
- **Tests:**
  - 7 unit tests in `ai::cloud`: base64, key check, request, answer, errors, keyword paths, headers.
  - `claude_tags_become_keywords_from_the_thumbnail_only` and `a_refused_key_fails_the_photo_and_says_why` run the worker with a stand-in for Claude.
  - The `secrets` environment override.
  - 5 mocked UI tests (`e2e/mocked/claude.spec.ts`).
- **Not verified:**
  - A real call to the Claude API: there is no key in this session or in CI.
  - The Windows ARM64 build with `ring`.

## On-device AI: content tags and same-day duplicates (2026-11-07)

### What was asked
- A small model running on the product owner's own computer to (1) find exact and near-identical photos taken on the same day and pick a keeper, without touching originals, and (2) tag photos by content (people, beach, dog, food), searchable and removable. Design first, then build after approval.

### What was decided (ADR-035, approved)
- **Model:** SigLIP 2 base (Apache-2.0), with the image encoder only. **Runtime:** ONNX Runtime via `ort` (load-dynamic), using DirectML on Windows and CoreML on macOS, else the CPU.
- **Tags:** a fixed list of 102. **Duplicates:** non-keepers are marked "not kept" and nothing is deleted.
- **Measured** on the product owner's laptop: 80 ms a photo on the integrated Radeon (fp32), so 100k photos take about 2.2 h once.

### What was built
- **Core** (`keepsake_core::ai`):
  - the model wrapper, with GPU first and CPU fallback, and preprocessing;
  - the int8 embedding store and the 64-bit difference hash;
  - the label set and its SigLIP scoring;
  - same-day grouping (union of hash and cosine links, burst-aware);
  - the `ai.analyze` job on the render service's new AI worker;
  - model install (curl + BLAKE3) and re-tagging from stored vectors.
- **Catalog:** schema v9 (`ai_analysis`, `ai_tag_rejections`, `duplicate_choices`); `SearchQuery::hide_not_kept`; the snapshot includes the new decisions.
- **Edits:** `MetadataService::{reject_ai_tag, keep_ai_tag, set_duplicate_choice, pick_keeper}`. The XMP fields `keepsake:AITagsRejected` and `keepsake:DuplicateChoice` are read and written, and restored by rebuild.
- **App:**
  - Duplicates screen;
  - Settings → On-device AI (download with progress, GPU switch, delete model);
  - AI tags in the inspector;
  - Hide not kept in the toolbar;
  - ONNX Runtime and DirectML in the Windows installer.
- **CLI:** `keepsake ai status | install | run | duplicates | retag`.
- **Setup:** `setup.ps1` installs ONNX Runtime + DirectML (x64 or ARM64), and `-WithAi` adds the model. `setup.sh` does the same for ONNX Runtime 1.31.1 on Apple Silicon, with `--with-ai`.
- **Label vectors:** `scripts/make-label-vectors.py` (developer tool).

### Changed the same day: only the dates you ask about
- **Asked:** "why is keepsake so slow in finding duplicates", then "it should not try to seek the whole catalogue to find them, I should be able to go to a specific date and right click to select find duplicated and tag".
- **Found:** 144 of 129,127 items had been analysed, at 1.5 s each. Each result waited 2–3.5 s for the catalog on the USB drive (its write-ahead log was 247 MB) while background thumbnails and proxies wrote too. The model took 80 ms, and on a copy of the catalog a save took under 1 ms.
- **Built:**
  - Turning AI on analyses nothing by itself.
  - Right-click a year, month or day in the sidebar's calendar tree → **Find duplicates and tags** (also the Menu key or Shift+F10). It queues that range ahead of other work (thumbnails too, no proxies) and opens Duplicates on it, with "Analysing: n of m done…", refreshing every 3 s. "All analysed dates" goes back to every day.
  - The worker drops the old background AI jobs and waits only for requested thumbnail jobs.
  - `keepsake ai run --from --to` analyses a range from the CLI.
- **Tests:** `only_the_dates_asked_for_are_queued_ahead_of_the_backlog` and `the_summary_can_be_limited_to_dates`. The worker test now asks for its day, with a stale background job dropped. The mocked UI covers right-click a year, the keyboard menu, and AI off.

### 7 Nov 2026 (evening): one progress window, and the launcher updates itself
- **Asked:** "I do not see the tagging progress bar?" Tag photos on a date showed only a line in the status bar; the progress window was for folders. Now Tag photos on a year, month or day opens the same window, with its bar, Stop (`ai_range_stop`) and Hide. The status-bar line is gone.
- **Asked:** "update my script in keepsake/scripts". `scripts/keepsake.bat` now gets the latest changes of its branch (`git pull --ff-only`) before it starts or builds the app. It skips this when files there were changed, and carries on with the copy it has if it is offline.
- **Tests:** the mocked UI's date tagging uses the window (getting ready, bar, done), Stop on dates, and AI off in the window.

### 7 Nov 2026 (evening): an app icon
- **Asked:** "how about we create an icon for the app!" The product owner chose a modern camera (from a camera and a lens, after three first concepts).
- **Built:** a mirrorless camera, front view, dark grey on an orange rounded square. The source is `app/ui/src-tauri/icons/icon.svg`. It was rendered to a 1024 px PNG (Edge, headless), and `pnpm tauri icon` made the `.ico` (Windows), the `.icns` (macOS) and the PNGs; only the desktop files are kept. This replaces the blank placeholders. No new dependency.

### 7 Nov 2026 (evening): whole folders, a progress window, quicker saves
- **Asked:** "I want to be able to run it at a top level folder tree and it traverses the tree and tags, I would like to see a window showing progress", and to speed it up. Before that: "Keepsake started and now everything is hung". The app was busy, not hung. On start it caught up on thumbnails and proxies, and catalog writes on the USB drive took up to 12 s. By the time it was checked nothing was queued.
- **Built:**
  - Folder menu: **Find duplicates** and **Tag photos** for a folder and all below it.
  - A progress window in the corner with Stop, Hide (a corner button brings it back), Close and Show duplicates.
  - The AI worker claims and saves 32 jobs at a time: two catalog writes per 32 photos instead of 96.
  - Core: `ai::Target`, `enqueue_target`, `target_status`, `stop_target`, `Ai::matching`, `Jobs::pending_matching`, `Jobs::remove_queued_for`, `ai::analyze::{prepare, save}`.
  - App: `ai_analyze_folder`, `ai_folder_status`, `ai_folder_stop`.
- **Tests:** `a_folder_and_the_folders_in_it_can_be_tagged_and_stopped`; the worker and range tests run through the batched worker; the mocked UI covers tagging a folder (window, Hide, done) and stopping a folder's duplicate search.

### 7 Nov 2026 (later): best-guess tags and a tag list in the toolbar
- **Asked:** "tagging is still very poor we need a stronger AI … I am getting less than 50% of pics tagged", and "would be good to have a dropdown of tags at top of screen".
- **Measured:** SigLIP 2 Large (approved for a trial) on 300 random library photos.
  - It was no better than the base model: 233 photos with a sure tag against 247, and about 20 of 24 right best guesses against 21.
  - It was three times slower, so it was not adopted.
  - The base model's best guess was usually right but under the 0.02 cut-off.
- **Built:**
  - A photo with no sure tag gets its best guess (two if close), down to 0.002. 296 of the 300 now get a tag, against 247.
  - New tags: Moon, Mosque, Helicopter and Squirrel (110 in all). Tag list version 3, so older tags are redone quickly on the next Tag photos.
  - The toolbar's **Tag** list ("Any tag", then every tag with its count, most used first) filters the library to that tag and the tags below it.
- **Tests:** `a_photo_with_no_sure_tag_gets_its_best_guess`, tag counts and the tag filter in `hierarchical_tags_and_search`, and the mocked UI's tag list.

### 7 Nov 2026: people are tagged, and Tag photos shows it has started
- **Asked:** "when menu is selected, there is no indication of anything happening, there needs to be some feedback...like tagging", and "a simple picture contains a person or face and your AI model is failing maybe we need to change the model with a better one".
- **Found:**
  - No feedback: the menu waited for the work to be queued, which takes seconds on the USB drive.
  - People: the model was fine, but each tag averaged its phrases. On 48 of the library's photos, none of 15 with people got People.
- **Built:**
  - Tag photos closes the menu at once. The status bar shows "Getting ready to tag…" with a spinner, then the progress; a failure stays until dismissed. Find duplicates shows "Working…" in the menu.
  - Tags score their best phrase, with many more phrases for people; Baby, Child, Portrait, Group photo, Selfie, Crowd and Police also give People. 106 tags (Climbing, Cave, Coin, Police added). Vector file format 2.
  - Result: 12 of the 15 people photos get People, and none of the 33 others does.
  - Schema v11: `ai_analysis.tag_list`. Photos tagged with the old list are re-tagged from their stored analysis when Tag photos runs on their dates (quick, no model).
  - `keepsake ai explain <picture>` prints every tag's score for one picture.
- **Tests:**
  - New tests: `photos_tagged_with_an_older_tag_list_are_tagged_again`, `a_tag_scores_its_best_phrase_and_gives_the_tag_it_implies`, `the_list_is_checked`.
  - Mocked UI: "Getting ready to tag…" with slow queueing, the AI-off message in the status bar, and "Working…" for Find duplicates.

### Changed the same day: Find duplicates and Tag photos are separate
- **Asked:** "let us separate finding duplicates from tags, they should be separate functions runs seperatly".
- **Built:**
  - The date menu has **Find duplicates** (RAW photos, `ai.analyze`, no tags; opens Duplicates) and **Tag photos** (every photo, `ai.tag`; progress in the status bar).
  - Tagging an already-analysed photo reuses its stored embedding.
  - Schema v10 adds `ai_analysis.tagged`.
  - `keepsake ai run --tags`.
- **Tests:** `finding_duplicates_and_tagging_are_separate`. The range and worker tests now use Tag photos, and the mocked UI covers both menu items and the status-bar progress.

### Changed the same day: duplicates among RAW photos only
- **Asked:** "Only determine duplicates in RAW format, a picture that as the same name as a raw file with a jpg extension is not a duplicate it is an edited copy of the picture".
- **Built:** only items with a RAW file are compared (`duplicates::Scope::Raw`, `assets.is_raw`). JPEG-only photos, edited copies and videos are never grouped. Tags are unchanged.
- **Test:** `only_raw_photos_are_duplicates_and_an_edited_jpeg_is_not_one`.

### Tests
- **keepsake-core unit tests:** embedding round trip; difference hash (re-encoded copy matches, a different picture does not); label file checks (mismatch refused, CRLF checkout fine); SigLIP probability; grouping (hash and cosine links, burst bar, photos and videos never mixed, "not duplicates", keeper choice, big-day window); preprocessing; download checks (bad hash not kept, cancel leaves nothing).
- **`ai_tests`:**
  - **Without the model:** analyses stored and found by day; unanalysed items queued; AI tags searchable; a removed tag is never given again and is in the XMP; keep turns a tag into a keyword; decisions survive a catalog rebuild and originals are byte-for-byte unchanged; same-day groups, keeper and summary; different days never group.
  - **With the model** (skipped if absent): a re-encoded copy is the same picture and a different one is not; GPU and CPU agree; the full render-service run makes thumbnails, then analyses, then groups the two identical photos.
- **UI:** unit tests for the helpers, plus `e2e/mocked/ai.spec.ts` (pick a keeper, keyboard N, Undo, Hide not kept; AI off → Settings → on and off, delete model; inspector remove and keep).

### Not done
- Faces, semantic search, your own tag words, a sharpness-based keeper suggestion.
- Mirroring the new downloads on `vendor-binaries`.
- macOS was not run (no Mac in this session), and the ARM64 installer was not tried.
- Per-tag thresholds tuned on a labelled sample.

---

## Headless thumbnail backfill (2026-11-05)

### What was asked
- A program with no UI that walks every picture in the library and makes the missing thumbnails unattended: 100 items, one commit, next 100; resumable after a crash, a kill or a bad file.

### What was built (ADR-033)
- `keepsake renders thumbnails`, a subcommand rather than a new binary: the CLI already finds the library, settings and tools, handles Ctrl-C and ships with the app.
- `keepsake_core::render::backfill`: walks `assets` by `seq` from a position saved in `library_meta` (`render.backfill.cursor`). Each batch is rendered first (files only), then one short write records the batch's `renditions` rows and the position. Thumbnails only (256 and 1024 px), from a camera JPEG's built-in preview when it has one. Workers from the settings (4 here).
- A failed file, or a decoder panic, is reported (`FAILED <file>: <error>`, one line, and `--failures-log`) and passed. A batch whose media is all offline stops the run without moving past it (`--skip-offline` to move on). `--restart` walks everything again to retry failures; `--max-batches` stops early.
- Refactor: `Cache::store_bytes` and `still::make_stills` write rendition files and return the rows unwritten; `render_stills_of` now records an asset's renditions in one write. `Assets::page_after` / `count_after` for the walk.

### Tests
- `backfill_tests` (9, no tools needed): batches of 100 saved one by one (seen from another connection); a stopped run resumes after the saved batch and no asset is rendered twice; a catalog failure mid-batch keeps the earlier batch and loses only the current one; bad files and a panicking renderer are reported and passed; an all-offline batch stops without moving, then continues when the media is back; `skip_offline`; Ctrl-C saves the finished part; assets added later are picked up.
- `cli_tests::renders_thumbnails_saves_batches_resumes_and_logs_failed_files`: batch size 2, `--max-batches 1`, resume, failures log, JSON Lines.

### Run on the real library
- `E:\Pictures\2026` (129,127 assets, media on `\\MYCLOUD-92LNT0`), with the app open. Run 1 walked to #3,700 (mostly assets that already had thumbnails) and was killed with `Stop-Process -Force`; run 2 printed "Continuing after asset #3700" and made 100 thumbnails per batch (about 130 a minute from the share), was killed at #4,300; a third run continued after #4,300. No file failed in those batches.
- 5 of the first ~37 batch writes took 2–3.6 s ("slow catalog write"), the rest were quick; later batches had none. The writes are 200 rows and one key, so the time is the disk (WAL checkpoint and flush on E:), not the batch size.

### Bugs found and fixed on the way
- FFmpeg errors span several lines; failures are now folded to one line per file.
- With output redirected to a file, the hidden progress bar dropped every line; they now go to stdout.

### Not done
- The backfill does not trim the cache; the app's render service does (the cache was at 19.4 of 21.5 GB, so older previews will be evicted to make room).
- The app's queued `render.stills` jobs are left alone: they find the thumbnails and make only the preview.
- The same hidden-bar problem affects `keepsake renders run` when its output is redirected (not changed here).

---

## Faster thumbnails on a network drive (2026-11-02)

### What was asked
- "How can we speed up the thumbnail generation process? With the amount of pics I have it will take multiple days." Then: "Yes, do both."

### What was found
- The app made about 29 thumbnails a minute from the NAS, with 57,000 photos left (about 33 hours). The same files on the local disk: 323 JPEGs or 527 RAWs a minute (`keepsake renders bench`). The network drive was the limit, not the CPU.
- 52,000 background fingerprint jobs were reading whole files (1.4 s for a 48 MB RAW) at the same time. Camera JPEGs were read in full (18.6 MB) for two thumbnails, though they hold a 185 KB preview.

### What was built (ADR-032)
- The fingerprint worker waits while a thumbnail job is ready (it looks again every 5 s), so thumbnails get the drive first.
- Thumbnails alone of a JPEG with a built-in MPF preview of at least 1024 px are made from it (`still::jpeg_builtin_preview`, `mpf_preview_at`): the first 256 KB and the preview are read. Orientation follows the main picture; anything unusual falls back to the whole file. The viewer's preview is still the whole picture.

### Tests
- `render_tests`: a camera-style JPEG (EXIF orientation, MPF index, preview after the picture) gives its preview, turned; no MPF, a small preview or a cut file give none; garbage does not panic. Thumbnails alone come from the preview, and the viewer's preview from the picture. With thumbnail and fingerprint jobs queued, every fingerprint starts after the last thumbnail job.
- Corpus (`corpus_jpeg_built_in_previews_match_the_picture`): 59 of 1,010 JPEGs have a preview; each matches the picture in shape, orientation and content.

### Not done
- Edited JPEGs have no built-in preview and are still read in full. The speed on the NAS after this change has not been measured yet; restart the app and compare.

---

## Report rows open their photos (2026-11-01)

### What was asked
- "The report should have a hyperlink that will take me to the set of photos I clicked on": a year shows all that year's photos on one screen, a camera all of that camera's, and so on.

### What was built
- Each row's name in Tools → Report is a link. It opens the library grid with that row's items and no other filters (words, type, rating and sort are reset). The heading reads **Report › Year 2024** (or Camera …, Lens …, Type …, Drive …), and **Report** goes back to the same breakdown.
- Years, months and days open as a date range; **Unknown** dates as the undated items. Drives open what is on that drive, plugged in or not.
- New search filters so that the grid holds exactly the row's count: `camera_is` and `lens_is` (the exact model, spaces around it ignored; `""` for none) and `file_type` (items with a file of that extension, any case; `""` for a file without one). The existing camera and lens filters match "contains", so `ILCE-7` also found `ILCE-7M4`; the report rows do not use them.
- UI: a `report` section in `uiModel.ts` (its title and filter), an `open` message that clears other filters, `lineQuery` and `lineTitle` in `report.ts`.

### Tests
- `report_tests`: for every row of every breakdown (dates, Unknown, cameras including `ILCE-7` beside `ILCE-7M4`, lenses, file types including a RAW+JPEG pair and a file with no extension, drives including an unplugged one), the row's filter finds exactly the row's item count.
- Vitest: each breakdown's filter and title, February in leap and other years; opening a row resets filters, sort and mode.
- Mocked Playwright `report.spec.ts`: clicking a year, month, camera, lens, type and drive row shows that many items and the heading, and **Report** returns to the same table; a row opens without the filters set before.

### Not done
- The toolbar's camera and lens lists still use the "contains" filter, so picking `ILCE-7` there also shows `ILCE-7M4` photos.

---

## A full thumbnail cache keeps the thumbnails (2026-11-01)

### What was reported
- "I run keepsake and nothing loads", "I see no pics", "I am not seeing the folder structure on the left sidebar".

### What was found
- The 20 GiB cache was full (20,482 MB: 7.5 GB of video proxies, 9.5 GB of previews, 3.4 GB of thumbnails) with 58,000 items still queued. Plain least-recently-used eviction removed the first thumbnails made, those of the newest photos, so the grid had none to show and queued them again.
- Each eviction pass held the catalog 2–7 s on the USB hard disk. Every new catalog connection also wrote a version stamp, which waited behind it: the window took 63 s to start (2.6 s that morning), and the sidebar and grid stayed empty.

### What was built (ADR-031)
- Eviction removes previews and proxies before thumbnails (`renditions().list_lru`). The render service trims to 90% of the limit once over it (`Cache::evict_to`).
- While the cache is above 80% of its limit, background jobs make the two thumbnails only, no preview or proxy (`still::render_stills_of`). On-screen jobs make everything.
- The viewer asks for a missing photo preview at once and looks again every 2 s, as it already did for a video proxy.
- `Library::open_catalog` writes its stamp only when missing or from another version, so opening a connection never waits for a writer.

### Tests
- `render_tests`: eviction takes a preview and a proxy before an older thumbnail, and trims to the target; with a full cache the service makes thumbnails only (no preview, no proxy, jobs done), and putting the items on screen makes the preview and proxy.
- `catalog_tests`: opening a library catalog again does not wait while another connection holds the write lock.
- Mocked Playwright `viewer.spec.ts`: a photo opened without a preview asks for it and then shows it.

### Not done
- A default limit sized from the library or the free space. For a library this size, raise the limit in Settings (for example 100 GB).

---

## Scan again one folder inside an added folder, from a right-click menu (2026-11-01)

### What was asked
- "need the ability to scan again a folder in an imported folder, need a right click menu on the folder than I need to scan again".

### What was built
- Core: `rebuild::rescan_folder(cat, lib, volume, Some(&rel), opts)` and `RebuildOptions::only`. A batched scan walks only that folder of the volume, and its library sidecar mirror. Only files under it can be found gone. Sequences elsewhere on the volume are kept. Ratings and other edits are kept as in any scan again. `None` scans the whole volume. It reports when the drive is not connected or the folder is not there.
- Tauri: `folder_rescan(volume, path)`, sharing the scan job and its events with `folder_add` (`folders::scan`). `FolderDone.scannedAgain` lets the outcome read "Scanned …" rather than "Added …".
- UI: `FolderMenu` opens on right-click of a folder or volume in the sidebar's folder tree, or with the Menu key or Shift+F10. It has **Show** and **Scan again**, with arrow keys, Escape and a click away to close it. Scan again is disabled when the drive is not connected, and shows why when a scan cannot start (another is running). Starting it opens the Folders screen, whose progress heading now says "Scanning a folder…".

### Tests
- `add_folder_tests`: scanning one sub-folder again finds new files and drops gone ones there (and below), keeps a rating, and leaves the rest of the folder alone (a file gone elsewhere stays, a new file elsewhere is not found yet). Scanning the whole volume after that catches up. A missing sub-folder or a disconnected folder is refused and changes nothing.
- Mocked Playwright `folder-tree.spec.ts`: the right-click menu scans the folder (and the whole volume from its root) and shows the outcome; no menu on other entries; the keyboard opens and closes it and focus returns; disabled on an unplugged drive; Show opens the folder; a scan that cannot start says why.

### Not done
- No CLI option for one sub-folder (`keepsake add-folder` scans the whole folder).

---

## Previous and next in the viewer, on the picture's edges (2026-11-01)

### What was asked
- "the viewer should give me the ability to go backwards and forwards when viewing a pic not close it and open the next one".

### What was built
- The viewer already stepped with ← → (and Page Up/Down, Home, End), but its only buttons were two small ‹ › glyphs in the footer corners, easy to miss.
- Now large ‹ › buttons sit on the left and right edges of the picture or video (`.viewer-step`, `viewer-prev` / `viewer-next`). They are brighter on hover and hidden at the first or last item. They step through the view, or through the selection when more than one item is selected, as the arrow keys do. The footer glyphs are gone, and the footer hint reads "← → previous / next".
- At the ends the buttons are `aria-disabled` rather than `disabled`, so keyboard focus stays in the viewer and the arrow keys keep working after a click.
- While zoomed in on a photo, the arrow keys still pan; the edge buttons step.

### Tests
- Mocked Playwright `viewer.spec.ts`: the buttons step forward and back without closing, are unavailable at the ends, and the arrow keys work after a click.

---

## "Database is locked" while a backlog runs (2026-11-01)

### What was reported
- "got this error SQLite error: database is locked". Thumbnail jobs failed with it a minute after start-up and every 13–25 s after. The catalog is on a USB hard disk with a 178 MB write-ahead log, and the queue held about 59,600 fingerprint and 58,200 thumbnail jobs.

### Cause and fix
- Each connection waited 5 s for another one's write, then failed. At start-up and after each folder scan, `render::enqueue_backlog` queues thumbnails and re-queues fingerprints for every item without a full fingerprint, about 83,000 here, 2,000 per write. On that drive one such write held the catalog for longer than 5 s.
- Jobs are now queued 200 per write (`render::ENQUEUE_CHUNK`), and connections wait up to 30 s (`catalog::BUSY_TIMEOUT`).
- Render, proxy and fingerprint workers treat "locked" as no fault of the job. The job is deferred 2 s with no attempt counted, and the worker's own claim or complete is tried again, instead of the error ending the worker.
- `Catalog::write` logs any write that holds the catalog for 2 s or more, with the line it came from (`slow catalog write`), so any other long holder shows in the log.
- Fingerprint jobs got sort keys counting from 0 in each write, so a backlog over one write was not newest first across writes. Keys now count across the whole backlog.

### Tests
- `catalog::error` unit tests: SQLITE_BUSY and SQLITE_LOCKED are busy; other errors are not.
- `fingerprint_tests`: 450 items with no fingerprint are queued over three writes and claimed newest first.

### Not done
- No automated test of a worker meeting a locked catalog (it would wait the full 30 s). Proved by the log on the reporting machine instead.

---

## Older video types are found when adding a folder (2026-11-01)

### What was reported
- "when Z:ideos was scanned it did not pick up all the content in the folder and its subfolders": a NAS `Videos` share with 306 videos gave 33 items (all its `.mp4` and `.avi`).

### Cause and fix
- Only `mp4, mts, m2ts, mov, avi, mkv` were videos; the share is mostly `.mpg` (233), `.vob` (32) and `.mpeg` (8), which were skipped as not media.
- One list, `grouping::VIDEO_EXT`, now used by the grouper, `rebuild::media_kind_for` and import (each had its own copy). Added `mpg, mpeg, vob, m4v, 3gp, wmv, mod, tod`.
- **Scan again** on an added folder picks up the new types (skipped files are not remembered).

### Tests
- `add_folder_tests`: a home video folder (MPEG at the top and in subfolders, `VIDEO_TS` VOBs, MOD/TOD with a MODD sidecar, 3GP, M4V, WMV) gives one video item per clip, and DVD IFO/BUP files are not items.

### Not done
- A DVD's VOBs are one item each, not one per title. Two videos with the same name in one folder (`VTS_01_1.mpg` beside `VTS_01_1.VOB`) are one item, with the second as an extra file.

---

## Start-up no longer freezes while the library opens (2026-11-01)

### What was reported
- "keepsake is hanging when I start it": the first start after the schema v8 upgrade sat with a frozen, blank window for 47 s (log: `window ready ms=47418`; usual 1.2–2 s). The catalog (112,674 items, 209 MB) is on a USB hard drive, and the upgrade rewrites every item row.

### Cause and fix
- The default library was opened inside Tauri's `setup`, on the main thread, so the window could not draw until any catalog upgrade was done.
- Now `open_default_in_background` opens it on its own thread. A `StartGate` in `AppState` makes `app_info` wait until it is done, so the UI's start logic is unchanged and cannot show first-run setup too early. The gate is ended by a drop guard, even if opening panics.
- The UI shows **Opening your library…** until then, and after 3 s adds that the first start after an update can take a minute while the catalog is upgraded.

### Tests
- `state.rs`: the gate returns at once when nothing is opening and holds until the opening finishes.
- Mocked Playwright `opening.spec.ts` (`?opening=<ms>` in the mock): the opening message, the note after 3 s, then the library; a quick open shows no message.

---

## Library report under Tools (2026-11-01)

### What was asked
- "under tools I would like to add a report on the total number of pictures … by date, year, camera type, lens, file type … amount of total storage they take on disk".

### What was built (ADR-030)
- **Tools → Report** (`ReportScreen.tsx`): totals (photos, videos, files, size) and one table at a time by Year, Month, Day, Camera, Lens, File type or Drive, with a total row; Refresh; **Export to CSV…** (save dialog, then `library_report_csv`). Unknown dates, cameras, lenses and extensions are one "Unknown" row, last.
- Core: `tx.report().library()` (`catalog/repo/report.rs`), GROUP BY queries only, no media read. A RAW+JPEG pair is one photo; files and size count every file of an item, one copy each (copies on backup drives share the BLAKE3 key). Unplugged drives count. The Drive table shows every copy per drive. `keepsake_core::report` writes the CSV (UTF-8 with BOM, formulas defused).
- **Schema v8**: `assets.file_count` and `assets.file_bytes`, kept by triggers on `files` and filled by the migration (about 15 s once for 300,000 items). Without them the report joined every file to its asset: 12 s at 300,000; now 1.5 s. File types and drives are a scan of `files` without that join, so they show items without the photo/video split.
- Tauri: `library_report`, `library_report_csv` (`src-tauri/src/report.rs`, thin); bindings `LibraryReport`, `ReportGroup`, `ReportDrive`; capability `dialog:allow-save` added. Mock backend computes the report from the mock items.
- CLI: `keepsake report [--by year|month|day|camera|lens|type|drive] [--csv PATH]`, with `--json`.
- Lens: already stored (`assets.lens_model`, from ExifTool `LensModel`), so nothing new to read.

### Tests
- Rust (`report_tests.rs`, 8 tests): empty library; RAW+JPEG pair is one photo with both files' bytes, and under both types; photos and videos apart by day, month and year, newest first; missing date, camera, lens and extension are Unknown and last; unplugged drives count and a backup copy counts once (drive table: every copy); a not-yet-fingerprinted copy counts until its fingerprint matches, and deletes update the totals; CSV content and BOM; a seeded 5,000-item library adds up in every breakdown. `report.rs` unit tests for sizes and CSV quoting. Scale test: the report at 300,000 items, 1.47 s (limit 2 s). CLI test: two-destination import counts once in totals and fully per drive, CSV written. `safety_audit_tests` lists the new CSV write in `SAFETY.md`.
- UI: vitest `report.test.ts` (labels, items, drives, CSV name); mocked Playwright `report.spec.ts` (totals, every breakdown, Unknown, unplugged drive, export, refresh, empty library); the a11y sweep includes the Report screen. All workspace Rust tests and all 76 mocked browser tests pass on Windows.

### Not done
- Filters on the report (whole library only), charts, lens make; photo/video split for file types and drives.

---

## Full fingerprints in the background for folders added in place (2026-10-31)

### What was wrong
- `2019` on the network drive is 598 GB of RAW. Adding a folder read every byte for its BLAKE3 fingerprint, so at 25-30 MB/s the folder took 5-6 hours to be fully in the library.

### What was built (ADR-029)
- A batched add (app and `keepsake add-folder`) reads only each file's quick hash (first and last MiB, about 27 GB for `2019`, roughly 15 minutes). Files are saved with a stand-in key (`FileHash::Pending`).
- Schema v7: `files.fingerprinted` (existing rows stay fingerprinted). `File::hash` is a `FileHash`: `key()` for the thumbnail cache, `full()` for anything that checks content.
- `fingerprint::JOB_FINGERPRINT` jobs, one per asset, on the job queue; one fingerprint worker in the render service (same pauses as renders). Each checks the file is unchanged (size, date, quick hash), reads it in full, stores the BLAKE3 and moves its thumbnails to the real key. Changed files are reported; a disconnected drive defers the job. Queued by `render::enqueue_backlog` at app start and after every add; `without_jobs` only counts render jobs now.
- Duplicate checks match only real fingerprints. Verify checks unfingerprinted files by size and quick hash (new problem "changed since added") and counts them. Backup copies such a file only if its quick hash still matches, and stores the fingerprint it computed.
- The Folders screen shows "Fingerprinting N files in the background" per folder (refreshed every 3 s while any are left; "to fingerprint when connected" for an unplugged drive). `keepsake verify` and `keepsake add-folder` say so too.

### Tests
- Rust (`fingerprint_tests.rs`, 9 tests): a batched add leaves stand-in keys that are unique and never match content, the job stores the real BLAKE3 for every file and then has nothing to do; an add without batches still reads in full; a file changed after adding (same size, same first and last MiB) is refused and a rescan fixes it; a disconnected drive is deferred; a thumbnail made before the fingerprint moves to the real key; verify by size and quick hash, missing and changed; a backup fingerprints what it copies and a second run finds the copies; a backup refuses a file changed since added; fingerprint jobs do not hide assets from the render backlog. All workspace tests pass.
- Mocked browser (`folders.spec.ts`): after an add the folder shows "Fingerprinting 40 files in the background", and the line goes when done.

---

## Adding a big network folder no longer sits silent before the first save (2026-10-31)

### What was wrong
- Adding `2019` from the network drive (14,268 files, 598 GB of RAW) showed no progress for minutes. Before the first batch was saved, the add listed the folder asking the drive about every file a second time (86 s), read 2,990 Lightroom sidecars one after another (84 s), and matched each sidecar against every file in the folder (52 s in a release build, far longer in the debug app). None of these steps reported progress.

### What was built
- The folder walk uses the details the listing already returned (`DirEntry::metadata`), so a network folder is not asked about each file again: 86 s to 6 s.
- Sidecars are read 8 at a time (`read_sidecars`, standard-library threads, no new dependency): 84 s to 26 s.
- Lightroom `<stem>.xmp` sidecars are matched through an index of files by path without extension, built once: 52 s to under 0.1 s.
- The progress shows "Listing files" and "Reading sidecars" while those run. Debug log lines give each step's time.
- Measured on `2019` with a release build: first save after 33 s instead of 3 min 43 s.

### Still slow by nature
- Every file is read in full to make its BLAKE3 fingerprint. For 598 GB at the 25–30 MB/s the drive gives, that is several hours. Items appear 20 files at a time all along, and a stopped scan carries on.

### Tests
- Rust: `each_lightroom_sidecar_goes_with_its_own_photo` (60 photos with their own Lightroom sidecars and ratings, read in batches, each rating lands on its own photo). `progress_counts_every_file_read` now expects the new step names. All workspace tests pass.

---

## Adding a folder saves as it goes and resumes (2026-10-31)

### What was wrong
- Adding `2010` from the network drive (about 10,600 files) never showed up. The scan read and probed every file, then saved everything in one transaction at the very end. When it did not get there, nothing was kept, and each try began again at the first file. The catalog had no trace of it, so the tree was right not to show it.

### What was built
- `add_folder` with `RebuildOptions::batch_files` (ADR-028). The folder is registered before any file is read. Items are then hashed, probed and saved 20 files at a time, each batch in its own transaction, never splitting an item's files.
- Resume: files saved by an earlier scan with the same size, modification time and sidecar are not read again, so adding the folder again carries on where it stopped. Removed files are dropped at the end. Sequences are found once across the whole folder (`ProbeReport::frames`).
- The app sends `folder://saved` on registration and after every batch. The sidebar, tree and Folders screen refresh on each. The grid re-opens at most every 3 s, and waits while something is selected or the viewer is open. The Folders screen shows "N items in the library so far", and a stopped scan says what was kept.
- `keepsake add-folder` uses the same batches and says how to carry on when stopped. `rebuild-catalog` still reads everything and saves once.

### Tests
- Rust (`add_folder_batch_tests.rs`): saves at 0, 20, 40, 50 items, registered before the first save, progress counts every file; stopping keeps the saved batches and the next scan resumes them without reading; scanning again reads only changed files, drops removed ones, adds new ones; stopping before any save still lists the folder. Existing add-folder, rebuild and auto-import tests still pass.
- Mocked browser (`folders.spec.ts`): a folder is listed in the Folders screen and the sidebar before the scan ends, with its count growing; after a failure the 20 saved items stay, and adding it again finishes at 38.

---

## Information panel in the viewer (2026-10-30)

### What was built
- An Info button at the top right of the viewer, and the I key, show or hide a panel on the right (`ViewerInfo.tsx`). It lists the key facts first (date and time, camera, lens, exposure, ISO, focal length, video duration and frame rate, dimensions, file size, location), then every ExifTool tag, grouped, with a filter and Copy.
- No backend work: the facts come from the catalog's details and the existing `metadata_all` command. The inspector's tag list was split out of `AllMetadata.tsx` as `MetadataTags`, and the read as `useFileMetadata` (`fileMetadata.ts`), so both places share them.
- The panel follows the item shown, and the viewer's JPEG | RAW choice for pairs. Videos show duration and frame rate instead of exposure. On an unplugged drive it shows the catalog's facts and a note, and reads nothing.
- The viewer's body is a two-column grid; the photo stage is measured (ResizeObserver), so Fit, zoom and pan use the narrower area. Open or closed is remembered in `localStorage` (`keepsake.viewer.info`, `viewerInfo.ts`).
- The mock's tags gained a focal length and a Composite image size (the inspector's metadata test now counts 11 tags).

### Tests
- Mocked browser (`viewer.spec.ts`): toggled with the button and I, remembered after a reload (open and closed), follows the item with the arrows, the filter's keys stay in the filter; Fit fits the smaller stage beside the panel and zoom stays centred; JPEG | RAW switches the tags read, a video shows its duration and no exposure, an unplugged item shows the note and reads nothing.
- Real app (`tools.spec.ts`, Windows): a dated JPEG shows its real date and time, 1280 × 800 and ExifTool's File tags, and fits beside the panel; a real clip shows its duration and dimensions.

---

## Calendar: only months and days with pictures (2026-10-30)

### What was built
- The calendar's months level lists only the months that have items, and the days level only the days, in date order. Empty months and days are no longer drawn as greyed-out cells.
- The days level drops the weekday headings and blank padding (they only lined up a full month). Each day reads its weekday and number in the user's language ("Sat 14"), the same label as the sidebar tree (`dayLabel`).
- The backend already returned only non-empty buckets; this is a UI change (`CalendarView.tsx`, `index.css`).

### Tests
- Mocked browser: `library.spec.ts` "calendar shows only the months and days that have pictures" (a 40-item library: fewer than 12 months and 28 days, each with at least one item, in order, none disabled, no weekday headings, weekday in the label). Existing calendar, keyboard and accessibility tests still pass.
- Real app: `app.spec.ts` library test (calendar drill-down) still passes.

---

## Faster viewer zoom, labelled thumbnail sizes (2026-10-30)

### What was built
- **Full size in the background:** 500 ms after a photo opens (drive online), the viewer asks for its full size, so Z, the slider and the wheel are quick. No "Loading full size…" note for this unasked work; the note shows only when zoom is waiting for it. The picture is decoded off the main thread (`Image.decode()`) before it replaces the preview.
- **Upright JPEGs are copied:** `render::full` copies a JPEG with no EXIF orientation (or "upright") into the cache instead of decoding and re-encoding it (`still::is_upright_jpeg`, headers only). Turned JPEGs, PNG, TIFF, HEIF and RAW are unchanged. Originals are only read.
- **Transform-based zoom:** the picture is placed and scaled with a CSS transform from a view of (zoom, picture point at the window's centre), instead of a scrolled, resized `<img>`. Wheel steps are merged to one per frame; the point under the pointer stays put; the picture is centred while smaller than the window and never leaves a gap when larger. The view survives the preview → full-size swap.
- **Thumbnail sizes:** the toolbar switch is labelled "Thumbnails" and its buttons read Small, Medium and Large.

### Tests
- Rust: `still::upright_jpegs_are_told_apart`; `full_size_is_the_whole_picture_upright_and_made_once` now checks that the upright JPEG is a byte-for-byte copy and the turned one is redrawn.
- Mocked browser: `viewer.spec.ts`: the full size arrives by itself at Fit with no note, and zooming then does not fetch again; an offline item is not fetched; panning and wheel zoom measured from the picture's position. `thumb-size.spec.ts`: the labelled group and button names.
- Real app: new `zoom-perf.spec.ts` (24 MP JPEG: full size ready in 0.86 s, slider step 90th percentile 34 ms; limits 3 s and 50 ms, doubled in debug); `tools.spec.ts` viewer test still passes.

---

## Thumbnail sizes and the viewer's zoom slider (2026-10-30)

### What was built
- **Grid:**
  - Small (120 px), Medium (176 px) and Large (280 px) tiles. A switch in the toolbar (Grid view only), or + and − in the grid. Remembered on this computer (`localStorage` key `keepsake.thumbSize`).
  - Large tiles, and tiles a high-DPI screen draws with more than 300 pixels, use the 1024 px thumbnail (`Row.thumbLarge`), falling back to the 256 px one until it is made.
  - The focused item stays in view when the size changes.
- **Viewer (photos):**
  - A zoom bar under the picture: Fit, −, a slider from Fit to 400% (a log scale, so each part of it feels the same), +, 100%, and the level ("Fit (23%)", "152%").
  - + and − step by 25%; the wheel zooms around the pointer; Z and a click still switch between Fit and 100%.
  - Zoom keeps the point under the pointer (or the centre) in place. The picture is centred while smaller than the window, and never cut off at the top or left when larger.
  - Once the full size is made it is also shown at Fit (sharper on large screens). With the drive unplugged, the preview is zoomed.
  - Arrow keys on a slider (zoom or video position) move the slider, not to another item.

### Tests
- Mocked browser: `thumb-size.spec.ts` (switch, sizes, 1024 px thumbnail for Large, remembered after a reload, + and −, focused item kept in view, only in Grid view); `viewer.spec.ts` (slider to 400%, − and + steps, 100% is one picture pixel per screen pixel, Fit fits, slider middle, arrows on the slider, wheel zoom around the middle and back to Fit).
- Real app: Large loads the 1024 px thumbnail from the cache and Medium the 256 px one; the existing viewer test (Z to 100% on a real JPEG) still passes.

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
