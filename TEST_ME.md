# Manual Test Steps — Prompt 12: hardening and release 0.1.0

Test on **Windows 11** (primary) and **macOS 14+** (sections A, C–F; the release is Windows only).

**Build:**
- Run `scripts/setup.ps1` (or `setup.sh`) once. It now also puts `LICENSE-ffmpeg.txt` in `vendor/`.
- `cargo build -p keepsake-app -p keepsake-cli`
- For the timing checks: `cd app/ui; pnpm tauri build --no-bundle` and `cargo build --release -p keepsake-cli`.

To keep your real settings and logs apart:
```powershell
$T = "$env:TEMP\ks12"; $env:KEEPSAKE_CONFIG_DIR = "$T\config"; $env:KEEPSAKE_LOG_DIR = "$T\logs"
```

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass. New this time:
   - `source_safety_tests`: a card and a read-only volume, locked by the OS, are byte-for-byte unchanged after everything that reads them;
   - `safety_audit_tests`: every write to disk is listed in `docs/SAFETY.md`;
   - `tools_tests`, `library_writes_tests`;
   - the `logs` and `diag` unit tests;
   - the CLI crash and corpus tests.
2. `cargo test --release -p keepsake-core --test scale_tests -- --ignored --nocapture`
   **Expected:** seeds 300,000 items (about a minute), then prints a table.
   - Every search is under 300 ms, the calendar under 300 ms, and the catalog side of start-up under 1 s.
   - On the reference PC: searches 0–115 ms, the calendar at most about 170 ms, start-up about 0.4 s.
3. In `app/ui`:
   - `pnpm test` **Expected:** 15 unit tests pass.
   - `pnpm test:e2e` **Expected:** 38 browser tests pass, including `keyboard.spec.ts` (every flow without a mouse) and `a11y.spec.ts` (axe in light and dark).
   - On Windows: `pnpm test:app` **Expected:** the real-app tests pass.
   - With the release builds: `$env:KEEPSAKE_APP_PROFILE="release"; pnpm test:app -- perf.spec.ts` prints start-up, search, calendar and scrolling at 300,000 items; all within the limits.
4. `pnpm lint`, `pnpm format:check`, `cargo fmt --check`, `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no problems.

---

## B. The installed app (Windows; the release check)

1. Install `Keepsake_0.1.0_x64_en-US.msi` from the draft release (or `pnpm tauri build` locally) on a PC **without** the source code or `vendor/`.
   **Expected:**
   - SmartScreen may warn (the installers are unsigned): **More info → Run anyway**.
   - `C:\Program Files\Keepsake\` holds `ffmpeg.exe`, `ffprobe.exe`, `exiftool\`, `libraw\` and `licenses\`.
2. In PowerShell:
   ```powershell
   & "C:\Program Files\Keepsake\keepsake-app.exe" --self-check "$env:TEMP\check.json"; $LASTEXITCODE; Get-Content "$env:TEMP\check.json"
   ```
   **Expected:** exit code `0`, and four tools, each with a version.
3. Start Keepsake from the Start menu and create a library. Import a card or a folder of photos and a video.
   **Expected:**
   - thumbnails appear;
   - the video plays in the viewer;
   - Z zooms a JPEG to 100%;
   - on a RAW + JPEG pair, R shows the RAW, decoded at full size.

   Before this release, an installed copy could do none of this: it could not find its tools.
4. On an ARM PC, repeat with the ARM64 installer.
   **Expected:** the same. The RAW decode runs under emulation, a little slower.

---

## C. A large library

1. Make a throwaway 300,000-item library:
   ```powershell
   keepsake init "$T\big"; keepsake --library "$T\big" dev seed --assets 300000
   ```
   Open it in the app (Settings → Open another library, or first run).
   **Expected:**
   - the window shows thumbnails within about 1–1.5 s of starting;
   - typing "lake" in search updates the count about a third of a second after you stop typing;
   - the calendar opens at once;
   - scrolling the grid quickly is smooth, without stutter.
2. Press Ctrl+A, then End.
   **Expected:** instant, still smooth.

---

## D. Data safety

1. Read `docs/SAFETY.md`.
   **Expected:** every place Keepsake writes to disk, with what, where and the test.
2. Lock a card copy against writing:
   - Windows: right-click the folder → Properties → Security → Edit → Everyone → Deny **Write**.
   - macOS: `chmod -R a-w`.

   Then import it, and add it as a library volume with `keepsake rebuild-catalog`. View items, rate them and back up.
   **Expected:** everything works. Ratings and labels go to the library's mirrored sidecars. No file in the locked folder changes (compare `Get-FileHash` or `shasum` before and after).

---

## E. Logs, crashes and the diagnostic report

1. Start the app, then open `$T\logs` (Settings → Help and diagnostics → **Open log folder**).
   **Expected:** a `keepsake-<time>-<pid>.log` that says "Keepsake started" and, later, "start-up: window ready" with the time in ms.
2. Run `keepsake dev crash`, then start the app.
   **Expected:**
   - a `crash-….txt` file in the log folder;
   - in the app, the notice "Keepsake closed unexpectedly last time." with the file name;
   - **Copy diagnostic report** copies the report; **Dismiss** hides the notice, and it does not come back after a restart (the file is renamed `….seen.txt`).
3. Settings → **Copy diagnostic report**, then paste it into Notepad.
   **Expected:**
   - the version, OS, the four tools with versions, library counts, settings, the last crash and the last log lines;
   - your user folder appears as `~`, never as `C:\Users\<you>`.
4. Disconnect from the network for all of the above.
   **Expected:** nothing changes. Keepsake never connects anywhere.

---

## F. Accessibility

1. **Keyboard only** (put the mouse away). Do each of these:
   - first run;
   - browse with the arrows; Ctrl+1/2/3 to switch views; Ctrl+F to search; Tab through the filters;
   - F6 to the inspector; type a comment; rate with 0–5;
   - Enter to open the viewer; arrows and Esc;
   - Ctrl+T to shift times;
   - the import window (Tab, Space to tick an item, Enter to start);
   - the sidebar arrows to Backup; start a backup; Verify now;
   - Ctrl+, for Settings; save.

   **Expected:**
   - every control can be reached;
   - the focus ring is always visible;
   - Tab stays inside the viewer and the time-shift dialog, and closing them returns focus to the item;
   - in the calendar, Enter opens one level at a time and Backspace goes back up.
2. **Screen reader.** Turn on Narrator (Ctrl+Win+Enter), or VoiceOver on macOS (⌘F5). Move through the grid with the arrows.
   **Expected:**
   - each item is read, e.g. "DSC01234.ARW, RAW photo, 2024-06-15 14:02, 4 stars, Red label";
   - progress, results and errors are announced;
   - every button has a name.
3. **Contrast.** Check both themes (Windows Settings → Personalization → Colors → Light/Dark; macOS Appearance).
   **Expected:**
   - text is readable everywhere: blue buttons, the orange "offline" badges, grey text on a selected row;
   - axe passes on every screen, in both themes (A.3).

---

## G. The release (product owner)

1. After the stack is merged and `v0.1.0` is pushed, the **Release** workflow runs.
   **Expected:**
   - x64 and ARM64 installers are built;
   - the smoke test installs the MSI, checks the bundled files and licences, runs the installed tools and `keepsake-app --self-check`;
   - a **draft** release appears with four installers and the notes from `docs/RELEASE_NOTES.md`.
2. Check the draft: installer names, notes, the signing caveat. Then publish it on GitHub (Releases → Edit → Publish).
