# Manual Test Steps — All metadata, Calendar tree, Open in menu, and Add a folder in place

Test on **Windows 11** and **macOS 14+**. Paths below are Windows examples; on macOS use a folder such as `~/Pictures/2025`.

Start the app from source (`cd app/ui; pnpm tauri dev`), or from `scripts\keepsake.bat` on Windows. **Close any running Keepsake first.**

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
