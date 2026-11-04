# Keepsake 0.1.0

The first release of Keepsake: a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer.

**This release is for Windows 11 (x64 and ARM64).** macOS will follow in a later release.

## Install

Download the installer for your PC:

| PC | File |
|---|---|
| Most PCs (Intel or AMD) | `Keepsake_0.1.0_x64_en-US.msi` or `Keepsake_0.1.0_x64-setup.exe` |
| ARM PCs (Snapdragon) | `Keepsake_0.1.0_arm64_en-US.msi` or `Keepsake_0.1.0_arm64-setup.exe` |

The installers are **not code-signed yet**, so Windows SmartScreen may warn you. Click **More info → Run anyway**.

Everything Keepsake needs is included: FFmpeg, ExifTool and LibRaw. Nothing else needs downloading, and Keepsake never connects to the internet.

## What it does

**Import cards safely**
- Insert a Sony card and Keepsake recognises it, even in another reader, and shows what is new since the last import.
- Import to two drives at once:
  - every copy is checked (BLAKE3) before it gets its final name;
  - files are renamed by template;
  - presets save your usual destinations;
  - live thumbnails come straight from the card.
- **SAFE TO FORMAT** appears only when every item is verified on two separate disks.
- An interrupted import (a pulled card, a crash, Stop) resumes where it left off and never leaves half-written files.

**Browse a large library**
- A calendar (years, months, days), a grid and a details table. They stay smooth at 300,000 items.
- Search by words, date, camera, lens, type and rating.
- Ratings, colour labels, comments and keywords are saved in standard XMP sidecars. Keepsake reads existing Lightroom and digiKam sidecars and keeps their other fields.
- Items on unplugged drives keep their thumbnails, marked offline.

**View**
- A full-screen viewer:
  - photos zoom to 100%, with RAW files decoded at full size;
  - RAW + JPEG pairs switch with R;
  - videos have frame stepping and the camera's timecode.
- Open any item in another application (Ctrl+O), set per file type.

**Fix times**
- Correct a camera clock for a whole shoot: pick a photo, enter the right time, check the preview, apply. Only the sidecars change.

**Back up and verify**
- Incremental, verified backups to another drive, with a copy of the library.
- Verify a drive at any time.
- A history of every backup and check, with the date each drive was last verified.

**Settings, logs and help**
- Settings cover the cache location and size, workers, Open in applications and import presets.
- Logs and crash reports stay on your computer. **Settings → Copy diagnostic report** gives you a text to paste into a bug report; your home folder is shown as `~`.

**Command line**
- `keepsake` imports, verifies, searches, shifts times, backs up and rebuilds the catalog. Every command has `--json`. See `docs/CLI.md`.

## Your files are safe

- Keepsake **never modifies, moves or deletes your original photos and videos**. Edits go into XMP sidecars and the library.
- Cards and read-only drives are never written to. Tests lock them at the operating-system level and check every file is byte-for-byte unchanged.
- Every place Keepsake writes to disk is listed, with its test, in `docs/SAFETY.md`.

## Known limits

- **Windows only** in this release. macOS needs an LGPL FFmpeg build for macOS first.
- **Unsigned installers** (see Install).
- On ARM PCs, LibRaw (full-size RAW) runs as an x64 program under Windows' emulation. It works, a little slower.
- Cameras that only connect over MTP (no card over USB) are not supported yet; use a card reader.
- Full-size HEIF viewing is untested: we have no HEIF sample files yet.
- Not yet included (planned): trimming and joining clips, export, the map view, on-device tagging and face grouping, slideshows.

## Licences

Keepsake runs these programs as separate processes; they are not built into Keepsake. Their licences are in the install folder:
- **FFmpeg**: LGPL v3 (`licenses\FFmpeg-LICENSE.txt`).
- **LibRaw 0.22.2**: LGPL 2.1 or CDDL 1.0 (`libraw\`).
- **ExifTool** by Phil Harvey: the same terms as Perl.
