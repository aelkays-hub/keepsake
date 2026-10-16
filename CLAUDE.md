# Keepsake — Engineering Guide

## Product

Keepsake is a **local-first desktop application** for Windows 11 (x64 and ARM64) and macOS 14+ (Apple Silicon) that replaces Sony PlayMemories Home.

Core capabilities:
- Camera-card and USB-camera detection with hash-verified ingest
- Calendar-first media library with file-by-template renaming
- Lossless video tools: trim (keyframe and smart-cut), combine, frame grab (16-bit for 10-bit sources), rotate (metadata-only)
- MP4 conversion with hardware encoders
- Multi-view player (up to 4 synced clips with music, captions, speed)
- Time-lapse with deflicker
- Flickr export
- OpenStreetMap map view with GPX import
- On-device AI tagging, face clustering, and semantic search

---

## Non-Negotiables

### a. Original media is immutable
Original media files are **never modified, moved, or deleted** by the application. All user edits live in XMP sidecars, JSON recipes, or new derivative files. The original is sacred.

### b. Every feature ships with tests
Nothing is considered done until automated tests pass on **both Windows and macOS in CI**. No feature lands on `main` without a green CI run.

### c. Dependencies require approval
Ask the product owner before adding any dependency. Every accepted dependency is recorded in `docs/DECISIONS.md` with its licence and reason. **No GPL dependencies may be linked into the binary.** FFmpeg is an LGPL build invoked as a sidecar process, never linked.

### d. No secrets or media in the repo
Never commit API keys, secrets, or personal media. Test media is read from the path in the `KEEPSAKE_CORPUS` environment variable and is never copied into the repository. If `KEEPSAKE_CORPUS` is unset, corpus-dependent tests are skipped.

### e. Branch and commit discipline
Work on a **feature branch per prompt**. Make small, focused commits with clear messages. Never force-push `main`.

### f. End-of-session docs
End every working session by:
1. Updating `docs/PROGRESS.md` with what was done.
2. Writing (or overwriting) `TEST_ME.md` with plain-English manual test steps and expected results for both OSes.

---

## Stack

| Layer | Choice |
|---|---|
| Language | Rust stable, 2021 edition, Cargo workspace |
| Desktop framework | Tauri 2 |
| UI | React 18 + TypeScript (strict) + Vite + TanStack Virtual |
| Database | SQLite via `rusqlite` (bundled feature), WAL mode, FTS5 |
| Media metadata | ExifTool in `-stay_open` mode |
| Raw decoding | LibRaw and libheif via Rust bindings |
| Video | FFmpeg / ffprobe as LGPL sidecar binaries |
| Hashing | BLAKE3 (verification) and xxHash3 (quick-diff) |
| Error handling | `thiserror` in libraries; `anyhow` only in binaries |
| Logging | `tracing` with structured spans |

---

## Repository Layout

```
keepsake/
├── Cargo.toml                  # workspace root
├── CLAUDE.md                   # this file
├── TEST_ME.md                  # manual test steps (regenerated each session)
├── crates/
│   ├── keepsake-core/          # all domain logic, zero UI
│   ├── keepsake-platform/      # OS adapters behind traits (cfg-gated windows/macos)
│   └── keepsake-cli/           # CLI binary
├── app/
│   ├── src-tauri/              # thin Tauri command layer over keepsake-core
│   └── ui/                     # React + TypeScript frontend
├── docs/
│   ├── SPEC.md
│   ├── ARCHITECTURE.md
│   ├── DECISIONS.md
│   └── PROGRESS.md
├── scripts/
│   ├── setup.sh                # macOS/Linux dev setup
│   └── setup.ps1               # Windows dev setup
├── tests/                      # cross-crate integration tests
└── vendor/                     # gitignored downloaded sidecar binaries
```

`vendor/` is listed in `.gitignore`. The setup scripts populate it.

---

## Conventions

- **No `unwrap()` or `expect()` in non-test `keepsake-core` code.** Use `?` and `thiserror`.
- `cargo fmt` and `cargo clippy -- -D warnings` must be clean before every commit.
- ESLint and Prettier must be clean in `app/ui`.
- Long-running work (ingest, render, export) runs on the **job queue**, never on the UI thread or a Tauri command thread.
- All filesystem paths stored in the catalog are relative to a **volume UUID**, never as absolute paths, so the library is portable across mount points.
- Tauri commands are thin: validate input, call into `keepsake-core`, return a serialisable result. No business logic in `src-tauri`.

---

## Commands

### Prerequisites

Run `scripts/setup.sh` (macOS) or `scripts/setup.ps1` (Windows) once. This installs Rust stable, Node LTS, pnpm, and downloads pinned FFmpeg + ExifTool into `vendor/`.

### Build

```bash
# Rust workspace
cargo build --workspace

# UI only
cd app/ui && pnpm install && pnpm build

# Full Tauri app (debug)
cd app/ui && pnpm tauri dev

# Full Tauri app (release)
cd app/ui && pnpm tauri build
```

### Test

```bash
# All Rust tests (includes fixture tests)
cargo test --workspace

# Corpus-dependent tests (requires KEEPSAKE_CORPUS set)
$env:KEEPSAKE_CORPUS = "C:\path\to\corpus"
cargo test --workspace -- --include-ignored corpus

# UI tests
cd app/ui && pnpm test
```

### Test corpus

The project keeps two kinds of tests:

**Fixture tests** (`crates/keepsake-core/tests/fixture_tests.rs`) always run in CI.
They verify the scanner, role detector, and grouper against committed synthetic stubs
in `tests/fixtures/`. Stubs are tiny text files (sufficient for path-based tests).
Run `scripts/make-fixtures.ps1` (Windows) or `scripts/make-fixtures.sh` (macOS/Linux)
to regenerate them as real tiny FFmpeg-generated MP4s (requires `vendor/ffmpeg`).

**Corpus tests** (`crates/keepsake-core/tests/corpus_tests.rs`) are `#[ignore]` by default.
They need real Sony card media. Set up the corpus layout once:

```
$KEEPSAKE_CORPUS/
  cards/
    <camera-name>/   ← one subdirectory per camera card copy
      PRIVATE/
        M4ROOT/
          CLIP/
          SUB/
      DCIM/
      ...
```

Then:

```bash
# 1. Scan your cards and write corpus/manifest.json
keepsake corpus scan $KEEPSAKE_CORPUS

# 2. Generate corpus/expected.toml with inferred asset groups
keepsake corpus init-expected --force

# 3. Edit corpus/expected.toml — fill in capture_ts / camera_model / duration_ms
#    and set confirmed = true for assets you've verified against the camera

# 4. Commit both files
git add corpus/ && git commit -m "chore: update corpus manifest"

# 5. Run corpus tests
cargo test --workspace -- --include-ignored corpus
```

### Lint

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cd app/ui && pnpm lint
```

### CLI

```bash
cargo run -p keepsake-cli -- --version
cargo run -p keepsake-cli -- ingest --help
```

---

## Definition of Done

A feature is **done** when all of the following are true:

1. Code is written and compiles without warnings.
2. Automated tests cover the new behaviour and pass locally.
3. `cargo fmt`, `cargo clippy -D warnings`, ESLint, and Prettier are all clean.
4. CI is green on **both** `windows-latest` and `macos-14`.
5. `docs/PROGRESS.md` is updated.
6. `TEST_ME.md` is written with manual test steps and expected results for both OSes.
7. Relevant documentation (`SPEC.md`, `ARCHITECTURE.md`, `DECISIONS.md`) is updated if the change affects them.
