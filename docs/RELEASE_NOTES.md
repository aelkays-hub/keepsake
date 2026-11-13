# Keepsake 0.5.2

Keepsake is a local-first replacement for Sony PlayMemories Home. It imports camera cards safely, keeps a calendar-first library of photos and videos, and backs it up, all on your own computer. Version 0.5.2 lets Read aircraft look a registration up online, and carry what it finds to the same plane's other photos and to the rest of a burst. It also fixes the filters, Show in Explorer and Open in for network drives.

## New in 0.5.2

**Read aircraft**
- **Look registrations up online** (Settings → Read aircraft; off until you turn it on): each registration read is looked up once at the free adsbdb.com. Only the registration's text is sent, never a photo. The type and airline it knows are offered in the review window, marked "(adsbdb)"; a painted type it disagrees with is offered unticked. Aircraft no longer flying may be unknown there.
- **The same plane's other photos:** after you accept a photo's registration with its airline or type, the window offers them to your other photos of that registration that lack them (**Add to N photos**).
- **The rest of a burst:** photos taken by the same camera within 10 seconds that look alike (On-device AI's comparison) are ticked under the photo, and get the same keywords when you accept.
- **Names partly hidden** by an engine or a wing are read (Air Transat as `trainsa`), and a registry's company name counts as the airline ("Emirates Airline" → Emirates).

**Fixes**
- The filters at the top could leave the grid empty: the work that marks the photos on screen held the library up. It now runs on its own.
- Photos that cannot be rendered are no longer tried again every time they are on screen.
- **Show in Explorer** works for photos on a network drive, and **Open in** hands them over by drive letter (`Z:\…`), so Topaz Photo opens them. If Topaz Photo is already open, Keepsake says so: close it and choose Open in again.

**Upgrading from 0.5.1:** the first time 0.5.2 opens your library it updates the catalog (schema 17). Older versions cannot open it after that, so install 0.5.2 on every computer that opens the library.

## New in 0.5.1 (included)

**Read aircraft: you decide what is kept**
- Every Read aircraft (the photos you pick, the viewer, a folder, a year, month or day) opens a review window in the middle of the screen. Photos appear as they are read.
- Three sliders set how sure a **registration**, an **airline** and a **type** must be to be offered. Moving one re-sorts every photo at once (nothing is read again), and they are remembered.
- Each photo shows what was found, ticked, with how surely it was read, and what was read below your levels. Untick mistakes, then **Accept**, **Dismiss**, or **Accept all shown**. Nothing becomes a keyword until you accept it.

**Read aircraft reads more**
- Registrations painted under the wing (upside down in a photo from below) are read.
- Painted types read a little less clearly (`Boeing 777-300ER`) are kept.
- Airlines that no longer fly are known, for older photos: Jet Airways, Air Berlin, Kingfisher, Germanwings, Monarch, Thomas Cook, Virgin America, US Airways, Continental, Northwest.

**Also**
- The sidebar tree keeps its place after a right-click action and over a restart.

Photos read by Read aircraft before 0.5.1 keep their keywords; to review them in the window, read them again (a folder or dates does this by itself for photos read before).

**This release is for Windows 11 (x64 and ARM64).** macOS will follow in a later release.

## Install

Download the installer for your PC:

| PC | File |
|---|---|
| Most PCs (Intel or AMD) | `Keepsake_0.5.2_x64_en-US.msi` or `Keepsake_0.5.2_x64-setup.exe` |
| ARM PCs (Snapdragon) | `Keepsake_0.5.2_arm64_en-US.msi` or `Keepsake_0.5.2_arm64-setup.exe` |

The installers are **not code-signed yet**, so Windows SmartScreen may warn you. Click **More info → Run anyway**.

Everything Keepsake needs is included: FFmpeg, ExifTool and LibRaw. Keepsake connects to the internet only when you ask it to: to download an AI model you turn on in Settings, or to send a photo's small preview to Claude, ChatGPT or Gemini (Tag with AI, Ask in Edit suggestions).

**Upgrading from 0.1.0:** the catalog is updated the first time 0.5.2 opens your library; older versions cannot open it after that.

## New in 0.5.0 (included)

**Your existing folders**
- **Add a folder in place**: photos you already have are catalogued where they are, read-only. Large folders fill in as they are read and resume if stopped; full checks run in the background. **Scan again** (right-click a folder) finds new, changed and removed files.
- **By folder**: the sidebar shows your drives and folders as they are on disk, beside the calendar's years, months and days, each with its count. The tree keeps its place and comes back where you left it.
- **Every card imports by itself** into year › month › day, if you turn it on.

**Browsing**
- Thumbnail sizes (Small, Medium, Large), much faster thumbnails from a network drive, and thumbnails that stay when the cache is full.
- Select many items (tick-boxes, Select mode, drag a rectangle) to edit them together.
- The viewer: a zoom slider, faster zoom, previous and next, and an information panel with every metadata tag.
- **Show in Explorer / Finder**, and **Open in…** with several programs per file type (for example Photoshop and Topaz Photo).
- **Report** (Tools): your photos, videos and space by year, month, camera, lens, file type and drive; rows open their photos; export to CSV.

**AI, on this computer**
- **Tag photos**: tags such as Beach, Dog or Sunset from what a photo shows, for a folder, a date or everything, searchable like keywords.
- **Duplicates**: near-identical RAW photos of one day, grouped, so you can mark the keeper. Nothing is deleted.
- **Name and find**: name a face, and Keepsake suggests the other photos that person is in, across the years. You confirm each.
- **Read aircraft**: the registration, airline and type painted on aircraft, as keywords, for picked photos, a folder or dates.

**AI you ask for, from the cloud** (your own API key; you pay the provider)
- **Tag with AI**: Claude, ChatGPT or Gemini tag the photos you pick. Only a 1024-pixel preview is sent, never the original; the inspector shows which AI gave each keyword.
- **Edit suggestions** (right-click a photo): Keepsake measures the photo (brightness, highlights, shadows, contrast, colour, noise, sharpness, a tilted horizon) and lists what to improve, with the steps in Photoshop and in Topaz Photo. **Ask** an AI for advice that takes in what the photo shows. **Open in Photoshop** or **Open in Topaz Photo** from the same window.

## Also in Keepsake (since 0.1.0)


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

- **Windows only** in this release. The Mac version is built and tested automatically but not packaged yet.
- **Unsigned installers** (see Install).
- On ARM PCs, LibRaw (full-size RAW) runs as an x64 program under Windows' emulation. It works, a little slower.
- Cameras that only connect over MTP (no card over USB) are not supported yet; use a card reader.
- Full-size HEIF viewing is untested: we have no HEIF sample files yet.
- The AI features run on the graphics card where they can (DirectML), else the processor; their models are downloaded once when you turn them on.
- Noise and focus in Edit suggestions are judged from a small preview, so they are rough.
- Not yet included (planned): trimming and joining clips, export, the map view, a People screen, slideshows.

## Licences

Keepsake runs these programs as separate processes; they are not built into Keepsake. Their licences are in the install folder:
- **FFmpeg**: LGPL v3 (`licenses\FFmpeg-LICENSE.txt`).
- **LibRaw 0.22.2**: LGPL 2.1 or CDDL 1.0 (`libraw\`).
- **ExifTool** by Phil Harvey: the same terms as Perl.
