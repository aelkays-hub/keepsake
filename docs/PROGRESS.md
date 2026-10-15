# Keepsake — Progress Log

| Prompt | Description | Status |
|--------|-------------|--------|
| 0 | Repository bootstrap: CLAUDE.md, docs, workspace scaffold, setup scripts, CI, TEST_ME.md | ✅ Done |
| 1 | Device watcher — card/camera detection on Windows and macOS | ⬜ Pending |
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
