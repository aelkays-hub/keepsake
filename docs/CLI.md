# keepsake(1) — Keepsake command line

## NAME

**keepsake** — import camera cards safely, verify, search, shift times and back up a local photo and video library.

## SYNOPSIS

```
keepsake [--library PATH] [--json] <command> [options]

keepsake init <library-path>
keepsake ingest <source> --dest A [--dest B] [--template T] [--event E] [--label L] [--all] [--dry-run] [--no-probe]
keepsake ingest --resume [--session ID] [--no-probe]
keepsake ingest --status [--session ID]
keepsake ingest --list
keepsake verify [--volume X]
keepsake search [QUERY] [--from DAY] [--to DAY] [--camera TEXT] [--type photo|video] [--limit N]
keepsake shift-time [PATH…] --by ±H:MM[:SS] [--query Q] [--from DAY] [--to DAY] [--camera TEXT] [--type T] [--dry-run]
keepsake backup <dest> [--label L]
keepsake rebuild-catalog [--fresh] [--no-probe]
keepsake probe <folder>
keepsake watch [--auto-ingest] [--no-probe]
keepsake card list | trust <card> --preset P | untrust <card> | rename <card> <name> | forget <card>
keepsake preset save <name> --dest A [--dest B] [--template T] [--event E] | list | delete <name>
keepsake renders status
keepsake renders run [--workers N] [--proxy-workers N] [--encoder NAME] [--ignore-battery] [--keep-running] [--queued-only]
keepsake renders enqueue (--all | PATH… | --query Q | --from/--to/--camera/--type …) [--visible]
keepsake renders show <file>
keepsake renders retry
keepsake renders evict [--max-bytes N]
keepsake renders bench <folder> [--per-type N] [--videos N] [--workers N] [--encoder NAME]
```

## DESCRIPTION

Keepsake keeps your originals untouched. Everything you change (ratings, time shifts, …) is written to XMP sidecar files and to the catalog. The catalog is a database that can always be rebuilt from the files and their sidecars.

A **library** is a folder holding the catalog (`catalog.db`), the volume registry, edit recipes and mirrored sidecars. Media lives on **volumes**: the drives or folders you import to.

## GLOBAL OPTIONS

`--library PATH`
: Use this library. Without it, Keepsake uses `KEEPSAKE_LIBRARY`, then the default saved by `keepsake init`.

`--json`
: Print one JSON document on standard output instead of text (`watch` prints one per line), and draw no progress bars. Errors are printed to standard error as `{"error": "…"}`.

`-h`, `--help`; `-V`, `--version`
: Show help or the version.

## COMMANDS

### init

`keepsake init <library-path>`

Creates a library in the folder (creating the folder if needed) and makes it the default. If the folder already holds a library, `init` adopts it as the default and changes nothing in it.

JSON: `library`, `id`, `created`, `default`, `config`.

### ingest

`keepsake ingest <source> --dest A [--dest B] [options]`

Imports a camera card (or a copy of one) to one or two destinations:

1. Groups the files into assets: clips with their XML, proxy and thumbnail; RAW + JPEG + HEIF; AVCHD segments.
2. Reads capture dates.
3. Skips what is already in the library.
4. Names every file with the template.
5. Copies each file once to both destinations. Each copy goes into a hidden temporary file, is flushed to disk and read back past the operating system's cache, then renamed into place.
6. Reads every final file again, catalogues the assets and writes XMP sidecars next to them.

The card is **never written to**. A destination may not be inside the card, on the same disk as the card, or on the same disk as the other destination.

**SAFE TO FORMAT** is printed only when every selected asset is verified on two different destinations. Assets you did not select are listed with a warning, because formatting the card would lose them.

When the source is a whole Sony card (a drive or volume, not a folder copy), Keepsake remembers the card by its file-system serial. The next time it is inserted, in any reader or slot, `watch` shows how many items are new since this import (see `card`).

Options:

`--dest PATH` (once or twice)
: Destination folder or drive. Two are needed for "safe to format".

`--template T`
: File-naming template (default `{yyyy}/{yyyy}-{mm}-{dd}/{orig}`). Tokens:
  - `{yyyy} {mm} {dd} {hh}`: capture time on the camera's clock.
  - `{event}`: the `--event` value.
  - `{camera}`, `{lens}`.
  - `{seq}`: a 4-digit counter per folder, in capture order.
  - `{orig}`: the camera's file name without its extension.

  `/` makes folders. The extension is always kept. Undated files go under `0000-undated`. Characters that are invalid on Windows or macOS are replaced. Sony sidecars, proxies, thumbnails and RAW siblings keep the new name of their original.

`--event E`
: Value for `{event}`.

`--label L`
: Name of the card volume (default `Card`).

`--all`
: Also copy assets that are already in the library.

`--dry-run`
: Show what would be copied and where, and the space needed and free on each destination. **Writes nothing**: no session, no folders, no volume registration.

`--no-probe`
: Don't read capture dates with ExifTool/ffprobe.

`--resume [--session ID]`
: Continue an interrupted import (the latest unfinished one by default). Leftover temporary files are removed and the files they belonged to are copied again from the start. Files already renamed are re-checked before they count.

`--status [--session ID]`
: Show the state of an import and whether the card is safe to format.

`--list`
: List all imports.

JSON (import or resume): `session`, `report` (`assets`, `by_state`, `unselected`, `bytes_copied`, `failed`, `safe_to_format`, `not_safe`), and `cancelled` if stopped. Dry run: `dry_run`, and `plan.assets[]` (`original`, `diff`, `state`, `files[]` with `source`, `dest`, `role`, `size`) plus `plan.space[]`.

### verify

`keepsake verify [--volume X]`

Re-reads every catalogued file on every mounted volume (or only volume `X`, by label or id). Files are hashed past the operating system's cache, and each file is reported as **missing**, **changed** (content differs from what was imported) or **unreadable**. Volumes that are not mounted are listed and not checked. Verify reads only; it never repairs or rewrites anything.

Each checked volume is recorded in the library's history (`history.json`), which the app's Backup screen shows as "last verified".

JSON: `volumes[]` (`id`, `label`, `mount`, `files_checked`, `bytes_checked`, `ok`), `issues[]` (`volume`, `path`, `asset`, `kind`: `missing`/`changed`/`unreadable`, plus `expected`/`actual` or `error`), and `cancelled`.

### search

`keepsake search [QUERY] [filters]`

Finds assets whose file names, comments or tags contain every word of `QUERY` (prefix match, accents ignored). Filters narrow the result, and can be used without a query:

`--from DAY`, `--to DAY`
: Capture day in local time, `YYYY-MM-DD`, inclusive.

`--camera TEXT`
: Camera model contains `TEXT` (case-insensitive).

`--type photo|video`
: Kind of asset.

`--limit N`
: At most `N` results (default 100).

Results with words are best matches first; without words, oldest first.

JSON: `query`, `count`, `results[]` (`asset`, `file`, `volume`, `kind`, `captured`, `camera`, `rating`, `label`).

### shift-time

`keepsake shift-time [PATH…] --by ±H:MM[:SS] [filters] [--dry-run]`

Shifts the capture time of the selected assets, for example when the camera clock was wrong. It writes the XMP sidecars of every copy, plus the catalog; **originals are never touched**. Shifts add up.

Selection, which can be combined:
- `PATH`: a catalogued file, or a folder, meaning every asset with a file inside it.
- `--query`, `--from`, `--to`, `--camera`, `--type`: the same filters as `search`.

At least one is required. A path that isn't in the catalog is an error.

`--by` examples: `+01:00:00`, `-0:30`, `+25:00:00`.

`--dry-run`: show old and new times without writing.

JSON: `by_seconds`, `dry_run`, `changed[]` (`asset`, `file`, `before`, `after`), `skipped[]` (assets without a capture time).

### backup

`keepsake backup <dest> [--label L]`

Copies the whole library to `dest` (a drive or folder, created if missing). Every file is copied from a mounted volume that holds it, keeping its path, with the same verified copy method as `ingest`.

The backup is **incremental**: files already on the backup with the same content are skipped, so run it as often as you like. A file is copied only if its source still matches the catalog. A damaged copy is reported, not spread; run `verify` to find out more.

The library itself (a consistent copy of the catalog, the volume registry, the backup history, recipes and mirrored sidecars) is written to `dest/.keepsake-library/`. Every run is recorded in the library's `history.json`.

JSON: `destination`, `assets`, `files_copied`, `bytes_copied`, `files_already_there`, `failed[]` (`[path, error]`), `unavailable[]` (assets with no mounted copy), `library_snapshot`, `stale_temporaries_removed`, `cancelled`.

### rebuild-catalog

`keepsake rebuild-catalog [--fresh] [--no-probe]`

Rebuilds the catalog from the files and XMP sidecars on every registered, mounted volume. Card volumes are skipped, because they are import sources. `--fresh` deletes the database first. Your ratings, labels, comments, tags, time shifts and events come back from the sidecars.

JSON: `fresh`, `assets`, `volumes[]` (`id`, `label`, `online`, `report`).

### probe

`keepsake probe <folder>`

Shows what Keepsake reads from a card or folder, without changing anything: assets and their files, capture time with source and confidence, camera, lens, duration, video format, timecode, and burst/bracket/interval sequences.

### watch

`keepsake watch [--auto-ingest] [--no-probe]`

Runs until Ctrl-C and reports cards as they are inserted and removed. A card already inserted when `watch` starts is reported too. For each Sony card it shows:
- the name and mount point;
- the layouts found: XAVC (`PRIVATE/M4ROOT`), AVCHD, MP4 (`MP_ROOT`), stills (`DCIM/nnnMSDCF`);
- whether the card is new or known;
- for a known card, **how many items are new since its last import**.

Other drives are ignored. Nothing is ever written to a card.

A card is recognised by its file-system serial number, so it is the same card in any reader, slot or drive letter. Formatting a card in the camera gives it a new serial, so it then counts as a new card.

`--auto-ingest`
: Also import **trusted** cards automatically, using the card's preset (see `card trust` and `preset`). An import happens only if the card has new items and every preset destination folder exists, meaning the drive is connected. Otherwise `watch` says why it skipped.

  The import is the same as `ingest`: new items only, verified copies, the same safety rules, SAFE TO FORMAT reported at the end. Cards are imported one at a time.

  If the card is pulled out mid-import, the files not yet copied are reported as failed and still count as new. The import can be continued with `ingest --resume`.

`--no-probe`
: Don't read capture dates during automatic imports.

JSON: **one JSON object per line** (JSON Lines), each with an `event` field:

| `event` | Fields |
|---|---|
| `watching` | `auto_ingest` |
| `card_inserted` | `card`, `label`, `mount`, `known`, `layouts[]`, `items`, `new_items`, `last_import_ms`, `trusted`, `preset` |
| `card_removed` | `card`, `label`, `mount` |
| `volume_ignored` | `mount`, `reason` |
| `auto_ingest_skipped` | `card`, `reason` |
| `auto_ingest_started` | `card`, `session`, `preset`, `destinations[]` |
| `auto_ingest_progress` | `card`, `session`, `files_done`, `bytes_done` |
| `auto_ingest_finished` | `card`, `session`, `report` (as for `ingest`), `new_items_left` |
| `auto_ingest_cancelled` | `card`, `session` |
| `auto_ingest_failed` | `card`, `session`, `error`, `resumable` |
| `error` | `mount`, `error` |
| `stopped` | |

Exit status: 0 when stopped with Ctrl-C between imports, and 130 if Ctrl-C interrupted an automatic import.

### card

`keepsake card list`
: The cards this library has seen: name, id, when last seen and where, last import, and whether the card is trusted.

`keepsake card trust <card> --preset P`
: Import this card automatically with preset `P` while `watch --auto-ingest` runs. A preset with one destination works, but it is never safe to format.

`keepsake card untrust <card>`
: Stop importing it automatically.

`keepsake card rename <card> <name>`
: Name the card. Cards are first named after their volume label, which is often `Untitled`.

`keepsake card forget <card>`
: Forget the card and its import history. Its media and import sessions stay in the library. If it is inserted again, it is a new card.

`<card>` is the card's name (case-insensitive), its id, or the first 4 or more characters of the id.

JSON: `cards[]` for `list`, each with `id`, `key`, `label`, `layouts`, `capacity`, `hw_serial`, `hw_model`, `first_seen_ms`, `last_seen_ms`, `last_mount`, `last_import_ms`, `last_session`, `trusted`, `preset`. The other commands print `card`, or `forgotten` for `forget`.

### preset

`keepsake preset save <name> --dest A [--dest B] [--template T] [--event E]`
: Save (or replace) named import settings. The destination folders must exist. Two different destinations are needed for safe to format.

`keepsake preset list`
: List the saved presets.

`keepsake preset delete <name>`
: Delete a preset. Refused while a trusted card uses it.

JSON: `preset` (`name`, `destinations[]`, `template`, `event`), `presets[]`, or `deleted`.

### renders

Thumbnails (256 and 1024 px), previews (2560 px, never enlarged) and video proxies (720p H.264 with a one-second GOP, or the camera's own H.264 proxy copied unchanged). They are made in the background into a cache folder (see FILES). Originals are only read.

The work runs from a queue that survives restarts, in this order:
1. assets on screen in the app;
2. recent imports (`ingest` queues what it imports);
3. everything else.

Workers pause on battery power and while an import is running (in any Keepsake process). They finish the job in hand first.

How the full-size picture is obtained:
- JPEG, PNG, TIFF: decoded.
- RAW (ARW, DNG): the largest embedded JPEG. For Sony ARW that is 1616 px.
- HEIF: decoded by FFmpeg.
- Video: the frame at 10 % of the duration.

A RAW + JPEG pair uses the JPEG.

`keepsake renders status`
: Jobs done, queued (visible / recent / backlog), running and failed, with the errors; cache use against its limit; and whether workers would pause right now.

`keepsake renders run`
: Queue the backlog (every asset not queued yet; `--queued-only` skips this), then work through the queue with a progress bar until it is empty.

  `--keep-running`
  : Wait for new jobs until Ctrl-C.

  `--workers`, `--proxy-workers`
  : Override the settings.

  `--encoder`
  : Use this H.264 encoder (`h264_nvenc`, `h264_qsv`, `h264_amf`, `h264_mf`, `libopenh264`, `h264_videotoolbox`). Otherwise the first that works on this machine is used.

  `--ignore-battery`
  : Keep working on battery.

  Ctrl-C finishes the current job(s) and stops. Media that is offline is put back for later, not failed. Other failures retry twice (after 30 s and 60 s) and then stay failed until `retry`.

`keepsake renders enqueue`
: Queue renditions for some assets: `--all` (the backlog), or catalogued files and folders, or the same filters as `search`. Assets whose renditions are missing (evicted, deleted) are queued again. `--visible` puts them first.

`keepsake renders show <file>`
: The cache paths of a catalogued file's renditions.

`keepsake renders retry`
: Queue failed jobs again.

`keepsake renders evict [--max-bytes N]`
: Trim the cache now: least recently used first, down to the limit. Renditions whose media is offline are never removed.

`keepsake renders bench <folder>`
: Copy a sample of `folder` (spread across sizes; `--per-type` photos per type, `--videos` clips) into a temporary library. Then time each rendition type on this machine and print a Markdown table of items per minute and proxy speed in × real time, for `docs/PERFORMANCE.md`. The folder is only read.

JSON:
- `status`: `status` (`jobs` with `by_kind` and `queued_by_priority`, `running[]`, `failed[]`, `cache` with `root`/`files`/`bytes`/`max_bytes`/`kept_offline_bytes`, `paused`) and `failed_files[]`.
- `run`: JSON Lines events, each with `event` set to `started`, `done` (with `made[]`, `ms` and `encoder`), `failed` (with `retry`), `deferred`, `paused` (with `reason`: `on_battery`/`ingesting`), `resumed`, `evicted` or `finished` (with `done`, `failed` and `cancelled`).
- `enqueue`: `queued`.
- `show`: `asset` and `renditions` (variant → path).
- `retry`: `requeued`.
- `evict`: `evict`.
- `bench`: `machine`, `workers`, `encoder`, `results[]` (`type`, `items`, `seconds`, `items_per_minute`, `note`) and `failures[]`.

Exit status for `run`: 0, 2 if some jobs failed for good, 130 if stopped with Ctrl-C.

## CTRL-C

During `ingest`, `verify`, `backup`, `renders run` and automatic imports in `watch`, the **first Ctrl-C** lets the file being copied or checked finish (it is verified and recorded), then the command stops cleanly with exit status 130. Nothing partial is left under a final name. The **second Ctrl-C** stops at once. Either way, `ingest --resume` (or running `backup` again) continues safely.

## EXIT STATUS

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Error (bad arguments, missing library, refused unsafe destination, …) |
| 2 | Finished, but with problems: `verify` found missing or changed files; `ingest` is not safe to format; `backup` had failures or unavailable assets; `rebuild-catalog` reported problems |
| 130 | Stopped with Ctrl-C |

## ENVIRONMENT

`KEEPSAKE_LIBRARY`
: Library to use when `--library` is not given.

`KEEPSAKE_CONFIG_DIR`
: Folder for `config.toml` (default: the OS configuration folder).

`KEEPSAKE_VENDOR`, `KEEPSAKE_EXIFTOOL`, `KEEPSAKE_FFPROBE`, `KEEPSAKE_FFMPEG`
: Where to find ExifTool, ffprobe and ffmpeg (default: the nearest `vendor/` folder; ffmpeg sits next to ffprobe).

`RUST_LOG`
: Diagnostic logging to standard error (for example `RUST_LOG=keepsake_core=debug`).

## FILES

`<config>/config.toml`
: Shared with the app. `default_library = "…"`, written by `init`. The optional `[render]` section holds:
  - `cache_dir`: where renditions go (ideally an SSD; default `%LOCALAPPDATA%\keepsake\cache\renditions` or `~/Library/Caches/keepsake/renditions`);
  - `max_bytes`: the cache limit (default 20 GiB);
  - `workers`: thumbnail/preview workers (default half the threads, 1–4);
  - `proxy_workers` (default 1);
  - `pause_on_battery` (default `true`). The config folder is `%APPDATA%\keepsake` on Windows and `~/Library/Application Support/keepsake` on macOS.

`<library>/catalog.db`
: The catalog (SQLite, WAL mode).

`<library>/volumes.json`, `library.json`, `recipes/`, `sidecars/`
: The volume registry, the library identity, edit recipes, and sidecars for read-only volumes.

`<library>/cards.json`, `<library>/cards/<id>.json`
: The cards this library remembers, and the items imported from each one. These are not in the catalog, so `rebuild-catalog --fresh` keeps them, and `backup` copies them.

`<library>/presets.json`
: Saved presets.

`<file>.<ext>.xmp`
: The sidecar beside each original.

`.<name>.keepsake-partial-<session>`
: A hidden temporary file during a copy; removed on resume.

## EXAMPLES

Set up a library and import a card to two drives, checking first:

```
keepsake init D:\Keepsake
keepsake ingest E:\ --dest F:\Photos --dest G:\Photos --dry-run
keepsake ingest E:\ --dest F:\Photos --dest G:\Photos
```

Name files by day and event:

```
keepsake ingest E:\ --dest F:\Photos --dest G:\Photos --template "{yyyy}/{yyyy}-{mm}-{dd} {event}/{hh}{seq}_{orig}" --event "Lake"
```

After an interruption:

```
keepsake ingest --resume
```

Check every drive, then one:

```
keepsake verify
keepsake verify --volume Photos
```

Find videos from one weekend shot on an A7 IV:

```
keepsake search --from 2024-06-15 --to 2024-06-16 --type video --camera 7M4
```

The camera clock was an hour behind for one folder:

```
keepsake shift-time F:\Photos\2024\2024-06-15 --by +01:00:00 --dry-run
keepsake shift-time F:\Photos\2024\2024-06-15 --by +01:00:00
```

Back up to a third drive (run it again any time; only new files are copied):

```
keepsake backup H:\KeepsakeBackup
```

Import a card automatically every time it is inserted:

```
keepsake preset save "Two drives" --dest F:\Photos --dest G:\Photos
keepsake watch                      # insert the card once: it is now known
keepsake card rename Untitled "A7 IV card 1"
keepsake card trust "A7 IV card 1" --preset "Two drives"
keepsake watch --auto-ingest
```

Script with JSON (PowerShell):

```
keepsake --json search --type video | ConvertFrom-Json
```
