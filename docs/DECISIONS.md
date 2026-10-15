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

**Status**: Accepted

**Context**: FFmpeg provides the best cross-platform support for Sony video formats. Linking FFmpeg statically risks pulling in GPL components. Dynamic linking against a system FFmpeg introduces version fragility.

**Decision**: Ship pinned LGPL builds of `ffmpeg` and `ffprobe` as sidecar binaries in `vendor/` (excluded from the repo; downloaded by `scripts/setup.sh` / `scripts/setup.ps1`). The app locates them by path and invokes them as child processes. No FFmpeg code is linked into the Keepsake binary.

**Consequences**:
- Licence is LGPL-2.1 (the LGPL build excludes GPL encoders such as x264 in some configurations). We use libx264 through a process boundary, which is explicitly permitted by LGPL. Build provenance is documented per platform.
- Binary size: ~30–60 MB per platform for the sidecar pair.
- `vendor/` is gitignored; CI downloads the sidecars during setup.

**FFmpeg build source**:
- Windows: gyan.dev `ffmpeg-release-essentials` (LGPL).
- macOS: evermeet.cx static builds (LGPL).

---

## ADR-006 — Sony Camera Remote SDK as optional plug-in

**Status**: Accepted

**Context**: Sony's Camera Remote SDK enables live-view tethering and remote shutter for supported camera bodies. However, it requires accepting Sony's SDK licence and is not open-source.

**Decision**: Tethering is implemented as an optional plug-in crate (`keepsake-plugin-sony-crs`) that is compiled only when the SDK headers are present and the user opts in. The plug-in communicates with `keepsake-core` through a `TetherAdapter` trait. The core product ships and is fully tested without the plug-in.

**Consequences**:
- No SDK licence burden on users who do not need tethering.
- Sony SDK is not redistributed in the repo.
- Plug-in is not part of the CI matrix; tested separately on hardware.
