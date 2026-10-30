# Manual Test Steps — Thumbnail sizes and the zoom slider, Automatic card import, Folder tree, All metadata, Calendar tree, Open in menu, and Add a folder in place

Test on **Windows 11** and **macOS 14+**. Paths below are Windows examples; on macOS use a folder such as `~/Pictures/2025`.

Start the app from source (`cd app/ui; pnpm tauri dev`), or from `scripts\keepsake.bat` on Windows. **Close any running Keepsake first.**

---

## Thumbnail sizes in the grid

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/thumb-size.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open Grid view (Ctrl+2). At the right of the toolbar, next to Calendar | Grid | Details, are three square buttons: small, medium and large.
   **Expected:** the middle (Medium) is highlighted.
2. Click the large one.
   **Expected:**
   - fewer, bigger tiles (about 280 px);
   - the pictures are sharp, not blurry (they use the 1024 px thumbnails; tiles whose 1024 px thumbnail is not made yet show the small one until it is).
3. Click the small one.
   **Expected:** many small tiles (about 120 px).
4. Click a tile, press **+** twice, then **−** twice.
   **Expected:**
   - the size goes up to Large and back down to Small;
   - the selected tile stays on screen;
   - nothing is rated.
5. Restart Keepsake.
   **Expected:** the grid keeps the last size.
6. Switch to Details (Ctrl+3) or Calendar (Ctrl+1).
   **Expected:** the size switch is not shown.

---

## The zoom slider in the viewer

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/viewer.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open a photo in the viewer (Enter or double-click).
   **Expected:** under the picture, a bar with **Fit**, **−**, a slider, **+**, **100%**, and "Fit".
2. Drag the slider to the right.
   **Expected:**
   - the picture grows from the middle of the window;
   - "Loading full size…" shows briefly the first time (a RAW: "Decoding the RAW…");
   - the level reads a percentage, up to 400% at the end.
3. Drag the picture, or use the arrow keys.
   **Expected:** it pans; you stay on the same photo.
4. Click **100%**.
   **Expected:** one picture pixel per screen pixel (fine detail is crisp).
5. Click **Fit**.
   **Expected:**
   - the whole picture fits the window;
   - the level reads e.g. "Fit (23%)".
6. Press **+** a few times, then **−**.
   **Expected:** it zooms in and out in steps.
7. Point at a detail and turn the mouse wheel up.
   **Expected:** it zooms in on that detail, which stays under the pointer. Wheel down goes back to Fit.
8. Press **Z**, then Z again.
   **Expected:** 100%, then Fit. A click on the picture does the same.
9. Tab to the slider and press → and ←.
   **Expected:** the zoom changes; it does not move to another photo.
10. Open a video.
    **Expected:** no zoom bar; the playback controls are as before.

---

## Every card imports by itself

Automated:
```
cargo test -p keepsake-core --test auto_import_tests
cargo test -p keepsake-core --lib -- template settings
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/auto-import.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open **Settings → When a card is inserted**.
   **Expected:**
   - **Copy new photos and videos automatically** is selected;
   - **Copy to** is `E:\Pictures`;
   - **Folders** is Year › Month › Day;
   - the example reads "A photo taken today goes to E:\Pictures\2026\October\2026-10-30\A7400123.ARW" (today's date).
2. Insert a camera card with a few new photos.
   **Expected:**
   - the Import window opens and says "Importing new items automatically to E:\Pictures, in folders by year, month and day";
   - it starts by itself, with no clicks;
   - only new items are copied; items already in the library are left on the card;
   - it ends with "Copied N items to E:\Pictures" and "NOT safe to format" (one copy only);
   - "Closing in 5 s…" counts down and the window closes by itself;
   - **Keep open** stops the countdown;
   - if anything failed, or you pressed Stop, it stays open.
3. In Explorer, open `E:\Pictures\<year>\<Month>\<year-month-day>\`.
   **Expected:**
   - the photos are there; any missing year, month or day folders were made;
   - no `.xmp` files appear next to them.
4. In Keepsake, open **By folder → Pictures → <year> → <Month>**.
   **Expected:**
   - the new day's folder, with its count;
   - the calendar shows that day.
5. Remove the card and insert it again.
   **Expected:** the window opens with nothing new to copy ("everything is already in your library"), or does not open at all.
6. Disconnect E: (or set Copy to a missing drive, e.g. `Z:\Pictures`), then insert a card.
   **Expected:** "Not imported automatically: destination … does not exist". Nothing is copied.
7. Choose **Open the import window, and let me choose** and save.
   **Expected:** the next card opens the window without starting.
8. Check the card in Explorer afterwards.
   **Expected:** nothing on it was changed or deleted.

---

## The sidebar tree by folder

Automated:
```
cargo test -p keepsake-core --test folder_tree_tests
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/folder-tree.spec.ts --project=mocked
```
**Expected:** all pass.

1. Under **Library** in the sidebar, click **By folder**.
   **Expected:**
   - the calendar tree is replaced by your folders (e.g. **Pictures**) with their counts and connected dot;
   - the separate Volumes list is gone.
2. Click **Pictures**.
   **Expected:**
   - the grid shows everything in it;
   - its folders appear below (2025, 2026, Edited JPEG…), each with a count.
3. Click **2025**, then **May**.
   **Expected:**
   - the heading reads "Pictures › 2025 › May";
   - the status bar's count matches the tree;
   - the grid holds only that folder and its sub-folders.
4. Click only the ▸ arrow of another folder.
   **Expected:** it opens or closes; the grid does not change.
5. Set Type to Video.
   **Expected:** the counts change; folders without videos disappear.
6. Restart Keepsake.
   **Expected:** the sidebar is still **By folder**.
7. Click **By date**.
   **Expected:** the calendar tree and the Volumes list are back.
8. Keyboard: Tab to the sidebar and ↓ to Pictures.
   - → opens it; → again goes to its first folder;
   - ← goes back up; Enter shows it.

---

## All metadata in the inspector

Automated:
```
cargo test -p keepsake-core --lib meta_tests
cargo test -p keepsake-core --test probe_tests every_tag
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/metadata.spec.ts --project=mocked
```
**Expected:** all pass.

1. Select an ARW photo. At the foot of the right-hand panel, click **All metadata**.
   **Expected:**
   - within about a second: "All metadata (300 or so)";
   - groups in this order:
     - Camera and image (IFD0)
     - Exposure (EXIF)
     - Sony maker notes
     - Computed by ExifTool
     - the file's inner structure (IFD1, SubIFD, SR2…)
     - File
     - System
   - values match the camera, e.g. Exposure Time 1/250, ISO 1600, Lens ID Sigma 24-70mm F2.8 DG DN | A.
2. Type `iso` in the filter.
   **Expected:** only tags whose name or value contains "iso". Clear it.
3. Click **Copy** and paste into Notepad (macOS: TextEdit).
   **Expected:** the file name, then `[Group]` blocks with `Name: value` lines.
4. Select another photo.
   **Expected:** the section stays open and shows that photo's tags. It is still open after restarting Keepsake.
5. Select a RAW + JPEG pair.
   **Expected:**
   - JPEG and RAW buttons;
   - RAW shows the ARW's tags, with many more maker notes.
6. Select an item on an unplugged drive.
   **Expected:** "Connect the drive with this item to read its metadata."
7. Check the file afterwards.
   **Expected:** its modified time and size are unchanged.

---

## Calendar tree in the sidebar

Automated: `cd app/ui; pnpm exec playwright test e2e/mocked/calendar-tree.spec.ts --project=mocked`.
**Expected:** passes.

1. Look at the sidebar under Library.
   **Expected:** **Calendar ▾** with every year that has items, each with its count, and "No date" if some items have no date.
2. Click a year.
   **Expected:**
   - the main area shows its months;
   - the year opens in the tree, listing its months with counts;
   - the year is highlighted.
3. Click a month.
   **Expected:** the main area shows its days; the tree lists the days (e.g. "Sun 15") with counts.
4. Click a day.
   **Expected:** the grid of that day; the heading reads Calendar › year › month › day; the count matches the tree.
5. Click only the ▸ arrow of another year.
   **Expected:** it opens or closes in the tree; the main area does not change.
6. Move through the calendar view itself (Ctrl+1, then click a year, then a month).
   **Expected:** the tree opens to the same place.
7. Keyboard: Tab to the sidebar and ↓ to a year.
   - → opens it; → again goes to its first month;
   - ← goes back to the year; ← again closes it;
   - Enter opens it in the main area.
8. Set a filter (e.g. Type: Video).
   **Expected:** the tree's counts change to match.

---

## Open in: Photoshop and Topaz Photo

Automated:
```
cargo test -p keepsake-core --lib settings
cargo test -p keepsake-app --lib
cd app/ui; pnpm exec playwright test e2e/mocked/open-in.spec.ts e2e/mocked/a11y.spec.ts --project=mocked
```
**Expected:** all pass.

1. **Settings → Open in.** Add a row:
   - types `arw, jpg, dng`;
   - Browse… to `C:\Program Files\Adobe\Adobe Photoshop 2026\Photoshop.exe` (macOS: `Adobe Photoshop 2026.app`).

   Then add a second row: the same types, with `C:\Program Files\Topaz Labs LLC\Topaz Photo\Topaz Photo.exe` (macOS: `Topaz Photo.app`). Click **Save**.
   **Expected:**
   - one row per type and application;
   - the first row of each type shows "(Ctrl+O)";
   - `config.toml` has `arw = ['…Photoshop.exe', '…Topaz Photo.exe']`.
2. Select a JPEG or ARW and click **Open in… ▾** in the right-hand panel.
   **Expected:**
   - the menu lists Photoshop (marked Ctrl+O), Topaz Photo and System default;
   - the first entry has focus.
3. Press ↓ then Enter.
   **Expected:**
   - Topaz Photo opens the file;
   - the panel says "Opened with Topaz Photo";
   - focus is back on the button.
4. Press **Ctrl+O** (macOS: Cmd+O).
   **Expected:** Photoshop opens the file.
5. Open the viewer (Enter), click **Open in… ▾**, then press Escape.
   **Expected:** the menu closes and the viewer stays open.
6. Pick **Photoshop** from the viewer's menu.
   **Expected:** it opens.
7. On a RAW + JPEG pair, press R (to show the RAW), then use Open in.
   **Expected:** the ARW is opened.
8. Select a video and open the menu.
   **Expected:** only System default (unless an app is set for mp4).

---

## Add a folder in place

## A. Automated tests

```
cargo test -p keepsake-core --test add_folder_tests
cargo test -p keepsake-cli --test cli_tests add_folder
cd app/ui; pnpm exec playwright test e2e/mocked/folders.spec.ts e2e/mocked/a11y.spec.ts --project=mocked
```
**Expected:** all pass.

---

## B. Add a folder of photos you already have (the app)

1. Before you start, note the folder's file count and size. Windows: right-click → Properties. macOS: Get Info.
2. In Keepsake, with an empty library, the grid says "Nothing here…" and shows **Add a folder…**. Click it.
   **Expected:** the **Folders** screen opens. Folders is also in the sidebar under Tools.
3. Click **Browse…**, pick the folder (e.g. `E:\Pictures\2025`) and click **Add folder**.
   **Expected:**
   - a progress bar with files, bytes and the file being read; then "Reading dates and cameras", then "Saving";
   - you can switch to other screens meanwhile;
   - at the end: "Added 2025: N items", "Nothing in the folder was changed", how many took ratings from `.xmp` files, and how many other files were left out.
4. Click **Show them**.
   **Expected:** the grid shows the folder's items, with dates and cameras. "2025" is in the sidebar under Volumes. Thumbnails appear over the next minutes.
5. Photos you rated in Lightroom show those stars.
6. Rate a photo and give it a label in Keepsake.
7. Check the folder again (as in step 1).
   **Expected:** the same file count and size. No new `.xmp` files appear next to your photos. Keepsake's sidecars are in the library folder, under `sidecars\`.

## C. Stop, scan again, overlaps

1. Add a large folder and click **Stop** partway.
   **Expected:** "Stopped. Nothing was added"; nothing new in the sidebar.
2. Copy a new photo into an added folder, then click **Scan again** next to it on the Folders screen.
   **Expected:** the count goes up by one. Earlier ratings are kept.
3. Try to add a folder **inside** one you added (e.g. `E:\Pictures\2025\January`).
   **Expected:** an error saying it is inside a folder already in the library. Nothing changes.
4. Try to add a folder that **holds** one you added (e.g. `E:\Pictures`).
   **Expected:** an error saying it holds a folder already in the library.

## D. A library made inside the photo folder

1. Create a library **in** a photo folder (Settings → Open another library → create, choosing e.g. `E:\Pictures\2025`). Then add that same folder.
   **Expected:**
   - only photos and videos are added: no `catalog.db`, `.json` or `sidecars` items;
   - after rating something, **Scan again** adds no extra items, and the rating stays.

## E. The command line

```
keepsake add-folder "E:\Pictures\2025"
```
**Expected:**
- a progress bar, then "Added 2025 (E:\Pictures\2025): N items, N files, N with Lightroom or Keepsake sidecars";
- Ctrl-C stops: "Stopped. Nothing was added";
- adding `E:\Pictures\2025\January` afterwards fails (exit code 1) with "already in the library".

## F. Accessibility

1. Keyboard only: Tab to the sidebar, arrow down to **Folders**, press Enter. Tab to the folder field, type a path and press Enter.
   **Expected:** the folder is added.
2. With a screen reader (Narrator / VoiceOver):
   - the folder field is announced as "Folder to add";
   - progress is announced as a status.
