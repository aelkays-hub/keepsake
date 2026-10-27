# Performance

Measured with `keepsake renders bench`. It copies a sample of a media folder into a temporary library and makes every rendition of each type with an empty cache, one type at a time. It prints the table below. The source folder is only read.

```
cargo build --release -p keepsake-cli
target/release/keepsake renders bench "$KEEPSAKE_CORPUS/library-sample" --per-type 40 --videos 6
```

- **Items/min** counts assets per minute. Each still item is one decode, and produces the 2560 px preview and the 1024 px and 256 px thumbnails.
- Proxies are also given in **× real time**: minutes of footage encoded per minute.
- The sample is spread across each type's size range. Files under 50 KB (thumbnails) are skipped, and so are videos over 600 MB, to keep the run short.
- Times include starting ExifTool and FFmpeg processes, so small runs read a little slow.

## Windows 11 — AMD Ryzen 7 7735HS (8 cores / 16 threads), 27 GB RAM, NVMe SSD

2026-10-24, branch `render`, release build. The sample came from `library-sample`: 24 MP JPEGs, Sony ARW (ILCE-6000 and later), PNG/TIFF, and AVCHD `.M2TS` 1080i clips. The proxy encoder detected was `h264_amf` (the AMD GPU).

**4 image workers** (the default here: half the threads, at most 4), 1 proxy worker:

| Type | Items | Time (s) | Items/min | Notes |
|---|---:|---:|---:|---|
| JPEG | 40 | 3.4 | 700 | |
| ARW (embedded preview) | 40 | 2.0 | 1194 | the largest embedded JPEG is 1616×1080, so ARW previews are 1616 px |
| PNG / TIFF | 13 | 1.2 | 672 | |
| Video poster frame | 6 | 1.2 | 295 | frame at 10 % via FFmpeg |
| Video proxy (720p H.264) | 6 | 42.7 | 8 | 6.4 min of footage, **9.0× real time** (`h264_amf`) |

**1 image worker**:

| Type | Items | Time (s) | Items/min |
|---|---:|---:|---:|
| JPEG | 40 | 8.7 | 277 |
| ARW (embedded preview) | 40 | 4.8 | 504 |
| PNG / TIFF | 13 | 2.4 | 330 |
| Video poster frame | 6 | 2.7 | 136 |

**Proxy encoders** (same 6 clips, 6.4 min of 1080i AVCHD, deinterlaced and scaled to 720p):

| Encoder | Time (s) | × real time |
|---|---:|---:|
| `h264_amf` (AMD, detected) | 42.7 | 9.0 |
| `h264_mf` (Windows Media Foundation; the spike's encoder) | 44.8 | 8.6 |
| `libopenh264` (software fallback) | 96.6 | 4.0 |

Notes:
- Stills scale with workers: about 2.5× from 1 to 4 workers. JPEG decoding dominates.
- AMF and Media Foundation are close. Decoding and deinterlacing the 1080i source limits them as much as encoding does. The spike measured `h264_mf` at 5–6× on XAVC S, without a GPU encoder.
- A library like `library-sample` (about 1,000 JPEGs, 550 ARWs, 50 clips) gets all its thumbnails and previews in about 2–3 minutes. Proxies take about one minute for every 9 minutes of footage.

## macOS — Apple Silicon

Not measured yet. It needs a Mac, and an LGPL FFmpeg for macOS for posters, HEIF and proxies (ADR-005). Image types (JPEG, ARW, PNG/TIFF) can be measured now with the command above. To add a row, run it on the Mac and paste the table here.

## HEIF

Not measured: the corpus has no HEIF files yet. HEIF decodes through the FFmpeg sidecar, so expect speeds similar to video poster frames.

## Scale: 300,000 items (prompt 12)

A synthetic library from `keepsake dev seed` (ten years, four cameras, a quarter of it on an unplugged drive, keywords and comments for half), on the same PC, 2026-10-26.

**In the release app** (`app/ui/e2e/app/perf.spec.ts`, release build, WebView2):

| Measure | Result | Limit |
|---|---:|---:|
| Start-up: process start to the first grid rows on screen | 0.98 s (1.1 s on the first launch, with a new WebView2 profile) | 3 s |
| Search, from the last keystroke to the new count on screen (includes the 80 ms typing pause) | 109–233 ms | 300 ms |
| Filter: 4 stars or more | 218 ms | 300 ms |
| Calendar: the years on screen | 137 ms | 300 ms |
| Calendar: a year's months on screen | 93 ms | 300 ms |
| Scrolling the grid for 4 s at about 15 rows a second | 0 dropped frames of 238 (slowest 17.2 ms at 60 Hz), no long tasks | 0 |

**In core** (`crates/keepsake-core/tests/scale_tests.rs`, release; the slowest of three runs):

| Operation | Time |
|---|---:|
| Open the catalog | 3 ms |
| Sidebar counts | 31 ms |
| First view (300,000 sorted) and its first 200 rows | 181 ms |
| Camera and lens lists | 200 ms |
| Searches: 13 kinds of query, each sorted by date and by name | 0–115 ms |
| Calendar levels, with and without a word filter | 2–104 ms |

**What was slow, and the fixes:**

| Problem | Before | After |
|---|---:|---:|
| Sidebar per-volume count read every file row (partial index, schema v6) | 1.9 s | 31 ms |
| Calendar with words: one search per cell for its cover (now one pass) | 1.8 s | 58 ms |
| Calendar years: every asset read (now index range counts and seeks) | 170 ms | 65 ms |

Also:
- **Shared reads:** every read shared one connection behind a lock, so opening a 300,000-item view held up the calendar and the next search. Reads now use a pool of connections.
- **Camera and lens lists:** fetched with every view (200 ms). Now fetched once.
- **Pauses:** typing pause 250 → 80 ms; view pause 120 → 0 ms.
- **Word filters when browsing:** written as `seq IN (matches)`.

In the real app, keystroke-to-results went from 418–581 ms to 109–233 ms. The calendar years went from 388 ms to 137 ms.

CI runs the core scale test on every push with the limits doubled (`KEEPSAKE_SCALE_SLACK=2`).
