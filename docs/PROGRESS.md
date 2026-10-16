# Keepsake — Progress Log

| Prompt | Description | Status |
|--------|-------------|--------|
| 0 | Repository bootstrap: CLAUDE.md, docs, workspace scaffold, setup scripts, CI, TEST_ME.md | ✅ Done |
| 1 (this) | Test corpus: `keepsake corpus scan/init-expected`, fixtures, test harness | ✅ Done |
| 2 | Device watcher — card/camera detection on Windows and macOS | ⬜ Pending |
| 2 | Sony card layout parser and asset grouper | ⬜ Pending |
| 3 | Probe: ExifTool integration and Sony XML sidecar reader | ⬜ Pending |
| 4 | Ingest engine: copy with streaming BLAKE3 and verification | ⬜ Pending |
| 5 | Catalog: SQLite schema, migrations, WAL/FTS5 setup | ⬜ Pending |
| 6 | XMP sidecar read/write (ratings, labels, comments, date shift) | ⬜ Pending |
| 7 | Thumbnail and preview generation via FFmpeg/LibRaw | ⬜ Pending |
| 8 | Tauri command layer: ingest, catalog queries, progress events | ⬜ Pending |
| 9 | UI: calendar view | ⬜ Pending |
| 10 | UI: grid view with TanStack Virtual | ⬜ Pending |
| 11 | UI: details view and metadata panel | ⬜ Pending |
| 12 | UI: ingest wizard (preview, destination, confirm) | ⬜ Pending |
| 13 | Search: date, camera, lens, type, text (FTS5) | ⬜ Pending |
| 14 | Video proxy generation and playback | ⬜ Pending |
| 15 | Backup verification command (CLI + UI) | ⬜ Pending |
| 16 | Lossless trim (keyframe-cut and smart-cut) | ⬜ Pending |
| 17 | Combine, rotate, frame grab, MP4 conversion | ⬜ Pending |
| 18 | Image adjustments (crop, resize, red-eye, auto-levels) | ⬜ Pending |
| 19 | Flickr export connector | ⬜ Pending |
| 20 | Map view (OpenStreetMap + GPX) | ⬜ Pending |
| 21 | On-device AI: tagging, face clustering, semantic search | ⬜ Pending |
| 22 | Multi-view player and time-lapse | ⬜ Pending |

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
