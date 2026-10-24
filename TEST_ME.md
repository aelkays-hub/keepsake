# Manual Test Steps — Prompt 9: thumbnails, previews and proxies

Test on **Windows 11 x64** (primary) and **macOS 14+**. On macOS, video items need an LGPL FFmpeg in `vendor/`, which isn't available yet, so expect only photo renditions there.

Build with `cargo build --release -p keepsake-cli` and run `scripts/setup.ps1` once. Below, `$K` is `.\target\release\keepsake.exe` (or `./target/release/keepsake`). The commands are PowerShell.

Renditions are written only to the cache folder. Your photos and videos are only read; step H checks this.

---

## A. Automated tests

1. `cargo test --workspace`
   **Expected:** all pass, including `render_tests` (18) and the `renders_…` CLI tests.
2. `$env:KEEPSAKE_CORPUS = "C:\path\to\corpus"`, then `cargo test -p keepsake-core --test render_corpus_tests -- --include-ignored corpus`
   **Expected:** passes, rendering real ARW, JPEG and AVCHD files from `library-sample`.
3. `cargo fmt --check` and `cargo clippy --workspace --all-targets -- -D warnings`
   **Expected:** no output.

---

## B. Set up

```powershell
$T = "$env:TEMP\ks9"; Remove-Item -Recurse -Force $T -ErrorAction SilentlyContinue
$env:KEEPSAKE_CONFIG_DIR = "$T\config"
& $K init "$T\lib"
(Get-Content "$T\config\config.toml") -replace '^\[render\]$', "[render]`ncache_dir = '$T\cache'" | Set-Content "$T\config\config.toml"
```

Open `$T\config\config.toml`.
**Expected:** `default_library`, then a `[render]` section with `cache_dir` (your temp folder), `max_bytes = 21474836480` (20 GiB), `proxy_workers = 1` and `pause_on_battery = true`.

Import some real media: a card, or a copy of a few folders from your library with JPEGs, ARWs and some clips (XAVC S, XAVC HS, AVCHD):

```powershell
& $K ingest E:\ --dest "$T\A" --dest "$T\B"     # add --unsafe-allow-same-disk if A and B share a disk
```

---

## C. Status and the queue

1. `& $K renders status`
   **Expected:**
   - `Renditions: 0 done, N queued (0 visible, N recent, 0 backlog)`;
   - one stills job per asset and one proxy job per clip;
   - `Cache: 0 B of 21.47 GB in …\ks9\cache`.
2. Unplug the charger (laptop) and run `& $K renders status` again.
   **Expected:** `Workers would pause now: on battery power.` Plug it back in.

---

## D. Run

1. `& $K renders run`
   **Expected:**
   - a header with the worker counts and the cache folder;
   - a progress bar;
   - at the end, `Done: N job(s), 0 failed for good.` and exit code 0.

   It takes about 2–3 minutes per 1,000 photos, plus about 1 minute per 9 minutes of footage (see `docs/PERFORMANCE.md`).
2. `& $K renders status`
   **Expected:** everything done; the cache holds about 3 files per asset plus one proxy per clip.
3. Pick an imported photo and a clip, and run `& $K renders show "$T\A\…\DSC01234.JPG"`.
   **Expected:** paths for `thumb-256`, `thumb-1024` and `preview-2560`. Open them:
   - each is upright (photos taken in portrait are portrait);
   - the long edges are 256, 1024 and 2560 px, or smaller if the original is smaller. **ARW-only photos** have 1616 px previews (the size of the JPEG embedded in the file).
4. For a clip, `renders show`.
   **Expected:**
   - a poster (the preview) from about 10 % into the clip;
   - `proxy-720` or `camera-proxy`.

   Play the proxy in a browser or media player. **Expected:**
   - it plays with sound;
   - seeking is instant;
   - it is 720 px tall (or the clip's height if smaller).

   Check with:
   ```powershell
   & vendor\ffprobe.exe -v error -select_streams v:0 -show_entries stream=codec_name,height -of csv=p=0 <proxy>
   ```
   **Expected:** `h264,720`.
5. **XAVC with a SUB proxy:** a clip whose card had `PRIVATE\M4ROOT\SUB\…S01.MP4`.
   **Expected:**
   - if that proxy is H.264, it shows as `camera-proxy`, and the run took almost no time for it;
   - if your camera records HEVC proxies (XAVC HS proxy setting), it shows as `proxy-720` instead.
6. **AVCHD:** the proxy plays with the sound in sync after a minute or more. That was the spike's AVCHD failure.

---

## E. Pauses and Ctrl-C

1. Queue more work (import another folder), start `& $K renders run`, and unplug the charger.
   **Expected:** the bar shows `paused: on battery power` and no new jobs start. Plug in: it resumes.
2. While `renders run` is working, start an import in another window (`& $K ingest …`).
   **Expected:**
   - the bar shows `paused: an import is running` within about 2 s;
   - it resumes within about 10 s of the import finishing.
3. Press Ctrl-C during a run.
   **Expected:**
   - the current job(s) finish;
   - `Stopped after the current job(s)…`, exit code 130;
   - `renders status` shows the rest still queued, none running;
   - `renders run` continues.
4. Kill the process instead (Task Manager), then `& $K renders run`.
   **Expected:** the interrupted jobs run again (nothing stays "running").

---

## F. Offline media and the cache limit

1. Unplug drive A, so only B has the files.
   **Expected:** `renders run` still works, reading from B.
2. Unplug both drives and queue work: `& $K renders enqueue --all`, then `renders run`.
   **Expected:** the jobs are **deferred** (not failed) and `renders status` shows them queued. Plug the drives back in: the next run makes them.
3. Make the limit tiny:
   ```powershell
   & $K renders evict --max-bytes 1000000
   ```
   **Expected:** `Removed N rendition(s)…`, leaving the cache at about 1 MB or less.
4. Unplug both drives, then `& $K renders evict --max-bytes 0`.
   **Expected:**
   - nothing is removed;
   - `Kept … for media that is offline (never evicted)`;
   - `renders status` shows the bytes kept for offline media.
5. Plug the drives back in and run `& $K renders enqueue "$T\A\…\DSC01234.JPG" --visible`, then `& $K renders run --queued-only`.
   **Expected:** the evicted renditions for that file are made again first.

---

## G. Benchmark (your machine)

```powershell
& $K renders bench "$env:KEEPSAKE_CORPUS\library-sample" --per-type 40 --videos 6
```
**Expected:** a Markdown table (JPEG, ARW, PNG/TIFF, video poster, video proxy). It takes about 1–2 minutes. Paste it into `docs/PERFORMANCE.md` under your machine's name, especially for the Mac.

To compare encoders, add `--encoder h264_mf` or `--encoder libopenh264`.

---

## H. Originals untouched

Before C, run:
```powershell
Get-ChildItem -Recurse -File "$T\A" | Get-FileHash | Export-Csv "$T\before.csv"
```
After F, run the same command into `after.csv`, then compare: `Compare-Object (Import-Csv "$T\before.csv") (Import-Csv "$T\after.csv") -Property Hash,Path`
**Expected:** no differences, apart from `.xmp` sidecars if you changed metadata.

---

## I. macOS

Repeat C, D (photos only) and F with `/tmp/ks9`. `renders status` shows the battery pause on a MacBook. If you place an LGPL `ffmpeg` in `vendor/`, the video steps work too: VideoToolbox is tried first, then OpenH264.
