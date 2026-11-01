# Manual Test Steps — Report rows open their photos, A full cache keeps the thumbnails, Scan again one folder (right-click), Previous and next in the viewer, No "database is locked" during a backlog, Older video types in added folders, Start-up while the library opens, Library report, Background fingerprints for added folders, Adding a large folder (saved as it goes), Viewer information panel, Calendar without empty days, Faster zoom, Thumbnail sizes and the zoom slider, Automatic card import, Folder tree, All metadata, Calendar tree, Open in menu, and Add a folder in place

Test on **Windows 11** and **macOS 14+**. Paths below are Windows examples; on macOS use a folder such as `~/Pictures/2025`.

Start the app from source (`cd app/ui; pnpm tauri dev`), or from `scripts\keepsake.bat` on Windows. **Close any running Keepsake first.**

---

## Report rows open their photos

Automated:
```
cargo test -p keepsake-core --test report_tests every_report_row
cd app/ui; pnpm test; pnpm test:e2e report
```

Manual (both OSes):
1. Open **Report** in the sidebar. **Expected:** the name in each row (a year, a camera…) is a link in the accent colour; hovering it says how many items it shows.
2. On **Year**, click a year. **Expected:** the grid shows that year's photos and videos; the heading reads **Report › Year 2024**, and the item count in the status bar equals the row's **Items**.
3. Click **Report** in the heading. **Expected:** the report comes back on the **Year** table.
4. Repeat with a month, a day, a camera, a lens, a file type (for example ARW) and a drive. **Expected:** each time the count equals the row's **Items**. A drive that is unplugged still shows its items (with cached thumbnails).
5. If the report has an **Unknown** row (date, camera or lens), click it. **Expected:** only items without a date (or without a camera, or lens) show.
6. Two cameras whose names overlap (for example ILCE-7 and ILCE-7M4): click one. **Expected:** only that camera's items show.
7. Set a filter in the toolbar (type Videos, some words), open the report and click a row. **Expected:** the filters are cleared, and the count matches the row.

---

## A full cache keeps the thumbnails

Automated:
```
cargo test -p keepsake-core --test render_tests eviction
cargo test -p keepsake-core --test render_tests a_full_cache
cargo test -p keepsake-core --test catalog_tests opening_a_library_catalog
cd app/ui; pnpm test:e2e viewer
```

Manual (both OSes), with a large library whose cache is full (Settings shows the cache use against its limit):
1. Start Keepsake. **Expected:** the window and the grid come up within a few seconds, and the sidebar's **By folder** tree lists the folders.
2. Scroll the grid to the newest photos and wait a minute. **Expected:** their thumbnails show and stay; they do not disappear and come back.
3. Open a photo that has not been opened before (double-click). **Expected:** the picture appears within a few seconds (its preview is made right away). Open a video that has not been played. **Expected:** it plays after a short wait.
4. On Settings, the cache use stays at or under its limit. **Expected:** it settles near 90% of the limit or lower rather than sitting exactly at it.
5. Raise the cache limit in Settings (for example to 100 GB), wait a while. **Expected:** previews are made in the background again (the cache use grows past the old limit).
6. Open the log folder (Settings → Open log folder) and look at the newest log. **Expected:** "slow catalog write" lines are rare, not one every minute or two.

---

## Scan again one folder (right-click)

Automated:
```
cargo test -p keepsake-core --test add_folder_tests subfolder
cd app/ui; pnpm test:e2e folder-tree
```

Manual (both OSes):
1. In the sidebar choose **By folder**, open an added folder (on Windows for example `Videos` on `Z:`), and right-click one of its sub-folders. **Expected:** a menu with **Show** and **Scan again**.
2. Add a new photo to that sub-folder on disk, delete one from it, and also add a photo to a *different* sub-folder. Then choose **Scan again** on the first sub-folder. **Expected:** the Folders screen opens with "Scanning a folder…" and then "Scanned <name>: N items". The new photo is in the library and the deleted one is gone. The photo added to the other sub-folder is *not* in the library yet. Ratings on the scanned sub-folder's photos are unchanged.
3. Right-click the added folder itself (the top of its tree) and choose **Scan again**. **Expected:** the whole folder is read, and the photo in the other sub-folder comes in.
4. Right-click a folder on a drive that is not connected. **Expected:** Scan again is greyed out, and the menu says the drive is not connected.
5. With the keyboard, move to a folder with the arrow keys and press the Menu key (Windows) or Shift+F10 (both OSes). **Expected:** the menu opens on its first item; ↑ ↓ move, Enter chooses, and Escape closes it and returns to the folder.
6. Start a backup, and while it runs choose Scan again on a folder. **Expected:** the menu says another job is already running, and nothing starts.
7. Nothing inside the added folder is changed by any of this: no files are written there.

---

## Previous and next in the viewer

Automated:
```
cd app/ui; pnpm test:e2e viewer
```

Manual (both OSes):
1. Double-click a photo in the grid. **Expected:** large ‹ and › buttons sit on the left and right edges of the picture, and brighten when the mouse is over the picture.
2. Click ›, then ‹. **Expected:** the next and then the previous item shows in the same viewer; the position at the top ("4 of 50") changes, and the viewer does not close.
3. Go to the first item. **Expected:** ‹ is hidden. On the last item, › is hidden.
4. After clicking a button, press → and ←. **Expected:** they step too.
5. Select three items (Ctrl-click on Windows, ⌘-click on macOS), press Enter, and use ‹ ›. **Expected:** only those three are shown.
6. Open a video. **Expected:** the buttons are there too, and do not cover the play bar.
7. Zoom a photo in (Z). **Expected:** the arrow keys pan the picture, and the edge buttons still step to the next item.

---

## No "database is locked" during a backlog

Automated:
```
cargo test -p keepsake-core --lib catalog::error
cargo test -p keepsake-core --test fingerprint_tests large_backlog
```

Manual (both OSes; best with the library on a USB hard disk or other slow drive):
1. Start Keepsake with a large library that still has thumbnails or fingerprints to make (the status bar shows work queued), then click **Scan again** on a folder in **Folders**.
2. Scroll the grid and open items while the scan and background work run, for a few minutes.
3. **Expected:** no "database is locked" message, and thumbnails keep appearing. A slow drive may make things wait briefly, but nothing fails.
4. Open the log folder (**Settings → Open log folder**). **Expected:** no `database is locked` lines. Any `slow catalog write` lines name the code that held the catalog; please send them along.

---

## Older video types in added folders

Automated:
```
cargo test -p keepsake-core --test add_folder_tests older_home_video_types
```

Manual (both OSes):
1. Open **Folders** in the sidebar and click **Scan again** next to `Videos` (on Windows `Z:ideos`, a NAS share); on macOS add a folder holding `.mpg`, `.vob` or `.wmv` files.
2. **Expected:** the count grows from 33 to about 300; `.mpg`, `.mpeg` and DVD `.VOB` clips appear as videos with thumbnails and play in the viewer; files in subfolders (for example `Cancun_2009`) are there too.
3. **Expected:** DVD `.IFO`/`.BUP` files, `Thumbs.db` and the hidden `.metadata` folder are not items. A few broken files (for example `SalehKetab_trial_0.mp4`, which has no `moov` atom) show without a thumbnail, as before.

---

## Start-up while the library opens

Automated:
```
cargo test -p keepsake-app state::tests
cd app/ui; pnpm test:e2e opening
```

Manual (both OSes):
1. Start Keepsake normally. **Expected:** the library appears within a few seconds, as before.
2. To see a slow open, use a library on a slow drive (a USB hard drive or network folder) the first time after a catalog upgrade, or a large library on any drive. **Expected:** the window draws at once with **Opening your library…**; after about 3 s it adds that the first start after an update can take a minute; then the library appears. The window can be moved and resized meanwhile and is never shown as *Not responding* (Windows) or with a spinning cursor (macOS).
3. Close Keepsake while it says Opening, then start it again. **Expected:** it opens normally (an interrupted upgrade is redone).

---

## Library report (Tools → Report)

Automated:
```
cargo test -p keepsake-core --test report_tests
cargo test -p keepsake-cli --test cli_tests report
cd app/ui; pnpm test; pnpm exec playwright test e2e/mocked/report.spec.ts --project=mocked
```
**Expected:** all pass.

The first start after this change upgrades the catalog to schema v8 (it adds each item's file count and size). This happens once and takes about 15 seconds for 300,000 items; the window may wait that long on that first start.

1. In the sidebar under **Tools**, click **Report**.
   **Expected:** within a second or two the screen shows Photos, Videos, Files and Size for the whole library, and a table by **Year**, newest first. Items with no date are one **Unknown** row at the bottom. Hovering a size shows the exact bytes.
2. Compare with what you know: the number of photos plus videos equals "N items" in the status bar of **All items** with no filters.
   **Expected:** they match. A RAW+JPEG pair is one photo, but its row's Files counts 2.
3. Click **Month**, **Day**, **Camera**, **Lens**, **File type** and **Drive** in turn.
   **Expected:** each shows its own table. Months read "June 2024", days "1 June 2024". Cameras and lenses are biggest first, with **Unknown** last (items without a camera or lens, e.g. added with `--no-probe`). File type shows `ARW`, `JPG`, `MP4` and so on, items only (Photos and Videos show —); a RAW+JPEG pair is counted under both. The footer row is the library total.
4. With a drive unplugged (e.g. the network drive off, or an external disk ejected), click **Refresh**.
   **Expected:** nothing changes in the totals: unplugged items still count. In **Drive**, that drive is listed with "(unplugged)".
5. If you have a backup (Tools → Backup), look at **Drive**.
   **Expected:** the backup drive is listed with its own files and size. The totals at the top are not doubled: a backed-up file counts once. The footer "All drives" adds every copy.
6. Click **Export to CSV…**, keep the suggested name (`Keepsake report <today>.csv`) and save it in Documents.
   **Expected:** "Saved to …" with the path. Opening the file in Excel (Windows) or Numbers (macOS) shows the columns Breakdown, Group, Photos, Videos, Items, Files, Bytes, Size, a Total row, then the Year, Month, Day, Camera, Lens, File type and Drive rows. Accented names read correctly.
7. Click **Export to CSV…** and cancel the dialog.
   **Expected:** nothing is written and no error is shown.
8. From a terminal: `keepsake report`, `keepsake report --by camera`, `keepsake report --by drive --csv report.csv`, `keepsake --json report`.
   **Expected:** a table with photos, videos, items, files and size per group and a Total line; `--csv` writes the file and says so; `--json` prints the whole report.
9. Check nothing was written to your photo folders.
   **Expected:** the report only reads the catalog; no file in a photo folder has a new modification time.

On **macOS**, the save dialog is the system sheet; on **Windows**, the standard Save As window. Everything else is the same.

---

## Adding a large folder: saved as it goes, carries on after a stop

Automated:
```
cargo test -p keepsake-core --test add_folder_batch_tests
cd app/ui; pnpm exec playwright test e2e/mocked/folders.spec.ts --project=mocked
```
**Expected:** all pass.

Use a big folder (thousands of photos). A network drive is the best test, e.g. `2010` on the MyCloud drive; on macOS a folder on a NAS share or an external disk.

1. Open **Folders**, type or pick the folder, and click **Add**.
   **Expected:**
   - within a few seconds the folder appears under "In the library" and in the sidebar's folder tree (By folder), even though the progress bar has only just started;
   - the line "N items in the library so far" grows by about 20 at a time;
   - the sidebar count for the folder grows as well; the grid shows the new photos every few seconds when nothing is selected.
2. Select a photo in the grid while the scan runs.
   **Expected:** the selection stays put (the grid does not jump); when the scan finishes, the grid shows everything.
3. Click **Stop** part way through.
   **Expected:** the screen says what was read so far is in the library; the folder and its items stay in the tree.
4. Click **Add** again for the same folder (or **Scan again** in its row).
   **Expected:** the count starts from where it stopped, the progress moves quickly over the files already saved, and it finishes with the full number of items. No item is listed twice.
5. Quit Keepsake while a scan runs, start it again, and add the folder again.
   **Expected:** the same as step 4: it carries on.
6. In the folder (outside Keepsake), delete one photo and add a new one, then **Scan again**.
   **Expected:** the deleted photo is gone from the library, the new one is there, and the scan is quick.
7. From a terminal: `keepsake add-folder <folder>`, press Ctrl+C part way, then run it again.
   **Expected:** the first run says what was read is in the library; the second finishes and its report counts the items not read again.
8. Add a very large folder with many Lightroom `.xmp` files (e.g. `2019` on the MyCloud drive).
   **Expected:** the progress shows "Listing files", then "Reading sidecars", and the first items appear within about a minute; the screen never sits still for minutes. Photos rated in Lightroom show those ratings. The whole folder is in within minutes to tens of minutes, not hours (only the start and end of each file is read).

---

## Full fingerprints are made in the background

Automated:
```
cargo test -p keepsake-core --test fingerprint_tests
cd app/ui; pnpm exec playwright test e2e/mocked/folders.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open an existing library made before this change.
   **Expected:** it opens as before; every folder already in it shows no "Fingerprinting" line (its files were read in full when added).
2. Add a big folder (e.g. `2019` on the MyCloud drive; on macOS a NAS share or external disk).
   **Expected:** when the add finishes, its row under "In the library" says "Fingerprinting N files in the background", and N goes down every few seconds. Thumbnails keep appearing meanwhile. When N reaches 0 the line goes.
3. While it is fingerprinting, quit Keepsake and start it again.
   **Expected:** the line comes back with the count where it was, and keeps going down.
4. Unplug the drive (or disconnect the network share) while it is fingerprinting.
   **Expected:** the row says "N files to fingerprint when connected"; reconnect and it carries on within about ten minutes.
5. From a terminal: `keepsake verify` while some files are not fingerprinted yet.
   **Expected:** for that folder it says how many were checked by size and first and last MiB only; no problems are reported.
6. Back the library up to another drive (Backup screen) before fingerprinting finishes.
   **Expected:** the backup copies the folder's files without errors; those files then no longer count as waiting for a fingerprint.

---

## The viewer's information panel

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/viewer.spec.ts --project=mocked
cd app/ui; pnpm exec playwright test e2e/app/tools.spec.ts --project=app
```
**Expected:** all pass (the second is Windows only).

1. Open a photo in the viewer (double-click it) and click **ⓘ Info** at the top right.
   **Expected:**
   - a panel opens on the right; the picture shrinks to fit the space left, and is still whole and centred;
   - at the top: date and time (with seconds), camera, lens, exposure (e.g. "1/250 s · f/2.8"), ISO, focal length, dimensions, file size, and the location if the photo has GPS;
   - below: All metadata, grouped as in the inspector (Camera and image, Exposure, maker notes, File…).
2. Type "iso" in the panel's filter.
   **Expected:** only ISO tags stay; the viewer does not move to another photo or close while you type.
3. Click outside the filter (on the picture's dark edge), then press → and ←.
   **Expected:** the next and previous items are shown, and the panel updates for each one.
4. With the panel open, press Z, use the zoom slider, drag the picture, then click Fit.
   **Expected:** zoom works as before; at 100% the picture zooms around the middle of the area beside the panel; Fit shows the whole picture beside the panel, not under it.
5. Press **I**.
   **Expected:** the panel closes and the picture grows back to the full width. Press I again: it opens.
6. Open a RAW + JPEG pair and press R.
   **Expected:** the panel says "The RAW file" and shows the RAW's tags (e.g. a different file name ending in .ARW); R again goes back to the JPEG.
7. Open a video clip.
   **Expected:** the panel shows duration, frame rate and dimensions, and no exposure line; playback keys (Space, `,` `.`, J L) still work.
8. Close the app with the panel open, start it again and open the viewer.
   **Expected:** the panel is open. Close it, restart: it stays closed.
9. Unplug the drive with some photos (or pick an item marked as offline) and view it.
   **Expected:** the panel shows the catalog's date, camera and lens, and says the drive is not connected; no tag list.
10. On **macOS**, repeat steps 1, 4 and 5 with a Retina display.
    **Expected:** the same; Fit's percentage is smaller with the panel open when the picture is limited by its width.

---

## The calendar shows only months and days with pictures

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/library.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open Calendar view (Ctrl+1) and click a year in which you took pictures in only some months.
   **Expected:**
   - only the months with pictures are shown, in order (e.g. March, June, December), each with its count and a cover;
   - no greyed-out empty months.
2. Click a month.
   **Expected:**
   - only the days with pictures are shown, in order, each with its count and a cover;
   - no Mo–Su headings and no blank or greyed-out days;
   - each day reads its weekday and number, e.g. "Sat 14" (or "14 Sat", depending on your language settings), and the weekday is correct.
3. Use the arrow keys to move between cells, Enter to open, Backspace to go up.
   **Expected:** focus moves only between the cells shown; Enter on a day opens that day's grid with the count shown on the cell.
4. Set a filter (e.g. 4 stars or more), then go back to the calendar.
   **Expected:** months and days with no matching items disappear.
5. Compare a day in the sidebar's calendar tree with the same day in the calendar.
   **Expected:** the same label (e.g. "Sat 14").

---

## Faster zoom in the viewer

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/viewer.spec.ts --project=mocked
cd app/ui; pnpm exec playwright test e2e/app/zoom-perf.spec.ts --project=app
```
**Expected:** all pass (the second is Windows-only and needs the debug app built; it prints a timing table).

Use a large camera JPEG (20 MP or more) on a plugged-in drive.

1. Open the photo in the viewer and wait one second without touching anything.
   **Expected:** no "Loading full size…" note appears; the picture may get slightly sharper as the full size replaces the preview.
2. Press **Z**.
   **Expected:** 100% at once, with no wait and no note.
3. Turn the mouse wheel quickly up and down over a detail.
   **Expected:** the zoom keeps up smoothly; the detail stays under the pointer; no stutter or white flash.
4. Drag the zoom slider end to end quickly.
   **Expected:** the picture follows the slider smoothly.
5. Zoom in and drag the picture to a corner, then further.
   **Expected:** it stops at the picture's edge; no empty gap opens beside it. Zoomed out below the window size, it sits centred.
6. Open a photo shot in portrait (camera turned).
   **Expected:** upright at Fit and at 100%.
7. Open a photo on an unplugged drive.
   **Expected:** the preview shows with the offline note; zoom enlarges the preview; nothing hangs.
8. Check the original files' modified time in Explorer / Finder before and after.
   **Expected:** unchanged.

---

## Thumbnail sizes in the grid

Automated:
```
cd app/ui; pnpm exec playwright test e2e/mocked/thumb-size.spec.ts --project=mocked
```
**Expected:** all pass.

1. Open Grid view (Ctrl+2). At the right of the toolbar, next to Calendar | Grid | Details, is "Thumbnails" with three buttons: **Small**, **Medium** and **Large**, each with a square icon.
   **Expected:** Medium is highlighted.
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
   - if you drag within half a second of opening, "Loading full size…" may show briefly (a RAW: "Decoding the RAW…"); after that, no note;
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
