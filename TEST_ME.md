# Manual Test Steps — Prompt 5: Probing

Test on: **Windows 11 x64** (primary) and **macOS 14+** (Apple Silicon) when available.
Run commands from the repo root (PowerShell on Windows, Terminal on macOS).

You need the vendored tools. Run once:

- **Windows:** `./scripts/setup.ps1 -VendorOnly`
- **macOS:** `./scripts/setup.sh` (ExifTool only; macOS FFmpeg is still deferred, so the ffprobe steps cannot run there yet)

Build the CLI: `cargo build -p keepsake-cli`. Below, `$K` is `.\target\debug\keepsake.exe` (Windows) or `./target/debug/keepsake` (macOS).

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** everything passes, including `probe_tests` (12 passed) and `rebuild_tests` (9 passed).
2. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no output.

---

## B. Probe your corpus card

1. Run: `& $K probe "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4"`
2. **Expected:**
   - First line: `48 assets, 2 card files, 0 other files`.
   - Each still shows a time like `2026-10-14T17:56:12.285-05:00  [exif / high]  ILCE-7M4  24-70mm F2.8 DG DN | Art 019`.
   - `A7404967.JPG` lists `+ DCIM/10061014/A7404967.ARW (raw_pair)`.
   - Each clip (`C0006`–`C0008`) shows `[sony_xml / high]`, a duration such as `9.509 s`, `h264 1920x1080 8-bit Yuv420`, and a timecode. It also lists its `M01.XML` and `THMBNL/…T01.JPG` as sidecars.
   - At the end: `burst of 18 …`, `burst of 5 …` and five `bracket of 3 …` lines.
3. Spot-check against the camera: pick 3 stills and 1 clip, view them on the camera (or its info display), and compare date, time and model. **Expected:** identical, with the time in the camera's local zone (−05:00).
4. `& $K probe "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4" --json > probe.json` — **Expected:** valid JSON with `groups`, `sequences` and `card_files` (`DATABASE.BIN`, `MEDIAPRO.XML`). Delete `probe.json` afterwards.

---

## C. Confirm corpus entries (needed for the corpus test)

`corpus/expected.toml` has been regenerated and pre-filled. Every entry shows `prefilled = true` and `confirmed = false`.

1. Run: `cargo test -p keepsake-core --test corpus_tests -- --include-ignored corpus`
   **Expected now:** `corpus_probes_cleanly` passes; `corpus_matches_every_confirmed_entry` **fails** with "no confirmed entries" (by design).
2. For each asset you checked on the camera in B.3: edit `corpus/expected.toml`, check `capture_ts`, `camera_model` and `duration_ms` (clips), then set `confirmed = true`. Fix any value that is wrong instead of confirming it.
3. Run the test again. **Expected:** `Verified N confirmed asset(s)` and both tests pass. If a value differs, the failure names the files and shows the expected and actual values.
4. Commit `corpus/expected.toml`.

---

## D. Index a card into a library (uses probing)

1. Make a test copy, so nothing is written to your corpus:
   - **Windows:**
     ```powershell
     $T = "$env:TEMP\keepsake-probe"; Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue
     New-Item -ItemType Directory "$T\card\PRIVATE\M4ROOT\CLIP" | Out-Null
     Copy-Item -Recurse "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4\DCIM" "$T\card\"
     Copy-Item "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4\PRIVATE\M4ROOT\CLIP\C0008*" "$T\card\PRIVATE\M4ROOT\CLIP\"
     ```
2. `& $K library init "$T\lib"` then `& $K library add-volume "$T\lib" "$T\card" --label "A7IV card" --kind card`
   **Expected:** `46 assets (46 probed), … 7 sequences` (45 stills + clip C0008).
3. `& $K library stats "$T\lib"` — **Expected:** `Days:` shows `2026-10-14  46` (the calendar uses the probed local date).
4. `& $K library edit "$T\lib" "$T\card\DCIM\10061014\A7404971.ARW" --rating 3`, then open `A7404971.ARW.xmp`.
   **Expected:** it contains `keepsake:CaptureSource` `exif`, `keepsake:CaptureConfidence` `high`, and a `keepsake:Sequence` with `Kind` `burst`.
5. `& $K library rebuild "$T\lib" --fresh --no-probe` then `stats`.
   **Expected:** 46 assets and `Days:` shows **1** asset: only A7404971 got a date back, from its sidecar. Without probing, unedited assets have no date. Run `library rebuild "$T\lib" --fresh` (with probing) and all 46 return.
6. Delete `$T`.

---

## E. Without the tools

1. Rename `vendor\exiftool` temporarily, then run `& $K probe "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4"`.
   **Expected:** a clear error saying ExifTool was not found, mentioning the setup script.
2. `library add-volume` in that state prints `note: not probing media …` and still indexes the files.
3. Rename the folder back.

---

## F. macOS-specific checks

1. Run section A with `vendor/exiftool/exiftool` present. **Expected:** the ExifTool tests pass. The ffprobe tests and the end-to-end card test skip locally until macOS FFmpeg is added (on CI they would fail, which is intended).
2. Run `& $K probe` on a card copy whose folder names contain accents (APFS stores them as typed). **Expected:** the same results as on Windows.
