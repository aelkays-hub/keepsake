# Manual Test Steps — Prompt 3: Packaging

Test on: **Windows 11 x64** (primary). macOS deferred.

---

## A. OS data directory paths

**Goal:** `keepsake_core::paths` returns the expected Windows paths.

1. Open a PowerShell terminal in the repo root.
2. Run:
   ```
   cargo test -p keepsake-core -- paths
   ```
3. **Expected:** All three tests pass:
   - `paths::tests::data_dir_is_some_and_ends_with_keepsake` ✓
   - `paths::tests::cache_dir_is_some_and_ends_with_keepsake` ✓
   - `paths::tests::config_dir_is_some_and_ends_with_keepsake` ✓
4. Add a quick manual check in a Rust scratch test or by reading the source:
   - `data_dir()` should return `C:\Users\<you>\AppData\Local\keepsake`
   - `cache_dir()` should return `C:\Users\<you>\AppData\Local\keepsake`  
     _(Windows: `dirs::cache_dir` returns LocalAppData, same as data_local_dir)_
   - `config_dir()` should return `C:\Users\<you>\AppData\Roaming\keepsake`

---

## B. FFmpeg and ExifTool declared as bundle resources

**Goal:** `tauri.conf.json` declares all four sidecars; CI check passes.

1. Run the CI resource-declaration check locally:
   ```powershell
   $conf = Get-Content app/ui/src-tauri/tauri.conf.json | ConvertFrom-Json
   $keys = $conf.bundle.resources.PSObject.Properties.Name
   $keys
   ```
2. **Expected output includes:**
   ```
   ../../../vendor/exiftool/exiftool.exe
   ../../../vendor/exiftool/exiftool_files/
   ../../../vendor/ffmpeg.exe
   ../../../vendor/ffprobe.exe
   ```
3. Run `cargo test --workspace` and confirm the existing CI fixture tests still pass.

---

## C. Release workflow syntax (dry-run)

**Goal:** The `release.yml` workflow file parses without errors.

1. Install `actionlint` if available, or use the GitHub Actions UI after pushing.
2. Push the `packaging` branch to GitHub:
   ```
   git push -u origin packaging
   ```
3. Open the PR. Confirm that `ci.yml` runs green on the branch.
4. The `release.yml` workflow does **not** run on branch push — it only runs on tags. 
   To confirm it is syntactically valid, use the GitHub "Actions" tab → select the workflow → "This workflow has a workflow_dispatch trigger" (it does not, but GitHub will still validate the YAML).  
   Alternatively: `gh workflow view release.yml` should show the workflow without error.

---

## D. Simulate release workflow steps locally (Windows x64 only)

**Goal:** Confirm the vendor-download PowerShell block works.

1. Ensure `vendor/` is empty or absent (or test in a clean temp folder).
2. Run the download block from `release.yml` in PowerShell:
   ```powershell
   $env:MIRROR = "https://github.com/aelkays-hub/keepsake/releases/download/vendor-binaries-v1"
   # ... (paste the "Download vendor binaries — x64 FFmpeg + ExifTool" step)
   ```
3. **Expected:** `vendor\ffmpeg.exe`, `vendor\ffprobe.exe`, `vendor\exiftool\exiftool.exe` all exist.
4. Verify FFmpeg: `vendor\ffmpeg.exe -version` — should print version + no `--enable-gpl`.
5. Verify ExifTool: `vendor\exiftool\exiftool.exe -ver` — should print `13.59`.

---

## E. Build installer locally

**Goal:** `pnpm tauri build` produces MSI and NSIS installers.

**Prerequisites:** Run `scripts\setup.ps1` first so `vendor\ffmpeg.exe` and `vendor\exiftool\` are populated.

1. From repo root:
   ```
   cd app\ui
   pnpm install --frozen-lockfile
   pnpm tauri build --bundles msi,nsis
   ```
2. **Expected:** Build completes without errors. Installers at:
   - `target\release\bundle\msi\Keepsake_0.1.0_x64_en-US.msi`
   - `target\release\bundle\nsis\Keepsake_0.1.0_x64-setup.exe`
3. Install the MSI silently:
   ```powershell
   msiexec /i "target\release\bundle\msi\Keepsake_0.1.0_x64_en-US.msi" /quiet /norestart
   ```
4. Verify: `C:\Program Files\Keepsake\keepsake-app.exe` exists.
5. Verify sidecars bundled: `C:\Program Files\Keepsake\ffmpeg\ffmpeg.exe` and `C:\Program Files\Keepsake\exiftool\exiftool.exe` exist.
6. Launch `keepsake-app.exe` — Keepsake window opens with placeholder screen. ✓
7. Uninstall via **Settings → Apps**.

---

## F. CLI smoke test

**Goal:** `keepsake` CLI binary works as a standalone executable.

1. Build CLI: `cargo build --release -p keepsake-cli`
2. Run: `target\release\keepsake.exe --version`
   - **Expected:** prints `keepsake 0.1.0` (or current version).
3. Run: `target\release\keepsake.exe corpus scan tests/fixtures/`
   - **Expected:** exits 0; may print an empty camera list (fixtures don't match full Sony card layout).

---

## G. Release instructions (scripts/release.md)

1. Read `scripts/release.md`.
2. Confirm the steps are clear and match the actual workflow in `.github/workflows/release.yml`.
3. **Expected:** No contradictions between the doc and the YAML.

---

## Notes

- **No real tag push needed for testing** — the release workflow will not run unless you push a `v*.*.*` tag to the repo.
- **Unsigned builds are expected** until `WINDOWS_CERTIFICATE` is configured as a repo secret.
- macOS DMG is deferred; the job is present as a commented-out block in `release.yml`.
