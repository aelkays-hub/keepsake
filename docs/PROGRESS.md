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
| 2 | Device watcher — card/camera detection on Windows and macOS | ✅ Done in prompt 8 (MTP cameras: v1) |
| 2 | Sony card layout parser and asset grouper | ✅ Done in prompt 5 |
| 3 | Probe: ExifTool integration and Sony XML sidecar reader | ✅ Done in prompt 5 |
| 4 | Ingest engine: copy with streaming BLAKE3 and verification | ✅ Done in prompt 6 |
| 5 | Catalog: SQLite schema, migrations, WAL/FTS5 setup | ✅ Done in prompt 4 |
| 6 | XMP sidecar read/write (ratings, labels, comments, date shift) | ✅ Done in prompt 4 |
| 7 | Thumbnail and preview generation via FFmpeg/LibRaw | ⬜ Pending |
| 8 | Tauri command layer: ingest, catalog queries, progress events | ⬜ Pending |
| 9 | UI: calendar view | ⬜ Pending |
| 10 | UI: grid view with TanStack Virtual | ⬜ Pending |
| 11 | UI: details view and metadata panel | ⬜ Pending |
| 12 | UI: ingest wizard (preview, destination, confirm) | ⬜ Pending |
| 13 | Search: date, camera, lens, type, text (FTS5) | 🟡 CLI done in prompt 7 (no lens filter, no UI) |
| 14 | Video proxy generation and playback | ⬜ Pending |
| 15 | Backup verification command (CLI + UI) | 🟡 CLI done in prompt 7 (`verify`, `backup`); UI pending |
| 16 | Lossless trim (keyframe-cut and smart-cut) | ⬜ Pending |
| 17 | Combine, rotate, frame grab, MP4 conversion | ⬜ Pending |
| 18 | Image adjustments (crop, resize, red-eye, auto-levels) | ⬜ Pending |
| 19 | Flickr export connector | ⬜ Pending |
| 20 | Map view (OpenStreetMap + GPX) | ⬜ Pending |
| 21 | On-device AI: tagging, face clustering, semantic search | ⬜ Pending |
| 22 | Multi-view player and time-lapse | ⬜ Pending |

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
