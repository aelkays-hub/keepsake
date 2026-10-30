# Data safety

Keepsake's first rule: **original media is never modified, moved or deleted.** Edits go to XMP sidecars, JSON recipes or new files. This page lists every place Keepsake's own code writes to disk, what it writes, where, and the test that covers it.

`crates/keepsake-core/tests/safety_audit_tests.rs` keeps the list complete. It scans the non-test code of every crate for anything that can write to disk:
- creating, writing, renaming, deleting or copying files and folders;
- opening a database;
- an FFmpeg output.

It finds the function each one is in, and fails unless a row below names that file and that function. It also fails if a test named here does not exist.

## Sources are never written

A **source** is a camera card or folder being imported, a folder added in place (Folders screen, `keepsake add-folder`) except as an import destination (below), or a library volume marked read-only (an archive disc, a locked drive). Nothing writes to a source. A folder added in place is always recorded read-only, so its sidecars go to the library's mirror, and adding it never runs the writable-volume probe below.

The tests ask the operating system to refuse every write to the source first: a deny ACL on Windows, `chmod a-w` elsewhere (`common::readonly::deny_writes`). An attempted write therefore fails loudly instead of being noticed afterwards. Then they compare every file's bytes, size and modification time, and the folder listing, with before.

| Proof | What runs against the locked source |
|---|---|
| `source_safety_tests.rs::a_card_is_only_read_by_everything_that_reads_a_card` | card scan for the import window, card thumbnails, the import pre-check, probing (ExifTool and ffprobe), an import to two drives, the same import again, an import stopped and resumed, card recognition |
| `source_safety_tests.rs::a_read_only_library_volume_is_only_read` | cataloguing with probing, every thumbnail, preview and proxy, the viewer's full-size pictures (LibRaw for RAW) and video facts, rating, label, comment, keywords and time shift (their sidecars go to the library's mirror), verify, backup, catalog rebuild |
| `add_folder_tests.rs::a_write_locked_folder_is_added_rated_and_scanned_again` | adding a folder in place, rating and commenting every item, scanning it again |
| `add_folder_tests.rs::a_folder_is_catalogued_in_place_and_never_written_to` | adding a folder (with a Lightroom sidecar) and rating an item: the folder's files and listing are exactly as before |
| `cli_tests.rs::add_folder_catalogues_in_place_and_writes_nothing_into_the_folder` | `keepsake add-folder`, twice |
| `ingest_tests.rs::source_mounted_read_only_is_never_written` | an import, twice (quick-diff then a full hash) |
| `rebuild_tests.rs::read_only_volume_is_never_written` | a catalog rebuild |
| `render_tests.rs::card_items_get_thumbnails_without_touching_the_card` | card thumbnails |
| `rebuild_tests.rs::originals_are_never_modified` | edits on a writable volume: only sidecars appear beside originals |

The sidecar tools follow the same rule:
- **ExifTool** is only ever asked to read (`-stay_open` with read arguments). Keepsake writes XMP itself.
- **FFmpeg** writes only to the output path it is given, always a temporary file in the rendition cache.
- **LibRaw** writes its picture to standard output (`-Z -`).

## Every write, by where it goes

### Import destinations (the two drives an import copies to)

Copies are written to a hidden temporary name, verified by BLAKE3 and only then renamed into place. Nothing partial ever has a final name.

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/ingest/copy.rs` `writer` `finalize` `remove_if_exists` | creates `.<name>.keepsake-partial-<session>`; renames it to the final name after verification; removes its own temporaries | `ingest_tests.rs::two_destinations_copy_verify_catalogue_and_report_safe` `ingest_tests.rs::cancel_finishes_the_current_file_and_leaves_no_partial_files` `ingest_tests.rs::rename_retries_then_fails_cleanly` |
| `crates/keepsake-core/src/ingest/mod.rs` `destination_volume` | creates the destination folder if missing. A folder added in place may be an import destination (a card copied into `E:\Pictures` by year, month and day): the import only adds new files and the folders for them, never changes or removes what is there, and the folder stays read-only for sidecars (they go to the library's mirror). A destination inside or holding a folder already in the library is refused | `ingest_tests.rs::refuses_unsafe_destinations` `auto_import_tests.rs::a_card_lands_in_year_month_day_folders_of_the_added_folder` `auto_import_tests.rs::a_destination_inside_an_added_folder_is_refused` |
| `crates/keepsake-core/src/ingest/mod.rs` `recover_items` | after a crash, finishes a verified temporary's rename, or sets an unverified one aside | `ingest_tests.rs::crash_mid_copy_then_resume` `ingest_tests.rs::crash_after_rename_then_resume` `ingest_tests.rs::process_killed_mid_copy_then_resumed` |
| `crates/keepsake-core/src/ingest/mod.rs` `verify_asset` | moves a final file that fails re-verification aside, under a hidden name (never deleted) | `ingest_tests.rs::final_file_failing_verification_is_moved_aside` `ingest_tests.rs::persistent_verify_mismatch_never_renames` |
| `crates/keepsake-core/src/library.rs` `probe_volume` | creates and deletes one hidden empty file, `.keepsake-probe-<id>`, at the root of a destination or a library volume being added, to learn whether it is writable and case-sensitive. Never on a source being imported. On a locked volume the create fails and the volume is recorded read-only | `source_safety_tests.rs::a_read_only_library_volume_is_only_read` `rebuild_tests.rs::read_only_volume_is_never_written` |

### Beside originals on writable library volumes: XMP sidecars only

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/metadata.rs` `sync_sidecars` · `crates/keepsake-core/src/xmp/sidecar.rs` `write_atomic` | `<original>.xmp` (rating, label, comment, keywords, capture time and shift, …), written to a temporary and renamed. For read-only or offline volumes it goes to the library's mirror (`sidecars/<volume>/…`) instead | `xmp_tests.rs::atomic_write_and_read_back` `rebuild_tests.rs::originals_are_never_modified` `cli_tests.rs::shift_time_writes_xmp_and_catalog_only` `tools.spec.ts::time shift writes the XMP sidecars and leaves the originals alone` |

### The library folder

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/library.rs` `create` `register_volume` `write_json_atomic` `write_bytes_atomic` | `library.json`, `volumes.json`, the `recipes/` and `sidecars/` folders; every JSON or TOML file is written to a temporary and renamed | `cli_tests.rs::init_creates_a_library_and_makes_it_the_default` `ingest_tests.rs::two_destinations_copy_verify_catalogue_and_report_safe` |
| `crates/keepsake-core/src/catalog/mod.rs` `open` `open_in_memory` | `catalog.db` (SQLite, WAL); the in-memory catalog writes nothing to disk | `catalog_tests.rs::opens_file_catalog_in_wal_mode_and_migrates_once` `catalog_tests.rs::case_sensitive_volume_allows_case_variants` |
| `crates/keepsake-core/src/catalog/mod.rs` `delete_files` | deletes `catalog.db` and its `-wal`/`-shm` for `rebuild-catalog --fresh` (the catalog is a cache of media plus sidecars) | `cli_tests.rs::rebuild_catalog_restores_search_results` |
| `crates/keepsake-core/src/cards.rs` `save` `record_import` `forget` | `cards.json` (the card memory), and a forgotten card's entry | `watch_tests.rs::trusted_card_imports_then_reports_nothing_new` `watch_tests.rs::forgetting_a_card_keeps_the_library` `cli_tests.rs::presets_and_cards_are_managed_from_the_cli` |
| `crates/keepsake-core/src/presets.rs` `save` `delete` | `presets.json` | `cli_tests.rs::presets_and_cards_are_managed_from_the_cli` `viewer.spec.ts::settings: import presets can be deleted and created` |
| `crates/keepsake-core/src/history.rs` `record` | `history.json` (backup and verify runs) | `history.rs::records_newest_first_and_finds_last_good_verify` `cli_tests.rs::backup_is_incremental_and_verified` |
| `crates/keepsake-core/src/metadata.rs` `save_recipe` `remove_recipe` | `recipes/<asset>/<recipe>.json` | `library_writes_tests.rs::recipes_are_written_and_removed_in_the_library_only` |
| `crates/keepsake-core/src/seed.rs` `seed` | a synthetic library (developers only, `keepsake dev seed`, refuses a non-empty library) | `browse_tests.rs::counts_and_pages_cover_the_whole_view_in_order` |

### Backup destinations

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/backup.rs` `copy_one` | each original and sidecar, through a verified temporary, renamed into place | `cli_tests.rs::backup_is_incremental_and_verified` `tools.spec.ts::backup copies and verifies, and both are in the history` |
| `crates/keepsake-core/src/backup.rs` `snapshot_library` `copy_tree` · `crates/keepsake-core/src/catalog/mod.rs` `snapshot_to` | `<backup>/.keepsake-library/`: a consistent copy of the catalog (`VACUUM INTO`) and the library files, built beside the old copy and swapped in | `cli_tests.rs::backup_is_incremental_and_verified` |
| `crates/keepsake-core/src/backup.rs` `remove_stale_temps` | deletes only its own `*.keepsake-partial-backup` files left by a stopped backup | `library_writes_tests.rs::a_backup_removes_only_its_own_unfinished_files` |

### The rendition cache (thumbnails, previews, proxies; can always be made again)

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/render/cache.rs` `temp_path` `put_bytes` `put_file` `clear_temp` | renditions through `<cache>/.tmp`, renamed into `<kind>/<aa>/<hash>-<recipe>.<ext>`; leftover temporaries | `render_tests.rs::cache_paths_are_content_addressed` `render_tests.rs::the_service_renders_everything_once_and_reports_progress` |
| `crates/keepsake-core/src/render/cache.rs` `evict` | deletes least-recently-used renditions over the size limit (never those of offline media) | `render_tests.rs::eviction_is_lru_and_keeps_offline_media` |
| `crates/keepsake-core/src/render/cache.rs` `remove_files` | after the cache moved, deletes Keepsake's own sub-folders of the old cache folder, nothing else | `render_tests.rs::removing_an_old_cache_keeps_everything_that_is_not_ours` |
| `crates/keepsake-core/src/render/card.rs` `card_thumbnail` `sweep` | `<cache>/card/`: thumbnails of items still on a card (never on the card); swept after a week | `render_tests.rs::card_items_get_thumbnails_without_touching_the_card` |
| `crates/keepsake-core/src/render/service.rs` `run` | creates the cache folder | `render_tests.rs::the_service_renders_everything_once_and_reports_progress` |
| `crates/keepsake-core/src/render/still.rs` `ffmpeg_inputs` `picture` | a temporary FFmpeg segment list and scratch frame in the cache | `render_tests.rs::avchd_segments_are_joined_before_encoding` `render_tests.rs::video_poster_is_the_frame_at_ten_percent_and_follows_rotation` |
| `crates/keepsake-core/src/render/video.rs` `proxy_args` `make_proxy` `copy_camera_proxy` | FFmpeg encodes a proxy to a cache temporary (`-y` overwrites only that temporary); a camera's own H.264 proxy is copied in | `render_tests.rs::proxies_are_720p_h264_with_one_second_gops` `render_tests.rs::camera_sub_proxy_is_copied_when_h264_and_replaced_when_hevc` |

### The settings folder

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/settings.rs` `save` | `config.toml` | `cli_tests.rs::init_creates_a_library_and_makes_it_the_default` `tools.spec.ts::Open in runs the program set for the type; settings are saved` |
| `app/ui/src-tauri/src/prefs.rs` `settings_set` · `app/ui/src-tauri/src/state.rs` `set_library` `reload_settings` | creates the chosen cache folder | `tools.spec.ts::Open in runs the program set for the type; settings are saved` `app.spec.ts::first run creates a library` |

### The log folder (stays on this computer)

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-core/src/logs.rs` `create` `prune` · `app/ui/src-tauri/src/diagnostics.rs` `init` | one log file per run, the newest 10 kept, 10 MB each at most | `logs.rs::one_file_per_run_capped_and_pruned` `diagnostics.spec.ts::a crash is reported locally at the next start; the report has real facts` |
| `crates/keepsake-core/src/logs.rs` `install_crash_hook` `mark_crashes_seen` | `crash-<time>-<pid>.txt` when a panic happens; renamed `*.seen.txt` once the user has seen it | `cli_tests.rs::a_crash_writes_a_report_to_the_local_log_folder` `logs.rs::crash_files_are_found_and_marked_seen` |

### Developer and release tools

| Code | What it writes | Tests |
|---|---|---|
| `crates/keepsake-cli/src/commands/corpus.rs` `run_scan` `run_init_expected` | `corpus/manifest.json` and `corpus/expected.toml` (paths, sizes and hashes; no media) | `cli_tests.rs::corpus_scan_and_init_expected_write_only_their_own_files` |
| `crates/keepsake-cli/src/commands/renders.rs` `bench` | copies a sample of media into a temporary library (the source is only read) | `cli_tests.rs::renders_bench_reports_items_per_minute` |
| `app/ui/src-tauri/src/lib.rs` `self_check` | `keepsake-app --self-check <file>` writes its JSON result where asked (the release smoke test) | `lib.rs::self_check_writes_its_report_where_asked` |

## Outside Keepsake's code

- **WebView2** (Windows) keeps its own browser data (cache, local storage) in `%LOCALAPPDATA%\dev.keepsake.app\EBWebView`. **WKWebView** (macOS) uses the system's equivalent.
- **The operating system's file dialogs** (Browse…) only return a path; they write nothing.
- **Open in** hands a file to another application, which may do anything it likes with it. Keepsake shows the file it is handing over and does not watch or change it.
