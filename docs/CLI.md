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
```

## DESCRIPTION

Keepsake keeps your originals untouched. Everything you change (ratings, time shifts, …) is written to XMP sidecar files and to the catalog. The catalog is a database that can always be rebuilt from the files and their sidecars.

A **library** is a folder holding the catalog (`catalog.db`), the volume registry, edit recipes and mirrored sidecars. Media lives on **volumes**: the drives or folders you import to.

## GLOBAL OPTIONS

`--library PATH`
: Use this library. Without it, Keepsake uses `KEEPSAKE_LIBRARY`, then the default saved by `keepsake init`.

`--json`
: Print one JSON document on standard output instead of text, and draw no progress bars. Errors are printed to standard error as `{"error": "…"}`.

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

The library itself (a consistent copy of the catalog, the volume registry, recipes and mirrored sidecars) is written to `dest/.keepsake-library/`.

JSON: `destination`, `assets`, `files_copied`, `bytes_copied`, `files_already_there`, `failed[]` (`[path, error]`), `unavailable[]` (assets with no mounted copy), `library_snapshot`, `stale_temporaries_removed`, `cancelled`.

### rebuild-catalog

`keepsake rebuild-catalog [--fresh] [--no-probe]`

Rebuilds the catalog from the files and XMP sidecars on every registered, mounted volume. Card volumes are skipped, because they are import sources. `--fresh` deletes the database first. Your ratings, labels, comments, tags, time shifts and events come back from the sidecars.

JSON: `fresh`, `assets`, `volumes[]` (`id`, `label`, `online`, `report`).

### probe

`keepsake probe <folder>`

Shows what Keepsake reads from a card or folder, without changing anything: assets and their files, capture time with source and confidence, camera, lens, duration, video format, timecode, and burst/bracket/interval sequences.

## CTRL-C

During `ingest`, `verify` and `backup`, the **first Ctrl-C** lets the file being copied or checked finish (it is verified and recorded), then the command stops cleanly with exit status 130. Nothing partial is left under a final name. The **second Ctrl-C** stops at once. Either way, `ingest --resume` (or running `backup` again) continues safely.

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

`KEEPSAKE_VENDOR`, `KEEPSAKE_EXIFTOOL`, `KEEPSAKE_FFPROBE`
: Where to find ExifTool and ffprobe (default: the nearest `vendor/` folder).

`RUST_LOG`
: Diagnostic logging to standard error (for example `RUST_LOG=keepsake_core=debug`).

## FILES

`<config>/config.toml`
: `default_library = "…"`, written by `init`. The config folder is `%APPDATA%\keepsake` on Windows and `~/Library/Application Support/keepsake` on macOS.

`<library>/catalog.db`
: The catalog (SQLite, WAL mode).

`<library>/volumes.json`, `library.json`, `recipes/`, `sidecars/`
: The volume registry, the library identity, edit recipes, and sidecars for read-only volumes.

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

Script with JSON (PowerShell):

```
keepsake --json search --type video | ConvertFrom-Json
```
