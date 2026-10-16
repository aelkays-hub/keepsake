# Architecture Decision Records

---

## ADR-001 — Tauri 2 + Rust core

**Status**: Accepted

**Context**: We need a cross-platform desktop app (Windows 11 x64/ARM64, macOS 14+ Apple Silicon) with a native feel, access to low-level filesystem and device APIs, and a modern web UI. Electron was considered.

**Decision**: Use Tauri 2 with a Rust backend (`keepsake-core`) and a React/TypeScript frontend. Rust handles all I/O-intensive and CPU-intensive work; the webview renders the UI only.

**Consequences**:
- Binary is small (no bundled Chromium engine).
- Full access to Rust's async ecosystem (`tokio`) for concurrent ingest and rendering.
- Tauri commands must remain thin; business logic stays in `keepsake-core` and is testable without a GUI.
- ARM64 Windows requires Tauri 2 (Tauri 1 had partial support); confirmed supported.

**Licence**: Tauri 2 is MIT/Apache-2.0. ✓

---

## ADR-002 — Card-first ingest; native OS adapters instead of libgphoto2

**Status**: Accepted

**Context**: Most Sony cameras expose their card as a standard filesystem over USB or via a card reader. libgphoto2 provides a cross-platform MTP/PTP abstraction but introduces a large C dependency and GPL-adjacent complexity.

**Decision**: Primary ingest path reads the card filesystem directly. OS-native device-arrival events (`WM_DEVICECHANGE` on Windows, IOKit on macOS) detect the card. MTP/PTP (for cameras that do not expose a filesystem) is deferred to v1 and implemented with OS-native APIs (`WPD` on Windows, `IOUSBHost` on macOS) wrapped behind a `MtpTransport` trait in `keepsake-platform`.

**Consequences**:
- No libgphoto2 dependency; no GPL risk.
- Card filesystem reads are faster and more reliable than MTP for bulk transfer.
- MTP path is more work but keeps licencing clean.

---

## ADR-003 — SQLite catalog + XMP sidecars as dual source of truth

**Status**: Accepted

**Context**: The library must be browsable offline and must survive catalog corruption without data loss.

**Decision**: SQLite (WAL mode, FTS5) is the **query cache** for fast search, browsing, and virtual list rendering. XMP sidecars and JSON edit recipes are the **durable source of truth** for all user-authored metadata. Deleting the catalog and running `rebuild-catalog` must restore all user data.

**Consequences**:
- Library is interoperable: other tools that read XMP (Lightroom, darktable, digikam) see Keepsake's ratings, labels, and comments.
- Rebuild is a defined, tested operation.
- Every write of user metadata must update both the catalog row and the XMP sidecar.

**Licence**: `rusqlite` (bundled SQLite) is MIT. SQLite itself is public domain. ✓

---

## ADR-004 — Playback via camera proxies or app-generated H.264 proxies

**Status**: Accepted

**Context**: Modern Sony cameras record XAVC HS in 10-bit 4:2:2 or 4:2:0 HEVC, and older models record AVCHD. Neither codec is decodable by the Chromium webview embedded in Tauri without hardware decoder integration that is out of scope.

**Decision**: Video playback uses the camera-generated proxy file (`PRIVATE/M4ROOT/SUB/*.MP4`) when available. If no proxy exists (e.g. AVCHD or older MP4 cards), the RenderService generates an H.264 1080p proxy via FFmpeg and caches it. The webview plays the proxy via `<video>` with a `blob:` URL.

**Consequences**:
- Playback works reliably across all supported Sony formats.
- Proxy generation takes time; progress is shown in the UI.
- Storage cost for proxies: ~1–3 GB per hour of 4K footage at proxy quality.
- Original files are never read by the webview.

---

## ADR-005 — FFmpeg as LGPL sidecar process

**Status**: Accepted (updated 2026-10-15)

**Context**: FFmpeg provides the best cross-platform support for Sony video formats. Linking FFmpeg statically risks pulling in GPL components. Dynamic linking against a system FFmpeg introduces version fragility. Early versions of the setup script used gyan.dev (Windows) and evermeet.cx (macOS), both of which have URL instability issues; gyan.dev also distributes GPL builds alongside LGPL ones.

**Decision**: Ship pinned LGPL builds of `ffmpeg` and `ffprobe` as sidecar binaries in `vendor/` (excluded from the repo; downloaded by `scripts/setup.sh` / `scripts/setup.ps1`). The app locates them by path and invokes them as child processes. No FFmpeg code is linked into the Keepsake binary.

Every pinned binary (FFmpeg, ExifTool) is mirrored as a GitHub Release asset at `aelkays-hub/keepsake` tag `vendor-binaries-v1` so setup scripts are insulated from upstream URL rot. Setup scripts download exclusively from this mirror and verify SHA-256 before use. If a checksum fails the download is deleted and the script exits with a message naming the file and the mirror URL.

After installation, `setup.ps1` (and `setup.sh` once macOS FFmpeg is added) runs `ffmpeg -version` and checks the configuration string:
- **FAIL** if `--enable-gpl` or `--enable-nonfree` is present — these indicate a GPL or proprietary build.
- **PASS** otherwise; the script prints `LGPLv2.1` or `LGPLv3` depending on whether `--enable-version3` is present.

Both LGPLv2.1 and LGPLv3 builds are acceptable. `--enable-version3` enables code licensed under LGPLv3 (and GPLv3 when combined with GPL components, but only LGPL builds are accepted here), which is a looser copyleft than GPL and fully compatible with Keepsake's distribution model.

**Consequences**:
- Licence is LGPL-2.1 or LGPL-3.0 depending on the build. BtbN LGPL builds disable all GPL and nonfree components. Build provenance is documented per platform below.
- Binary size: ~170 MB (win64 ZIP) / ~118 MB (winarm64 ZIP) for the archive; extracted sidecar pair is larger.
- `vendor/` is gitignored; developers run `scripts/setup.ps1` once to populate it.

**FFmpeg build source**:
- **Windows x64**: BtbN/FFmpeg-Builds — `ffmpeg-n9.0-latest-win64-lgpl-9.0.zip`  
  SHA-256: `73950cc6d7548c08d435bfb9c65b60798b6cd59647e59bb41e9af3876c90ebef`  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`  
  Licence: LGPL-2.1+ (confirmed: no `--enable-gpl`, no `--enable-version3`)

- **Windows ARM64**: BtbN/FFmpeg-Builds — `ffmpeg-n9.0-latest-winarm64-lgpl-9.0.zip`  
  SHA-256: `33cb51e1529c1d658eb814c0cd557272f35a83951d7bb8009ac6bbe7854b0cc6`  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`  
  Licence: LGPL-2.1+

- **macOS (Apple Silicon)**: Deferred. No maintained LGPL prebuilt exists for Apple Silicon. A future prompt will add a CI job that compiles FFmpeg from source (VideoToolbox enabled, no GPL components) and uploads the result to the vendor-binaries mirror.

**ExifTool source** (not FFmpeg, documented here for completeness):
- **Windows 64-bit**: ExifTool 13.59 standalone — `exiftool-13.59_64.zip`  
  SHA-256: `44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec`  
  Original source: sourceforge.net/projects/exiftool  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`  
  Licence: Artistic / GPL (Perl; invoked as a sidecar process, never linked)

- **macOS / Linux**: ExifTool 13.59 Unix tarball — `Image-ExifTool-13.59.tar.gz`  
  SHA-256: `668ea3acececb7235fbd0f4900e72d5f12c9b07e5c778fd36cb1e9b5828fd65a`  
  Mirror: `github.com/aelkays-hub/keepsake/releases/tag/vendor-binaries-v1`

---

## ADR-007 — clap for CLI argument parsing

**Status**: Accepted (Prompt 1)

**Context**: The CLI needs subcommands (`corpus scan`, `corpus init-expected`) with typed arguments, help text, and version output. Hand-rolling this with `std::env::args()` is brittle.

**Decision**: Use `clap` v4 with the `derive` feature. It is the de facto standard for Rust CLIs and generates `--help` / `--version` automatically.

**Licence**: MIT OR Apache-2.0. ✓

---

## ADR-008 — toml for corpus/expected.toml serialisation

**Status**: Accepted (Prompt 1)

**Context**: `corpus/expected.toml` is a human-edited file (the product owner fills in capture timestamps, camera models, and confirmation flags). TOML is more readable and diff-friendly than JSON for hand-edited config.

**Decision**: Use the `toml` crate v0.8 for serialising and deserialising `Expected` in the CLI and corpus tests. `keepsake-core` itself remains format-agnostic; the TOML dependency lives only in `keepsake-cli` and as a dev-dependency in `keepsake-core`.

**Licence**: MIT. ✓

---

## ADR-006 — Sony Camera Remote SDK as optional plug-in

**Status**: Accepted

**Context**: Sony's Camera Remote SDK enables live-view tethering and remote shutter for supported camera bodies. However, it requires accepting Sony's SDK licence and is not open-source.

**Decision**: Tethering is implemented as an optional plug-in crate (`keepsake-plugin-sony-crs`) that is compiled only when the SDK headers are present and the user opts in. The plug-in communicates with `keepsake-core` through a `TetherAdapter` trait. The core product ships and is fully tested without the plug-in.

**Consequences**:
- No SDK licence burden on users who do not need tethering.
- Sony SDK is not redistributed in the repo.
- Plug-in is not part of the CI matrix; tested separately on hardware.
