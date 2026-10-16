# TEST_ME — Spike 2: Playback Gate 1

## Prerequisites

- Windows 11 (macOS deferred)
- `vendor/ffmpeg.exe`, `vendor/ffprobe.exe`, `vendor/exiftool/exiftool.exe` present
- `KEEPSAKE_CORPUS` set to your corpus root (e.g. `C:\...\keepsake-corpus`)
- pnpm installed; run `pnpm install` once in `app/ui`

---

## Manual test A — Spike page loads

1. `cd app/ui && pnpm tauri dev`
2. Wait for the window to open.
3. Click the **spike →** link in the bottom of the placeholder screen (or press F12 in DevTools and run `window.location.hash = '#spike'`).
4. **Expected:** The spike page loads with "Spike: Playback Gate 1" as heading, a video player section and a thumbnail grid section.

---

## Manual test B — Video playback and seeking

1. On the spike page, the video player should be pre-filled with the path to `C0006.MP4` from your corpus.
2. **Expected:** The video plays without error. The event log below the player shows `canplay ✓`.
3. Click **Run seek test**.
4. **Expected:** Log shows four seek events (`Seek to Xs → Yms`) with no `error:` entries.
5. Paste a path to a generated proxy (`%TEMP%\keepsake-spike-proxies\C0006-proxy.mp4`) into the path field and click **Load**.
6. **Expected:** Proxy also plays and seeks without error.

---

## Manual test C — Thumbnail grid scroll

1. On the spike page, scroll down to the **Item 5** section.
2. **Expected:** Status line shows "1,876 files loaded" (or similar).
3. Click **Start FPS meter**, then scroll the grid quickly up and down for 5 seconds, then click **Stop**.
4. **Expected:** FPS meter shows ≥ 50 FPS throughout. No visible jank.

---

## Manual test D — Backend timing script

```powershell
$env:KEEPSAKE_CORPUS = "C:\path\to\keepsake-corpus"
cd C:\path\to\keepsake
pwsh -File scripts\spike-playback.ps1
```

**Expected output (Windows, no discrete GPU):**

```
--- Item 2a: XAVC S proxy generation ---
XAVC-S C0006  encoder=h264_mf  clip=~9s  wall=~2s   ~13s/min  ~4.5x
XAVC-S C0007  encoder=h264_mf  clip=~13s wall=~2.6s ~12s/min  ~5x
XAVC-S C0008  encoder=h264_mf  clip=~6s  wall=~1.4s ~14s/min  ~4.5x

--- Item 2b: Synthetic 4K source ... ---
Synthetic-4K  encoder=h264_mf  clip=30s  wall=~5s  ~10s/min  ~5.5x

--- Item 2c: Synthetic H.264-intra source ... ---
XAVC-S-I-intra  encoder=h264_mf  clip=30s  wall=~5s  ~10s/min  ~6x

--- Item 3: AVCHD capture date ---
DateTimeOriginal=2018:11:25 ...   (date present)

--- Item 4: ARW embedded preview ---
DSC*.ARW  ~270-300ms  preview=~500-700KB  FAIL (>150ms)
ARW preview avg: ~280ms  (target <= 150ms)
```

> Item 4 FAIL is expected — process-startup overhead. Will be resolved by
> ExifTool stay-open mode in the ingest engine.

**Expected:** Script exits 0. Results appended to `docs/spikes/playback-timings.txt`.

---

## Manual test E — GPU encoder (if NVIDIA GPU present)

Run the timing script on a machine with an NVIDIA GPU.

**Expected:** `encoder=h264_nvenc` in item 2a/2b/2c output. Speed ≥ 10× real-time.

---

## Notes

- `docs/spikes/playback-timings.txt` is gitignored (raw data); read `docs/spikes/playback.md` for interpreted results.
- The spike page is not product UI — it lives behind `#spike` and will be removed when the real media engine is built.
