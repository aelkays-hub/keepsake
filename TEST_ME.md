# Manual Test Steps — Prompt 4: Catalog

Test on: **Windows 11 x64** (primary) and **macOS 14+** (Apple Silicon) when available.
Commands are run from the repo root. On Windows use PowerShell; on macOS use Terminal.

You need a folder of test files to act as a "card". The repo's fixtures work:
they are tiny stand-in files, not real media, which is enough to test the catalog.

---

## A. Automated tests

1. Run:
   ```
   cargo test --workspace
   ```
2. **Expected:** every suite passes, including:
   - `catalog_tests` — 15 passed
   - `xmp_tests` — 10 passed
   - `rebuild_tests` — 9 passed
   - `path_props` — 6 passed
3. Run the linters:
   ```
   cargo fmt --check
   cargo clippy --workspace --all-targets -- -D warnings
   ```
   **Expected:** no output and no errors.

---

## B. Create a library and index a card

1. Build the CLI: `cargo build -p keepsake-cli`
2. Make a test card from the fixtures:
   - **Windows:**
     ```powershell
     $T = "$env:TEMP\keepsake-test"; Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue
     New-Item -ItemType Directory "$T\card" | Out-Null
     Copy-Item -Recurse tests\fixtures\xavc\* "$T\card\"
     Copy-Item -Recurse tests\fixtures\dcim\* "$T\card\"
     $K = ".\target\debug\keepsake.exe"
     ```
   - **macOS:**
     ```bash
     T=/tmp/keepsake-test; rm -rf $T; mkdir -p $T/card
     cp -R tests/fixtures/xavc/* tests/fixtures/dcim/* $T/card/
     K=./target/debug/keepsake
     ```
3. Create the library: `& $K library init "$T\lib"` (macOS: `$K library init $T/lib`)
   **Expected:** `Created library <uuid> at …`. The folder contains `library.json`, `volumes.json`, `catalog.db`, `recipes/`, `sidecars/`.
4. Add the card: `& $K library add-volume "$T\lib" "$T\card" --label "Test card" --kind card`
   **Expected:** `Volume Test card (…) - writable, case-insensitive` then `4 assets, 10 files indexed`.
   (2 XAVC clips with their XML and proxy, plus 2 DCIM stills. DSC00001's JPG, ARW and HIF count as one asset.)
5. `& $K library stats "$T\lib"`
   **Expected:** `Assets: 4`, `Files: 10`, the volume shown as online. "Days" is empty for now: capture dates come from ExifTool, which isn't connected yet.

---

## C. Edit metadata and check the sidecar

1. Run:
   ```
   & $K library edit "$T\lib" "$T\card\DCIM\100MSDCF\DSC00001.JPG" --rating 4 --label Red --comment "Café test" --tag "Travel|Japan" --tag Family
   ```
   **Expected:** `Updated asset …` and one sidecar `DCIM/100MSDCF/DSC00001.JPG.xmp (beside)`.
2. Open `card\DCIM\100MSDCF\DSC00001.JPG.xmp` in a text editor.
   **Expected:** you see `<xmp:Rating>4</xmp:Rating>`, `<xmp:Label>Red</xmp:Label>`, `Café test`, `Travel|Japan` under `lr:hierarchicalSubject` and `Travel/Japan` under `digiKam:TagsList`.
3. Confirm the JPG itself did not change: its size and modified date are the same as in `tests\fixtures\dcim\DCIM\100MSDCF\`.
4. `& $K library search "$T\lib" japan` and `… search "$T\lib" cafe`
   **Expected:** both find `DCIM/100MSDCF/DSC00001.JPG rating=4 label=Red tags=[Family, Travel|Japan]`. "cafe" matches "Café" because accents are ignored.
5. Try a shift: `& $K library edit "$T\lib" "$T\card\DCIM\100MSDCF\DSC00001.JPG" --shift-seconds -3600`
   **Expected:** an error saying the asset has no capture time (expected until ExifTool is connected).

---

## D. Delete the catalog and rebuild

1. `& $K library rebuild "$T\lib" --fresh`
   **Expected:** `Deleted catalog; rebuilding from files and sidecars.` then `4 assets, 10 files indexed, 1 sidecars read`.
2. Repeat C.4 (search).
   **Expected:** same result: rating, label, tags and comment came back from the sidecar.
3. Delete `catalog.db` (and any `catalog.db-wal` / `catalog.db-shm`) by hand, then run `library rebuild` without `--fresh`.
   **Expected:** same result again.

---

## E. Read-only volume

1. Make a second folder with a copy of a photo, then add it as read-only:
   - **Windows:**
     ```powershell
     New-Item -ItemType Directory "$T\backup" | Out-Null
     Copy-Item tests\fixtures\dcim\DCIM\100MSDCF\DSC00002.JPG "$T\backup\"
     & $K library add-volume "$T\lib" "$T\backup" --label Backup --read-only
     & $K library edit "$T\lib" "$T\backup\DSC00002.JPG" --rating 2
     ```
   - **macOS:** the same with `/` paths.
2. **Expected:** the edit lists a sidecar marked `(mirror)`. No `.xmp` file appears in `backup\`. A file `lib\sidecars\<volume-uuid>\DSC00002.JPG.xmp` exists.
3. `& $K library rebuild "$T\lib" --fresh` then `& $K library stats "$T\lib"`.
   **Expected:** both volumes listed. `search "$T\lib" DSC00002` shows two results: the card copy (`rating=0`) and the backup copy (`rating=2`). They are separate assets because nothing links them yet; ingest will link copies by hash in a later prompt.

---

## F. Lightroom / digiKam interop (optional)

1. Copy `tests\fixtures\xmp\lightroom.xmp` to `$T\card\DCIM\100MSDCF\DSC00002.JPG.xmp`, then run `library rebuild "$T\lib" --fresh`.
2. **Expected:** `search "$T\lib" tokyo` finds DSC00002 with `rating=4 label=Red` and the comment "Sunset at Shibuya & friends".
3. If you have Lightroom Classic or digiKam: open the `card` folder, read metadata from files, and check that DSC00001 shows 4 stars, the Red label, the comment and the keywords. (Lightroom reads `<name>.<ext>.xmp` sidecars for JPEGs; for raw files it only reads `<stem>.xmp`, which Keepsake doesn't write yet.)

---

## G. macOS-specific checks

1. Run section A on macOS.
2. In section B, put a folder with an accented name (for example `Café`) on the card, containing a copy of a JPG, before running `add-volume`.
   **Expected:** the file is indexed once. Searching `cafe` finds it.
3. Clean up: delete the `$T` folder.
