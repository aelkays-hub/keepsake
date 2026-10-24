# Manual Test Steps — Prompt 8: device detection

Test on **Windows 11 x64** (primary) and **macOS 14+**.
Build with `cargo build -p keepsake-cli`. Below, `$K` is `.\target\debug\keepsake.exe` (Windows) or `./target/debug/keepsake` (macOS).

**You need:**
- One or two real Sony cards (SD or CFexpress) with some photos or clips on them.
- A card reader. A second reader, or a camera connected over USB in mass-storage mode, is also useful.
- Two destination folders, ideally on two different drives.

Keepsake never writes to a card, but test with cards whose contents also exist somewhere else.

The PowerShell commands are for Windows. On macOS use the same commands with `/tmp/ks`, and cards appear under `/Volumes/…`.

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass, including:
   - `watch_tests` (12);
   - `devices_tests` (3), one of which starts the real device backend on this machine;
   - the new `cli_tests` (`watch_…`, `presets_and_cards…`).
2. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no output.
3. **macOS:** the GitHub `macos-14 (Rust)` job is green. It is the first run of the DiskArbitration code.

---

## B. Set up

```powershell
$T = "$env:TEMP\ks8"; Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue
$env:KEEPSAKE_CONFIG_DIR = "$T\config"
& $K init "$T\lib"
New-Item -ItemType Directory -Force "$T\A", "$T\B" | Out-Null   # or folders on two drives
```

---

## C. Detection (no import)

1. With **no card inserted**, run `& $K watch`.
   **Expected:**
   - `Watching for cards (automatic import off)…`;
   - nothing for your internal disks;
   - no "insert a disk" pop-up for empty card readers.
2. Insert a Sony card.
   **Expected:**
   - within about 2 seconds, `Card inserted: <label> (E:\) [XAVC, stills] — new card: N item(s)`;
   - N matches what the camera shows (a clip and its XML count as one item, and so do RAW + JPEG).
3. Remove the card (eject it first on macOS). **Expected:** `Card removed: <label> (E:\)`.
4. Insert a non-Sony USB stick. **Expected:** nothing is printed. With `--json` it shows as `volume_ignored`.
5. Leave the card inserted, press Ctrl-C, and start `& $K watch` again.
   **Expected:** the card already inserted is reported at start, and is now a **known card, never imported**.
6. Press Ctrl-C. **Expected:** `Stopped watching.` and exit code 0.
7. `& $K card list` **Expected:** one card, last seen just now, not trusted.

---

## D. "New since last import"

1. Import the card by hand to two drives:
   ```powershell
   & $K ingest E:\ --dest "$T\A" --dest "$T\B"
   ```
   Add `--unsafe-allow-same-disk` if A and B are on one disk. **Expected:** SAFE TO FORMAT.
2. `& $K watch`, then remove and reinsert the card.
   **Expected:** `0 new item(s) since last import`.
3. Shoot 2 new photos on the card, then insert it.
   **Expected:** `2 new item(s) since last import`.
4. **Other slot:** insert the same card in a different reader, or with the camera over USB, so it gets a different drive letter.
   **Expected:** the same card (same name in `card list`) with the same count. It is not a new card.
5. **Second card:** insert another card, possibly in the same reader. **Expected:** a separate **new card**.
6. **Reformat:** format a card in the camera, then insert it. **Expected:** a **new card** with 0 items. Formatting gives it a new serial.

---

## E. Trusted cards and automatic import

1. Make a preset:
   ```powershell
   & $K preset save "Two drives" --dest "$T\A" --dest "$T\B"
   ```
   Make sure the folders exist first, or the save is refused.
2. Name and trust the card:
   ```powershell
   & $K card rename Untitled "Card 1"
   & $K card trust "Card 1" --preset "Two drives"
   ```
3. Shoot a few new photos, then run:
   ```powershell
   & $K watch --auto-ingest
   ```
   Then insert the card. Add the hidden `--unsafe-allow-same-disk` if A and B share a disk.
   **Expected:**
   - `Card inserted … N new item(s)`, then `Importing with preset Two drives to …`;
   - then `Imported: N asset(s) catalogued` and `SAFE TO FORMAT`;
   - the files appear in both A and B, with `.xmp` sidecars;
   - nothing changes on the card.
4. Remove and reinsert. **Expected:** `0 new item(s)` and `Not importing automatically: nothing new on the card`.
5. Insert an **untrusted** card. **Expected:** it is reported but not imported.
6. **Destination missing:** unplug the drive holding B, or rename the B folder. Shoot a photo and insert the card.
   **Expected:** `Not importing automatically: destination … not found (is the drive connected?)`. Nothing is copied.
7. **Ctrl-C mid-import:** shoot a few large clips. Insert the card and press Ctrl-C once while it imports.
   **Expected:**
   - the current file finishes, then `Stopped after the current file. Continue with: keepsake ingest --resume --session …`;
   - exit code 130;
   - no `.keepsake-partial-*` files in A or B (use `Get-ChildItem -Force -Recurse`).

   Then `& $K ingest --resume`. **Expected:** SAFE TO FORMAT.
8. **Card pulled mid-import:** start an automatic import of large clips, then pull the card out (on macOS, eject it with Force).
   **Expected:**
   - the import ends with failed items and **NOT safe to format**;
   - `card removed` is printed;
   - after reinserting, the failed items count as new;
   - `ingest --resume` (with the card back in) completes it.
9. `& $K preset delete "Two drives"`
   **Expected:** refused, because "Card 1" uses it. Run `& $K card untrust "Card 1"` first, then delete again: it works.

---

## F. JSON for the app

`& $K --json watch --auto-ingest`, then insert and remove a card.
**Expected:**
- one JSON object per line, appearing as things happen;
- the first is `{"auto_ingest":true,"event":"watching"}`;
- then `card_inserted` (with `items`, `new_items` and `known`), `auto_ingest_*` and `card_removed`;
- `{"event":"stopped"}` after Ctrl-C.

---

## G. Card memory survives

1. `& $K rebuild-catalog --fresh`, then `& $K card list`. **Expected:** the cards are still listed and still trusted.
2. `& $K backup "$T\backup"`. **Expected:** `$T\backup\.keepsake-library\` contains `cards.json`, `cards\` and `presets.json`.
3. `& $K card forget "Card 1"`. **Expected:** the card is gone from `card list`, while its photos are still found by `& $K search --type photo`. Reinserted, it is a new card.

---

## H. macOS specifics

- Cards mount under `/Volumes/<name>`. Detection should appear within about 2 seconds of the mount.
- Eject with Finder, then pull the card. **Expected:** `Card removed`.
- A camera over USB that mounts its card behaves like a reader.
- The same card on Windows and on macOS is **not** recognised as one card (different serial formats). This is expected.
- Run C and D on macOS. Watch for crashes or a missing removal event, and report them with `RUST_LOG=keepsake_platform=debug`.
