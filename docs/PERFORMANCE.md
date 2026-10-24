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
