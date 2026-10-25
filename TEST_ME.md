# Manual Test Steps — Prompt 10: the main app UI

Test on **Windows 11** (primary) and **macOS 14+**.

**Build:**
- `scripts/setup.ps1` (or `setup.sh`) once.
- `cargo build -p keepsake-app -p keepsake-cli` (and `--release -p keepsake-cli` for step B3).

**Run the app:** from `app/ui`, `pnpm tauri dev`. This starts Vite and the debug app.

To keep your real settings apart, start it from a shell with a test config folder:
```powershell
$T = "$env:TEMP\ks10"; $env:KEEPSAKE_CONFIG_DIR = "$T\config"
```
On macOS the paths are `/tmp/ks10`.

Nothing in the app modifies your photos or videos. Edits go to XMP sidecars and the catalog.

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass, including `browse_tests`, and `render_tests` with the card thumbnail test. Running `cargo test -p keepsake-app` must leave `app/ui/src/bindings` unchanged (`git status`).
2. In `app/ui`:
   - `pnpm test` **Expected:** the unit tests pass.
   - `pnpm test:e2e` **Expected:** 14 browser tests pass against the mocked backend.
   - On Windows, after the builds above: `pnpm test:app` **Expected:** 4 real-app tests pass. They cover first run, the library flows, card import, and 300,000 items.
3. `pnpm lint`, `pnpm format:check`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no problems.

---

## B. First run and the library

1. Start the app with an empty `$T\config`.
   **Expected:** "Welcome to Keepsake". Browse… opens a native folder picker. Choose or type `$T\lib`, then click **Create or open library**.
   **Expected:** the main window opens with "0 items".
2. Import some media with the CLI (or with the import window, in F):
   ```powershell
   keepsake ingest <card or folder> --dest "$T\A" --dest "$T\B"
   ```
   Back in the app, **Expected:** within a few seconds:
   - the status bar shows the item count;
   - thumbnails appear as they are made ("Renders: N queued", then "up to date");
   - Volumes in the sidebar lists A and B with green dots;
   - Imports lists the session.
3. **Large library:**
   ```powershell
   keepsake init "$T\big"
   keepsake dev seed --assets 300000
   ```
   Use the release CLI for the seed (about a minute). Then restart the app; it opens the default library.
   **Expected:**
   - "300,000 items", and scrolling stays smooth;
   - End jumps to the last item, which has its thumbnail within a second or so;
   - Ctrl+A shows "300,000 selected" instantly;
   - "Seed archive (unplugged)" shows a hollow dot, and its items show dimmed thumbnails with an orange **offline** badge.

---

## C. Views and keyboard

1. Ctrl+1 / Ctrl+2 / Ctrl+3 switch Calendar / Grid / Details (also the buttons top right).
2. **Calendar:**
   - Years show counts and a cover picture. Click (or arrow keys + Enter) a year, then a month, then a day.
   - **Expected:** the day's grid, with the breadcrumb "Calendar › 2024 › June › 15" and the right count.
   - Backspace returns to the month.
   - "No date" (if any) opens the undated items.
3. **Grid:**
   - The arrow keys move the dashed focus.
   - Shift+arrows and Shift+click extend; Ctrl/⌘+click toggles; Ctrl/⌘+A selects all; Esc clears.
   - Home/End and PgUp/PgDn work.
4. **Details:**
   - Click Name, Date, Camera, Lens, Type, Size, Duration or Rating to sort; click again to reverse (the ▲/▼ shows which).
   - The keyboard works as in the grid.
5. **Tab and F6** move between the sidebar, the main area and the inspector. In the sidebar, ↑/↓ move between entries and Enter opens one.
6. **Themes:** switch the OS between light and dark. **Expected:** the app follows at once.

---

## D. Inspector and edits (XMP)

1. Select one photo. **Expected:** its preview, name, capture time and source, camera, lens and type.
2. Press **4**. **Expected:** four stars in the tile and the inspector. Open the `.xmp` beside the file in `$T\A\…`: it now has `xmp:Rating` 4. **0** clears the rating.
3. Press **6**. **Expected:** a Red label (keys 6–9: Red, Yellow, Green, Blue, as in Lightroom). Click a swatch to change it.
4. Type a comment and click elsewhere. **Expected:** it is saved; the XMP has the description.
5. Add the keyword `Places|Lake` and press Enter. **Expected:** a chip appears, and the XMP has `lr:hierarchicalSubject`. ✕ removes it.
6. Select 5 items and press 3. **Expected:** "5 items selected — rating, label and keywords apply to all", and all five get 3 stars.
7. In the 300,000-item library, select all and press 2. **Expected:** "Saving N / 300,000…" in the status bar until it finishes; the app stays responsive.

---

## E. Search and filters

1. Type part of a file name. **Expected:** the grid narrows as you type.
2. Combine **From/To**, **Camera**, **Lens**, **Type** (Photos/RAW/Videos) and **★ or more**. **Expected:** each one narrows the result, and the count in the status bar matches.
3. **✕ Clear** resets everything. Ctrl/⌘+F focuses the search box.

---

## F. Import window (real card)

1. With the app open, insert a Sony card that has items not yet imported.
   **Expected:** within about 2 seconds:
   - an **Import** window opens: "N new items since last import (M on the card)";
   - tiles grouped by day, each with a thumbnail;
   - a RAW+JPEG pair, or a clip with its XML and proxy, is **one** tile.

   Nothing is written to the card.
2. **None**, then **Select all**, then **New only** behave as named, and the selected count updates.
3. Pick a preset, or fill in **Destination** (Browse…) and **Second destination** on another drive.
   **Expected:** "needs X GB, Y free" for each, and the first file's example name next to the template.
4. Choose two folders on the same disk. **Expected:** "both destinations are on the same disk…", and Import is disabled.
5. Untick the second destination. **Expected:** the warning that the card will not be safe to format.
6. With two drives, click **Import**.
   **Expected:**
   - a progress bar with bytes and the current file;
   - at the end, **✅ SAFE TO FORMAT**;
   - the main window shows the new items;
   - reinserting the card shows "0 new items".
7. Start another import and click **Stop**. **Expected:** the current file finishes, then **NOT safe to format (stopped…)**. Importing again continues.
8. **Save preset** stores the current destinations and template for next time.

---

## G. macOS

Repeat B–E (and F if a card is at hand). Folder pickers are native. The real-app UI tests do not run on macOS (WKWebView cannot be automated); `pnpm test:e2e` covers the UI there.
