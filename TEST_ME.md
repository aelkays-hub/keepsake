# Manual Test Steps — Prompt 7: the CLI

Test on **Windows 11 x64** (primary) and **macOS 14+** when available.
Run from the repo root. Build first with `cargo build -p keepsake-cli`, and run `scripts/setup.ps1` / `setup.sh` once so `vendor/` has ExifTool and ffprobe.
Below, `$K` is `.\target\debug\keepsake.exe` (Windows) or `./target/debug/keepsake` (macOS). The commands are PowerShell. On macOS use the same commands with `/tmp/ks` paths and `cp -R`.

Every command also accepts `--json`. Try a few with it: the output should be one JSON document and no progress bar. The full manual is `docs/CLI.md`.

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass, including `cli_tests` (12 passed). The Ctrl-C test takes about 10 seconds. If the console can't deliver Ctrl-Break, it prints `skipping` and passes.
2. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no output.

---

## B. Set up a test card and a library

```powershell
$T = "$env:TEMP\ks"; Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force "$T\card" | Out-Null
Copy-Item -Recurse tests\fixtures\dcim\DCIM, tests\fixtures\xavc\PRIVATE "$T\card\"
$env:KEEPSAKE_CONFIG_DIR = "$T\config"     # keeps your real config untouched
& $K init "$T\lib"
```
**Expected:** `Created library … and made it the default.`

Then:
- Run `& $K init "$T\lib"` again. **Expected:** it says the library already exists and is now the default. Nothing is changed.
- In a new shell, without `KEEPSAKE_CONFIG_DIR`, run `& $K search x`. **Expected:** an error telling you to run `keepsake init` or pass `--library`, and exit code 1 (`$LASTEXITCODE`).

(A real card works too: copy one into `$T\card`, or use the card's drive letter directly.)

---

## C. ingest

All of this is on one disk, so it needs the hidden testing flag `--unsafe-allow-same-disk`. With a real card and two real drives, leave the flag out.

1. Dry run:
   ```powershell
   & $K ingest "$T\card" --dest "$T\A" --dest "$T\B" --dry-run --unsafe-allow-same-disk
   ```
   **Expected:** a list of assets with their files and destination names, and the space needed and free on each destination. `$T\A` and `$T\B` **do not exist** afterwards, and `& $K ingest --list` shows no imports.
2. Leave out the flag and drop `--dry-run`.
   **Expected:** the import is refused because both destinations are on the same disk. Nothing is copied.
3. Run it for real, with the flag:
   ```powershell
   & $K ingest "$T\card" --dest "$T\A" --dest "$T\B" --unsafe-allow-same-disk
   ```
   **Expected:**
   - a progress bar;
   - every asset verified on both destinations;
   - **SAFE TO FORMAT**;
   - exit code 0;
   - files under `A\2024\2024-01-15\…` and the same under `B`, each with an `.xmp` beside it.

   The card is unchanged; compare `Get-ChildItem -Recurse "$T\card"` before and after.
4. Run step 3 again.
   **Expected:** everything is reported as already imported, and nothing is copied.
5. `& $K ingest --status` and `& $K ingest --list`.
   **Expected:** the last session, safe to format.
6. Make a second card, copying `$T\card` to `$T\card2`. Import it to one destination only: `--dest "$T\C" --unsafe-allow-same-disk --all`.
   **Expected:** it is copied but **NOT safe to format** (only one copy), with exit code 2.
7. Try a template:
   ```powershell
   --template "{yyyy}-{mm}-{dd} {event}/{seq}_{orig}" --event Lake
   ```
   Use it on another copy with `--all` and new destinations.
   **Expected:** folders like `2024-01-15 Lake\0001_C0001.MP4`. The clip's XML, proxy and thumbnail are renamed to match.

---

## D. Ctrl-C

You need a few big files so there's time to press Ctrl-C:
```powershell
New-Item -ItemType Directory -Force "$T\big\DCIM\100MSDCF" | Out-Null
1..8 | % { fsutil file createnew "$T\big\DCIM\100MSDCF\DSC0000$_.JPG" 300000000 | Out-Null }
& $K ingest "$T\big" --dest "$T\BA" --dest "$T\BB" --unsafe-allow-same-disk --no-probe
```
(macOS: `mkfile 300m /tmp/ks/big/DCIM/100MSDCF/DSC00001.JPG`, and so on.)

1. Press **Ctrl-C once** while the bar is moving.
   **Expected:**
   - `Stopping after the current file…`;
   - a moment later, `Stopped after the current file` with a resume hint, and exit code 130;
   - no `.keepsake-partial-*` files in `BA` or `BB` (use `Get-ChildItem -Force -Recurse`);
   - every file that is there has its full size.
2. `& $K ingest --resume`
   **Expected:** the rest is copied, then SAFE TO FORMAT.
3. Start a fresh import of `big` to new destinations, and press **Ctrl-C twice** quickly.
   **Expected:** it exits immediately. `ingest --resume` cleans up the leftover temporary file and finishes.

---

## E. verify

1. `& $K verify`
   **Expected:** each volume (A, B, C …) is listed with files checked, `All checked files are intact.`, and exit code 0. The card is not listed.
2. Delete one file from `A` and append a byte to another (use copies, never your real originals):
   ```powershell
   Add-Content "$T\A\2024\2024-01-15\<some file>" "x"
   ```
   **Expected:** `MISSING:` and `CHANGED:` lines naming them, and exit code 2. Nothing is repaired or rewritten.
3. `& $K verify --volume B`
   **Expected:** only B is checked, and it is OK.
4. `& $K verify --volume nope`
   **Expected:** an error saying the volume is unknown.

---

## F. search

1. `& $K search C0001`
   **Expected:** the clip, with its date, kind, camera and path.
2. `& $K search --from 2024-01-15 --to 2024-01-15 --type video`
   **Expected:** only that day's videos.
3. `& $K search --camera <part of the camera name shown in step 1>`
   **Expected:** matches, case-insensitively.
4. `& $K search --from 2030-01-01`
   **Expected:** `No matches.`
5. `& $K search`
   **Expected:** an error asking for words or a filter.
6. `& $K search --from 15/01/2024`
   **Expected:** an error about the date format.

---

## G. shift-time

1. `& $K shift-time "$T\A\2024\2024-01-15" --by +01:00:00 --dry-run`
   **Expected:** before → after times one hour apart, and nothing written. The `.xmp` files have the same modified time as before.
2. Run it again without `--dry-run`.
   **Expected:**
   - `Shifted N asset(s)… originals untouched`;
   - the `.xmp` files in **both** A and B now contain the new `photoshop:DateCreated` / `xmp:CreateDate`;
   - the media files are byte-identical to the card (their hashes are unchanged);
   - `& $K search --from 2024-01-15 --to 2024-01-15` shows the new times.
3. `--by -1:00:00` puts them back.
4. `& $K shift-time "$T\not-there" --by +1:00` **Expected:** "not in the catalog".
5. `& $K shift-time --by +1:00` **Expected:** an error asking for a selection.
6. `& $K shift-time --type video --by 1:5` **Expected:** an error about the format.

---

## H. backup

1. `& $K backup "$T\backup"`
   **Expected:**
   - every file copied with a bar, and exit code 0;
   - `$T\backup` mirrors the library's folders;
   - `$T\backup\.keepsake-library\catalog.db` exists.
2. Run it again.
   **Expected:** 0 copied, all already there.
3. Import another card (section C step 6 or 7), then back up again.
   **Expected:** only the new files are copied.
4. Damage a file on the volume the backup reads from (append a byte, as in E2). Then back up to a **new** folder, `"$T\backup2"`.
   **Expected:** that file is reported as differing from the catalog, with a hint to run `keepsake verify`. The damaged copy is not written, and the exit code is 2.
5. `& $K backup "$T\A\inside"`
   **Expected:** refused, because the backup would sit inside a library volume.
6. Press Ctrl-C during a big backup.
   **Expected:** the current file finishes, then exit code 130. Running `backup` again continues.

---

## I. rebuild-catalog

1. Note the output of `& $K search --type video`.
2. `& $K rebuild-catalog --fresh`
   **Expected:** the volumes are rebuilt (not the card), and the summary shows the asset count.
3. Run `search --type video` again.
   **Expected:** the same results, including the time shift from G and any ratings and tags.

---

## J. probe

`& $K probe "$T\card"`
**Expected:** the assets with their capture time, source and confidence, camera, duration and format. Nothing on the card changes.

---

## K. macOS-specific

Repeat B–J on macOS with `/tmp/ks`. Ctrl-C uses the normal terminal Ctrl-C. For a read-only source, test a real card, or a DMG mounted read-only:
```bash
hdiutil create -srcfolder /tmp/ks/card -format UDRO card.dmg && hdiutil attach card.dmg
```
Then ingest from `/Volumes/card`. **Expected:** the same results; nothing tries to write to the card.
