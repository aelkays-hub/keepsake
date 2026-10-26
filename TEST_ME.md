# Manual Test Steps — Prompt 11: viewer, time shift, Open in, Backup, Settings

Test on **Windows 11** (primary) and **macOS 14+**.

**Build:**
- Run `scripts/setup.ps1` (or `setup.sh`) once. It now also downloads LibRaw into `vendor/libraw`.
- `cargo build -p keepsake-app -p keepsake-cli`

**Run the app:** from `app/ui`, `pnpm tauri dev`.

To keep your real settings apart, start it from a shell with a test config folder:
```powershell
$T = "$env:TEMP\ks11"; $env:KEEPSAKE_CONFIG_DIR = "$T\config"
```
On macOS use `export KEEPSAKE_CONFIG_DIR=/tmp/ks11/config` and `/tmp/ks11` for the paths below.

Use a library with a few photos, a RAW + JPEG pair, a RAW on its own and a video. Import them with the import window or `keepsake ingest`. Nothing in this prompt changes your photos or videos.

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass. New tests include `full_size_is_the_whole_picture_upright_and_made_once`, `raw_pairs_offer_a_toggle_and_the_raw_goes_to_libraw`, `video_info_reads_rate_and_start_timecode`, `removing_an_old_cache_keeps_everything_that_is_not_ours`, and the history check in `backup_is_incremental_and_verified`.
2. With `KEEPSAKE_CORPUS` set: `cargo test -p keepsake-core --test render_corpus_tests -- --include-ignored`
   **Expected:** `corpus_arw_full_size_via_libraw` passes. The ARW decodes larger than 2560 px.
3. In `app/ui`:
   - `pnpm test` **Expected:** 14 unit tests pass.
   - `pnpm test:e2e` **Expected:** 23 mocked browser tests pass.
   - On Windows, after the build: `pnpm test:app` **Expected:** 8 real-app tests pass.
4. `pnpm lint`, `pnpm format:check`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no problems.

---

## B. Viewer: photos

1. Select a JPEG and press **Enter** (or double-click it).
   **Expected:** a dark full-window viewer opens with the photo fitted, its name, date and "N of M" at the top.
2. Press **Z**, then click somewhere else on the photo.
   **Expected:**
   - "Loading full size…" shows briefly, then the photo at 100%: one photo pixel per screen pixel, so a 24 MP photo is much bigger than the window.
   - The click zooms in around where you clicked.
   - Drag pans; arrow keys pan; Z or scrolling the wheel down goes back to fit.
3. Zoom the same photo again.
   **Expected:** it is instant (the full-size picture is cached).
4. Open a **RAW + JPEG pair**.
   **Expected:**
   - A JPEG | RAW switch at the top right.
   - **R** switches to RAW: "Decoding the RAW…" (about 1–2 s), then the RAW decode. Its colours and detail differ slightly from the camera JPEG.
   - R again switches back to the JPEG.
5. Open a **RAW without a JPEG**.
   **Expected:** no switch; the full RAW decode is shown.
6. Press **→**, **←**, **Home** and **End**.
   **Expected:** the next, previous, first and last item of the view.
7. Close with **Esc**. Select 3 non-adjacent items with Ctrl-click and press Enter.
   **Expected:**
   - "· 3 selected" in the top bar.
   - The arrows visit only those three.
   - After Esc the three are still selected.
8. In the viewer, press **3** and then **7**.
   **Expected:** three stars and a yellow label on that item only. The XMP sidecar beside the original changes.
9. Press **F**.
   **Expected:** the window goes full screen; F again returns.
10. Disconnect the drive with an item, then open it.
    **Expected:** the preview shows, with "The drive with this photo is not connected: showing the preview." Z does not load a full size.

---

## C. Viewer: video

1. Open a video.
   **Expected:**
   - The proxy plays in the viewer, with play, ‹| |› buttons, a scrub bar, a timecode, "frame N" and position/duration.
   - If the proxy is not made yet: "Making a playable copy…", then the player appears within a few seconds.
2. Press **Space** to play and again to pause.
3. Press **.** three times, then **,** once.
   **Expected:** the timecode's frame number goes up 1, 2, 3, then back to 2. The picture changes each time.
4. Press **L** and then **J**.
   **Expected:** the timecode jumps one second forward, then back.
5. On a Sony clip with a recorded timecode, compare the first frame with the camera.
   **Expected:** the same starting timecode. A 29.97 drop-frame clip shows `;` before the frames, and frame numbers 00 and 01 are skipped at each new minute, except every tenth minute.
6. Drag the scrub bar.
   **Expected:** the picture and timecode follow.

---

## D. Batch time shift

1. Select several photos from one shoot whose camera clock was wrong. Press **Ctrl+T**, or **Shift time…** in the inspector.
   **Expected:** a dialog with the reference photo (the focused one), what the camera says, and a date/time field filled with that time.
2. Change the time, for example one hour later.
   **Expected:**
   - The offset reads "+1 h".
   - A table shows the first 10 items, now → after; a note gives how many more.
   - Items with no date are counted and left alone.
3. Pick a different reference photo in the list.
   **Expected:** the time field changes to that photo's time.
4. Click **Shift N items**.
   **Expected:**
   - The dialog closes and the grid re-sorts. The inspector shows the new time.
   - The `.xmp` sidecar of each photo has the new `exif:DateTimeOriginal` and `keepsake:CaptureShiftSeconds`.
   - The original files are unchanged: same size, same modified time, and `keepsake verify` reports them intact.
5. Press Ctrl+T and then Esc.
   **Expected:** nothing changes.

---

## E. Open in

1. Select a photo and press **Ctrl+O** (or **Open in…** in the inspector or viewer).
   **Expected:** it opens in the system default app (Photos on Windows, Preview on macOS).
2. In **Settings → Open in**, add file type `arw` with an application:
   - Windows: for example `C:\Program Files\…\YourEditor.exe` (Browse… helps).
   - macOS: `Preview.app`, or a full path.

   Click **Save settings**, then press Ctrl+O on a RAW.
   **Expected:** the RAW opens in that application. Other types still open in the default.
3. Disconnect the drive with an item, then select it.
   **Expected:**
   - The inspector's **Open in…** button is greyed out.
   - In the viewer, Ctrl+O shows "the drive with this item is not connected", and nothing opens.

---

## F. Backup

1. Click **Backup** in the sidebar.
   **Expected:** an explanation, a "Back up to a new place" box, and "No backups yet".
2. Type or browse to a folder on another drive, for example `E:\Keepsake backup`, and click **Start backup**.
   **Expected:**
   - A progress bar with files, bytes and the current file, and **Stop after this file**.
   - Then "Backup finished" with the count.
   - The destination appears with today's "Last backup" and "Last verified: never".
   - The history lists the run.
3. In Explorer or Finder, check the backup.
   **Expected:**
   - Originals by date with their sidecars.
   - `.keepsake-library\` with `catalog.db` and `history.json`.
4. Click **Back up now** again.
   **Expected:** "0 B copied this time".
5. Click **Verify now**.
   **Expected:** "Verification finished", N files checked. "Last verified" now has today's date, and the history has a Verify row with OK.
6. Change one byte of a photo on the backup (a hex editor, or append a character with a text editor). Click Verify now.
   **Expected:**
   - "Verification found problems", with that file listed as changed.
   - The history row reads "1 problem".
   - "Last verified" keeps the earlier good date.
7. Start a backup and click **Stop after this file**.
   **Expected:** "Backup stopped" and a "Stopped" history row. Running it again finishes the rest.
8. In a terminal: `keepsake backup E:\other` and `keepsake verify`.
   **Expected:** both runs appear in the Backup screen's history (reopen the screen).

---

## G. Settings

1. Press **Ctrl+,** (or **Settings** in the sidebar).
   **Expected:** sections for Library, Previews and thumbnails, Open in, and Import presets. The settings file path is shown.
2. **Cache:**
   - Choose a new cache folder, set the limit to 30 GB and image workers to 2, then **Save settings**.
   - **Expected:** "Saved." and a note that the previous cache is still at its old folder.
   - Thumbnails reappear as they are made in the new folder (scroll the grid).
   - Click **Remove old files**. **Expected:** "Removed N GB from the old cache". Only Keepsake's `thumbnail`, `preview`, `proxy`, `card` and `.tmp` folders are removed from the old folder; anything else there stays.
3. Set the limit to 0.1 and save.
   **Expected:** "the cache limit must be at least 256 MB". Nothing is saved.
4. Close and restart the app.
   **Expected:** the settings are as saved, and `config.toml` has `[render]` and `[open.apps]`.
5. **Presets:**
   - Edit a preset's destinations and save; delete a preset; create a new one.
   - **Expected:** the list updates, and the import window offers the same presets.
6. **Library:** click **Open another library…** and pick another library folder.
   **Expected:** the app reloads showing that library.

---

## H. macOS

Repeat B–G on macOS 14+.
- The viewer's RAW decode uses LibRaw's macOS build. The setup script removes the quarantine flag from it.
- Ctrl is ⌘ for Ctrl+O, Ctrl+T and Ctrl+,.
- Open in uses `open`, or `open -a <App>` for an `.app` name.
- There are no automated real-app tests on macOS; the mocked browser tests cover the UI there.
