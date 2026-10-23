# Manual Test Steps — Prompt 6: Ingest

Test on: **Windows 11 x64** (primary) and **macOS 14+** when available.
Run from the repo root. Build first: `cargo build -p keepsake-cli`.
Below, `$K` is `.\target\debug\keepsake.exe` (Windows) or `./target/debug/keepsake` (macOS).

The safest real test uses an actual card (or a USB stick holding a copy of one) and **two separate drives** as destinations. Sections B–E use folders on one disk instead; that needs the hidden testing flag `--unsafe-allow-same-disk`.

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass, including `ingest_tests` (24 passed, 1 ignored: the ignored one is the child process the kill test starts).
2. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings` — **Expected:** no output.

---

## B. A normal import (one disk, testing flag)

1. Make a test card and library:
   ```powershell
   $T = "$env:TEMP\keepsake-ingest"; New-Item -ItemType Directory -Force "$T\card\PRIVATE\M4ROOT\CLIP" | Out-Null
   Copy-Item -Recurse "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4\DCIM" "$T\card\"
   Copy-Item "$env:KEEPSAKE_CORPUS\cards\ILCE-7M4\PRIVATE\M4ROOT\CLIP\C0006*" "$T\card\PRIVATE\M4ROOT\CLIP\"
   & $K library init "$T\lib"
   ```
2. Without the flag: `& $K ingest start "$T\lib" "$T\card" --to "$T\driveA" --to "$T\driveB"`
   **Expected:** `refused: both destinations are on the same disk; two copies must be on two disks`. Nothing is copied.
3. With the flag: add `--unsafe-allow-same-disk`.
   **Expected:** a testing warning, then `46 assets: 46 catalogued`, then `SAFE TO FORMAT: every selected asset is verified on two destinations.`
4. Open `driveA\2026\2026-10-14\`. **Expected:**
   - The camera's file names are kept (default template), with `A7404967.JPG` and `A7404967.ARW` side by side.
   - `C0006.MP4` sits next to `C0006M01.XML`.
   - Each original has a `.xmp` sidecar.
   - There are no hidden `.keepsake-partial-` files (show hidden files to check).
5. Run the same command again. **Expected:** `0 … copied` (no MB line) and still SAFE TO FORMAT; every asset is already imported, confirmed by full hash.
6. Check the card was not changed: the file sizes and dates in `$T\card` are the same as before.

---

## C. Interrupt and resume

1. `& $K library init "$T\lib2"`, then start an import to `$T\k1` and `$T\k2` (with the flag) and close the window or press **Ctrl+C** a few seconds in.
2. `& $K ingest status "$T\lib2"` — **Expected:** some assets `copying` or `selected`, **NOT safe to format**, and possibly `… unfinished temporary file(s) remain`.
3. `& $K ingest resume "$T\lib2"` — **Expected:** `46 assets: 46 catalogued` and SAFE TO FORMAT. No hidden `.keepsake-partial-` files remain in `k1` or `k2`.

---

## D. Fewer than two copies

1. `& $K library init "$T\lib3"` then `& $K ingest start "$T\lib3" "$T\card" --to "$T\only" --unsafe-allow-same-disk`
   **Expected:** everything catalogued, but `NOT safe to format: - only one destination was chosen`.

---

## E. Templates

1. `& $K library init "$T\lib4"` then:
   ```powershell
   & $K ingest start "$T\lib4" "$T\card" --to "$T\t1" --to "$T\t2" --unsafe-allow-same-disk --template "{yyyy}-{mm}-{dd}/{event}/{hh}_{seq}_{orig}" --event "Lake: evening"
   ```
2. **Expected:** folder `2026-10-14\Lake_ evening\` (the `:` replaced). Files are named like `17_0001_A7404966.ARW`, and `C0006M01.XML` follows its clip as `18_…_C0006M01.XML`.

---

## F. Real card, two real drives (the real safety check)

1. Insert a card (or a USB stick with a card copy). Connect two external drives.
2. `& $K library init "$T\real"` then `& $K ingest start "$T\real" E:\ --to F:\Photos --to G:\Photos` (use your drive letters).
   **Expected:** no refusal (three different disks) and SAFE TO FORMAT at the end.
3. Pull **one destination drive** out mid-copy, then plug it back in and run `ingest resume`.
   **Expected:** the first run reports failed assets and NOT safe; `resume` completes and reports SAFE.
4. Pull the **card** mid-copy. **Expected:** `the source card is not available …` and NOT safe. Reinsert it (same drive letter) and `resume` completes.
5. Before formatting: open a few files from each drive.

---

## G. macOS

1. Run section A. **Expected:** pass. The read-only test uses `chmod`.
2. Repeat B with folders on an APFS disk, and F with two external drives (`/Volumes/…`).
3. Clean up: delete `$T` (and the test folders on your drives).
