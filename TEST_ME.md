# TEST_ME — Bootstrap (Prompt 0)

Manual verification steps for the bootstrap build. Perform these on both **Windows 11** and **macOS 14+** unless noted.

---

## Prerequisites

Before testing, run the setup script for your OS:

**macOS:**
```bash
bash scripts/setup.sh
```

**Windows (PowerShell, as a normal user):**
```powershell
.\scripts\setup.ps1
```

Expected result: Script completes without errors. `vendor/` contains `ffmpeg` (or `ffmpeg.exe`), `ffprobe` (or `ffprobe.exe`), and the `exiftool` folder (or `exiftool.exe`).

---

## Test 1 — Rust workspace builds clean

```bash
cargo build --workspace
```

**Expected**: Build succeeds with zero errors and zero warnings. You see a line like:
```
Finished `dev` profile [unoptimized + debuginfo] target(s) in ...
```

---

## Test 2 — Rust tests pass

```bash
cargo test --workspace
```

**Expected**: All tests pass. Output ends with something like:
```
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

The one test that runs is `keepsake_core::tests::version_is_semver`, which checks that the version string has the form `MAJOR.MINOR.PATCH`.

---

## Test 3 — CLI prints the version

```bash
cargo run -p keepsake-cli -- --version
```

**Expected**: Output is exactly:
```
keepsake 0.1.0
```

Also try the short flag:
```bash
cargo run -p keepsake-cli -- -V
```

**Expected**: Same output.

---

## Test 4 — UI dependencies install

```bash
cd app/ui
pnpm install
```

**Expected**: `node_modules/` is populated. No peer-dependency errors or audit failures that block the install.

---

## Test 5 — UI type-check passes

```bash
cd app/ui
pnpm build
```

**Expected**: TypeScript compiles without errors, Vite builds the production bundle into `app/ui/dist/`. Final line looks like:
```
✓ built in ...
```

---

## Test 6 — UI linting is clean

```bash
cd app/ui
pnpm lint
pnpm format:check
```

**Expected**: Both commands exit with code 0 and print no errors or warnings.

---

## Test 7 — Tauri app runs (optional; requires full setup)

If you have Tauri's system dependencies installed (WebView2 on Windows; Xcode command-line tools on macOS), start the dev server:

```bash
cd app/ui
pnpm tauri dev
```

**Expected**:
- A window titled **Keepsake** opens.
- The window shows the heading "Keepsake" in large text.
- Below it, the version number appears as **v0.1.0**.
- Below that, the subtitle "Your local-first photo and video library." is shown.

If you see "Could not load version: …" in red, the Tauri IPC bridge is not connected — this only happens when the UI is opened directly in a browser, not through `tauri dev`.

---

## Test 8 — cargo fmt and clippy are clean

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
```

**Expected**: Both commands exit 0 with no output (fmt) or only informational output (clippy). Zero errors or warnings.

---

## Notes

- The `vendor/` directory is gitignored. SHA-256 hashes in the setup scripts are placeholders that will be replaced with real values when the download URLs are confirmed and the binaries are verified manually.
- The Tauri app icons in `app/src-tauri/icons/` are stubs. The `.gitkeep` file holds the directory; real icons are generated in a later prompt.
- No corpus-dependent tests exist yet. When `KEEPSAKE_CORPUS` is set and real Sony card media is present, run `cargo test --workspace -- --include-ignored corpus` — this is a no-op at prompt 0 since no corpus tests exist.
