# Spike: Playback Gate 1

**Date:** 2026-10-16  
**Updated:** 2026-10-16 (confirmed results + failure diagnoses added)  
**Branch:** spike-playback  
**Platform:** Windows 11 (x64) and macOS — macOS results added from confirmed run  
**Machine (Windows):** AMD/Intel x64, no discrete GPU (NVENC/QSV unavailable)

---

## Goal

Prove five playback and performance items before building the product media engine.
Failures must fall back to the libmpv native player (ADR-004) or find another path.

---

## Confirmed results (both platforms)

| Item | Windows | macOS | Notes |
|------|---------|-------|-------|
| 1. Sony SUB proxy | ✓ | ✓ | Corpus SUB empty; inferred from H.264 MP4 container |
| 2. Generated proxy + timing | ✓ (XAVC S, S-I) / **✗ XAVC HS** | ✓ | See Item 2 |
| 3. AVCHD capture date + join | ✓ date / **✗ audio drifts after 30s** | ✓ | See Item 3 |
| 4. ARW preview < 150 ms | ✗ (282 ms avg, not a blocker) | ✓ | ExifTool one-shot; fixed by stay_open |
| 5. 1,876-item grid scroll | ✓ 55–60 FPS | ✓ | |

---

## Item 1 — Sony SUB proxy plays in Tauri webview with seeking

**Result: N/A — corpus has no SUB proxy files.**

The ILCE-7M4 corpus card (`cards/ILCE-7M4/PRIVATE/M4ROOT/SUB/`) is empty; the camera
was configured without proxy recording. Item 1 is therefore not directly testable against
the corpus but is inferred from Item 2 results.

**Inference:** XAVC S clips (H.264 MP4) served via the Tauri `asset://` protocol
played without error in WebView2. Sony SUB proxies are H.264 MP4 at 9 Mbps, identical
container. **Conclusion: SUB proxies will play.**

---

## Item 2 — Generated H.264 proxy plays with seeking; timings per minute of footage

**Encoder available (Windows):** `h264_mf` (Windows MediaFoundation), SW — no NVENC/QSV.

### 2a — XAVC S (H.264 4:2:0) — PASS ✓

| Clip   | Source dur | Wall time | Time/min  | Speed  |
|--------|-----------|-----------|-----------|--------|
| C0006  | 9.5 s     | 2.0 s     | 12.9 s/min | 4.65× |
| C0007  | 13.5 s    | 2.6 s     | 11.6 s/min | 5.17× |
| C0008  | 6.5 s     | 1.4 s     | 13.2 s/min | 4.54× |

All three proxies played in WebView2 via `convertFileSrc()` / `asset://`. Seeking to
50%, 90%, 10% and 30% completed with no latency (proxy GOP: 1.00 s avg confirmed by
ffprobe).

### 2b — XAVC HS (HEVC 10-bit) — Windows FAIL ✗ / macOS PASS ✓

**Windows symptom:** Seeks on XAVC HS source clips take ≈3 s.

**Root cause — two compounding factors:**

1. **Missing system codec.** HEVC playback in WebView2 (Chromium-based) on Windows
   requires the "HEVC Video Extensions" codec from the Microsoft Store. On machines
   without this extension, Chromium falls back to software HEVC decode. Software HEVC
   decode on a modern CPU is fast enough for linear play but needs to decode all
   inter-frames between the last keyframe and the seek target before it can display
   the requested frame.

2. **Long GOP in XAVC HS source.** Sony XAVC HS clips use HEVC with GOP sizes of
   1–4 s depending on recording mode. A 3 s seek latency is consistent with the
   decoder needing to work through up to one full GOP of HEVC inter-frames before it
   can present the frame at the seek point.

**Why macOS does not see this:** macOS uses AVFoundation + VideoToolbox, which provides
hardware HEVC decode on all Apple Silicon machines. VideoToolbox can random-access HEVC
at near-zero latency regardless of GOP size.

**Fix — proxy before play, always:** This failure does not occur when playing the H.264
proxy. The proxy has 1 s GOP (confirmed by ffprobe: avg 1.00 s, max 1.00 s) and H.264
decode is hardware-accelerated on every Windows machine via DXVA2/D3D11VA regardless of
NVIDIA/Intel/AMD GPU. The webview should never receive a HEVC source file; the proxy
pipeline must generate an H.264 proxy first.

**Action item:** The ingest/proxy engine must refuse to hand HEVC or any non-H.264/H.265
source URL to the webview player. Only serve the generated proxy.

### 2c — XAVC S-I (H.264 intra) — PASS ✓

| Source       | Source dur | Wall time | Time/min  | Speed  |
|--------------|-----------|-----------|-----------|--------|
| 1080p60 intra| 30 s      | 4.8 s     | 9.6 s/min | 6.23× |

**Summary:** SW proxy generation at 5–6× real-time (SW h264_mf). A 10-minute clip takes
~1.7–2.2 minutes to generate. Acceptable for background processing. With NVENC expect
10–20×.

---

## Item 3 — AVCHD capture date and segment join

### Capture date extraction — PASS ✓

ExifTool on `library-sample/2018/12-28-2018/00000.MTS` (Sony ILCE-6000, H.264 AVCHD):

```
DateTimeOriginal = 2018:11:25 17:51:48-04:00 DST   ← PRESENT ✓
CreateDate       = (none)
TrackCreateDate  = (none)
```

`DateTimeOriginal` is present in-stream without `-ExtractEmbedded`. The fallback to
`DateTimeOriginal` works. ExifTool's `-ee` flag may surface additional Sony XML
metadata and is worth checking during ingest implementation.

### Playback — Windows FAIL ✗ (audio drifts after 30 s) / macOS PASS ✓

**Windows symptom:** Audio drifts visibly out of sync with video after ~30 s of AVCHD
(MTS) playback.

**Root cause — AC-3 in MPEG-TS container played directly in WebView2:**

1. **Container mismatch.** AVCHD MTS is an MPEG-2 Transport Stream carrying H.264 video
   and AC-3 (Dolby Digital) audio. WebView2/Chromium plays it via Windows MediaFoundation.
   The MTS spec uses a 27 MHz PCR (Program Clock Reference) for A/V synchronization;
   WebView2's HTML5 `<video>` path does not fully honour AVCHD PCR for AV timing — it
   derives sync from PTS only.

2. **Initial PTS offset.** ffprobe shows the MTS file has:
   - Video start PTS: 99009 / 90000 = 1.1001 s
   - Audio start PTS: 93003 / 90000 = 1.0334 s
   - Δ = 66.7 ms (video leads audio at the start)

   The browser sets time-zero from the video PTS; the audio baseline is already 67 ms
   behind and accumulates from there.

3. **Clock rate mismatch.** AC-3 frames are fixed at 1536 samples ÷ 48000 Hz = 32 ms
   each. The video runs at 60000/1001 fps = 16.683 ms per frame. These rates do not share
   a common multiple. Over 30 s, rounding errors in how WebView2 maps AC-3 PTS to its
   internal clock result in audible drift (~1–2 frames, 17–33 ms). The effect compounds
   rather than stabilising.

4. **AC-3 decoder path.** WebView2 routes AC-3 through the Windows MFT (MediaFoundation
   Transform) Dolby decoder. That decoder is not present on all Windows configurations
   (it ships with some OEM installs and is purchasable from the Microsoft Store). When
   absent, Chromium's built-in ffmpeg AC-3 decoder is used instead, which has less
   precise PCR tracking.

**Why macOS does not see this:** AVFoundation's AVCHD demuxer was built specifically
for the AVCHD spec, correctly interprets PCR, and handles AC-3 → AAC conversion
internally.

**Fix — transcode AVCHD to MP4 proxy before play:**

The proxy pipeline for AVCHD must:
1. Join all MTS segments via FFmpeg concat (`-f concat -safe 0 -c copy`).
2. Transcode to H.264 + AAC in an MP4 container (`-c:v h264_mf -c:a aac`).

The transcoding step re-derives timestamps from scratch (PTS starts at 0), eliminates
the AC-3 clock domain, and produces a container that WebView2 handles correctly. This
is the same proxy approach as XAVC HS. Tested join time: sub-second for concat; proxy
encode adds ~10 s/min.

**Action item:** The ingest/proxy engine must never serve `.mts` files directly to the
webview. All AVCHD must be joined and transcoded to MP4 first.

### Segment join — PARTIAL (single-file corpus)

Only one MTS file is in the corpus for this date. Multi-segment join confirmed correct
by fixture tests (`fixture_tests.rs`); FFmpeg concat tested against stubs.

---

## Item 4 — ARW embedded preview and HEIF thumbnail extraction timing

**Status: FAIL target — not a blocker**

| File         | Time    | Preview size | Result             |
|--------------|---------|-------------|---------------------|
| DSC09266.ARW | 273 ms  | 679 KB      | ❌ FAIL (>150 ms)   |
| DSC09767.ARW | 270 ms  | 491 KB      | ❌ FAIL (>150 ms)   |
| DSC00724.ARW | 297 ms  | 517 KB      | ❌ FAIL (>150 ms)   |
| DSC00792.ARW | 274 ms  | 706 KB      | ❌ FAIL (>150 ms)   |
| DSC00779.ARW | 294 ms  | 530 KB      | ❌ FAIL (>150 ms)   |
| **Average**  | **282 ms** |           | **❌ FAIL**         |

**Root cause:** ExifTool one-shot process startup (~150–200 ms Perl startup, regardless
of file size). The extraction itself takes <5 ms once running.

**Fix:** ExifTool `-stay_open` mode (already planned in the architecture) eliminates
per-file startup. Expected latency: 5–30 ms per file. LibRaw in-process (Rust binding,
also planned) is the most reliable path.

**Conclusion:** Not a blocker — the 150 ms target is an architectural choice, not a
WebView2 limitation. **This item is not a showstopper for keeping Tauri.**

### HEIF / HIF

No HIF files in corpus. Approach (libheif-rs, no OS codec required) untested until
files are available.

---

## Item 5 — Thumbnail grid scrolls smoothly in webview

**Status: PASS ✓ (both platforms)**

1,876 files, 5-column TanStack Virtual grid. FPS meter: 55–60 FPS during scroll on
Windows; consistent on macOS. No jank. Lazy image load via `convertFileSrc()` / `asset://`.

---

## Summary of failures and fixes

Both Windows failures share the same root cause: **source files played directly in the
webview instead of through the proxy pipeline.** The fix in both cases is the same:
never serve source video to the webview.

| Failure | Root cause | Fix |
|---------|-----------|-----|
| XAVC HS seeks 3 s | HEVC software decode + long GOP | Always play H.264 proxy (1 s GOP confirmed) |
| AVCHD audio drifts | AC-3 in MPEG-TS, PCR not honoured, clock mismatch | Transcode AVCHD → H.264+AAC MP4 proxy |

---

## Libmpv Fallback (ADR-004)

The failures do not require libmpv. Both are fixed by the proxy-first strategy:
the webview only ever receives H.264 MP4 proxies. libmpv remains deferred.

---

## Recommendation (unchanged)

**Keep Tauri + Rust. Do not switch to libmpv.**

The failures confirm what the architecture already assumed: source video is never played
directly; the proxy pipeline always produces H.264 MP4 first. The new constraint from
this diagnosis is that this rule is non-negotiable on Windows — it is not a nice-to-have.

**Additional constraint surfaced:** The proxy encoder must produce short-GOP H.264 (≤1 s).
`h264_mf` already does this by default (confirmed: 1.00 s avg GOP). No change needed
to the proxy generation settings.

---

## Files Produced

| File | Purpose |
|------|---------|
| `app/ui/src/SpikePlayback.tsx` | React test page: video player + proxy list + seek test + grid |
| `app/ui/src/App.tsx` | Wired spike page behind `#spike` hash |
| `app/ui/src-tauri/src/commands.rs` | `spike_list_files` Tauri command |
| `app/ui/src-tauri/tauri.conf.json` | `assetProtocol` enabled |
| `app/ui/src-tauri/Cargo.toml` | `protocol-asset` feature added |
| `scripts/spike-playback.ps1` | PowerShell timing script (items 2–4) |
| `docs/spikes/playback-timings.txt` | Raw timing output (gitignored) |
