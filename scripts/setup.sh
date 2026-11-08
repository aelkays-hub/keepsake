#!/usr/bin/env bash
# Keepsake dev-environment setup — macOS / Linux
# Re-running is safe; every step is idempotent.
#
# NOTE: macOS FFmpeg support is currently deferred (Windows-first development).
# A future prompt will add a CI workflow that compiles an LGPL FFmpeg/ffprobe
# for Apple Silicon (VideoToolbox enabled, no GPL components).
#
# --with-ai: also download the AI models (372 MB, and 39 MB for faces) into vendor/models (ADR-035, ADR-040).
set -euo pipefail

WITH_AI=0
for arg in "$@"; do
    [[ "$arg" == "--with-ai" ]] && WITH_AI=1
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$REPO_ROOT/vendor"
mkdir -p "$VENDOR"

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[setup]${NC} $*"; }
warn()  { echo -e "${YELLOW}[setup]${NC} $*"; }
error() { echo -e "${RED}[setup]${NC} $*" >&2; exit 1; }

# ── Mirror base ───────────────────────────────────────────────────────────────
# All vendor binaries are mirrored as GitHub Release assets to guard against
# upstream URL rot.
MIRROR_BASE="https://github.com/aelkays-hub/keepsake/releases/download/vendor-binaries-v1"

# download_mirrored <filename> <dest_path> <expected_sha256>
download_mirrored() {
    local filename="$1" dest="$2" expected_sha256="$3"
    local url="$MIRROR_BASE/$filename"
    info "Downloading $filename ..."
    if ! curl -L --retry 3 --max-redirs 10 --fail -o "$dest" "$url"; then
        error "Download failed: $filename\n  URL: $url"
    fi
    local size
    size=$(wc -c < "$dest")
    if [[ "$size" -lt 1024 ]]; then
        rm -f "$dest"
        error "Downloaded file is suspiciously small ($size bytes): $filename\n  URL: $url"
    fi
    info "Verifying SHA-256 for $filename ..."
    local actual
    if command -v sha256sum &>/dev/null; then
        actual=$(sha256sum "$dest" | awk '{print $1}')
    else
        actual=$(shasum -a 256 "$dest" | awk '{print $1}')
    fi
    if [[ "$actual" != "$expected_sha256" ]]; then
        rm -f "$dest"
        error "SHA-256 mismatch: $filename\n  URL:      $url\n  expected: $expected_sha256\n  got:      $actual"
    fi
    info "  ✓ SHA-256 OK"
}

# ── 1. Rust stable ───────────────────────────────────────────────────────────
info "Checking Rust toolchain..."
if ! command -v rustup &>/dev/null; then
    info "Installing rustup..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1091
    source "$HOME/.cargo/env"
else
    rustup toolchain install stable --no-self-update
    rustup default stable
fi
RUST_VERSION=$(rustc --version)
info "  ✓ $RUST_VERSION"

# ── 2. Node LTS via nvm ──────────────────────────────────────────────────────
info "Checking Node.js..."
NVM_DIR="${NVM_DIR:-$HOME/.nvm}"
if [[ ! -f "$NVM_DIR/nvm.sh" ]]; then
    info "Installing nvm..."
    curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.1/install.sh | bash
fi
# shellcheck disable=SC1091
source "$NVM_DIR/nvm.sh"
nvm install --lts
nvm use --lts
NODE_VERSION=$(node --version)
info "  ✓ Node $NODE_VERSION"

# ── 3. pnpm ──────────────────────────────────────────────────────────────────
info "Checking pnpm..."
if ! command -v pnpm &>/dev/null; then
    npm install -g pnpm
fi
PNPM_VERSION=$(pnpm --version)
info "  ✓ pnpm $PNPM_VERSION"

# verify_ffmpeg_licence <ffmpeg_binary>
# Fails if the build is GPL or nonfree; prints whether it is LGPLv2.1 or LGPLv3.
# --enable-version3 means LGPLv3, which is acceptable under ADR-005.
verify_ffmpeg_licence() {
    local bin="$1"
    local ver
    ver=$("$bin" -version 2>&1 | tr '\n' ' ')
    if echo "$ver" | grep -q -- "--enable-gpl"; then
        error "Licence check failed: ffmpeg reports --enable-gpl.\n  This is a GPL build — replace it with an LGPL build.\n  Binary: $bin"
    fi
    if echo "$ver" | grep -q -- "--enable-nonfree"; then
        error "Licence check failed: ffmpeg reports --enable-nonfree.\n  Binary: $bin"
    fi
    local label="LGPLv2.1"
    echo "$ver" | grep -q -- "--enable-version3" && label="LGPLv3"
    info "  ✓ Licence verified: $label (no --enable-gpl, no --enable-nonfree)"
}

# ── 4. FFmpeg / ffprobe (macOS — DEFERRED) ───────────────────────────────────
# macOS FFmpeg support is deferred.  No maintained LGPL prebuilt exists for
# Apple Silicon, so a custom compile is required (VideoToolbox, no GPL).
# A CI workflow and setup steps will be added in a future prompt when macOS
# development resumes.  When FFmpeg is added, call verify_ffmpeg_licence after
# installation.
if [[ "$(uname)" == "Darwin" ]]; then
    warn "FFmpeg/ffprobe for macOS is not yet configured."
    warn "  Development is currently Windows-first.  macOS support is planned."
    warn "  Until then, video features that require FFmpeg will not be available on macOS."
fi

# ── 5. ExifTool (Unix tarball, SourceForge mirror) ───────────────────────────
# Source: https://exiftool.org (now hosted on SourceForge).
EXIFTOOL_VERSION="13.59"
EXIFTOOL_TGZ_NAME="Image-ExifTool-13.59.tar.gz"
EXIFTOOL_TGZ_SHA="668ea3acececb7235fbd0f4900e72d5f12c9b07e5c778fd36cb1e9b5828fd65a"
EXIFTOOL_DIR="$VENDOR/exiftool"

if [[ ! -d "$EXIFTOOL_DIR" ]]; then
    EXIFTOOL_TGZ="$VENDOR/$EXIFTOOL_TGZ_NAME"
    download_mirrored "$EXIFTOOL_TGZ_NAME" "$EXIFTOOL_TGZ" "$EXIFTOOL_TGZ_SHA"

    info "Extracting ExifTool..."
    mkdir -p "$EXIFTOOL_DIR"
    tar -xzf "$EXIFTOOL_TGZ" -C "$EXIFTOOL_DIR" --strip-components=1
    rm "$EXIFTOOL_TGZ"
    chmod +x "$EXIFTOOL_DIR/exiftool"
    info "  Installed $EXIFTOOL_DIR/exiftool"
else
    info "  ExifTool already present, skipping download."
fi

# Smoke-test ExifTool.
ET_VER=$("$EXIFTOOL_DIR/exiftool" -ver 2>&1 | head -1)
if [[ ! "$ET_VER" =~ ^[0-9] ]]; then
    error "ExifTool smoke-test failed (unexpected output: $ET_VER)"
fi
info "  ✓ ExifTool $ET_VER"

# ── 6. LibRaw dcraw_emu (RAW decoding sidecar, ADR-023) ──────────────────────
# Official LibRaw 0.22.2 macOS build (universal, Apple Silicon + Intel),
# mirrored unchanged. LGPL-2.1 or CDDL-1.0; run as a process, never linked.
LIBRAW_ZIP_NAME="LibRaw-0.22.2-macOS.zip"
LIBRAW_ZIP_SHA="46e9cec8798419775df1411cc4554bb023e003509ff44b4057fd3b84eb55a517"
LIBRAW_DIR="$VENDOR/libraw"

if [[ ! -x "$LIBRAW_DIR/dcraw_emu" ]]; then
    LIBRAW_ZIP="$VENDOR/$LIBRAW_ZIP_NAME"
    download_mirrored "$LIBRAW_ZIP_NAME" "$LIBRAW_ZIP" "$LIBRAW_ZIP_SHA"
    TMPX="$VENDOR/libraw-extract"
    rm -rf "$TMPX" && mkdir -p "$TMPX" "$LIBRAW_DIR"
    unzip -q "$LIBRAW_ZIP" -d "$TMPX"
    ROOT="$TMPX/LibRaw-0.22.2"
    for f in bin/dcraw_emu LICENSE.LGPL LICENSE.CDDL COPYRIGHT; do
        [[ -f "$ROOT/$f" ]] || error "$f not found in $LIBRAW_ZIP_NAME"
        cp "$ROOT/$f" "$LIBRAW_DIR/"
    done
    chmod +x "$LIBRAW_DIR/dcraw_emu"
    # Downloaded binaries are quarantined by Gatekeeper; this one is mirrored
    # from the official release and checksum-verified above.
    xattr -d com.apple.quarantine "$LIBRAW_DIR/dcraw_emu" 2>/dev/null || true
    rm -rf "$TMPX" "$LIBRAW_ZIP"
    info "  Installed $LIBRAW_DIR/dcraw_emu"
else
    info "  LibRaw already present, skipping download."
fi
# dcraw_emu exits non-zero after printing its usage: check the text only.
LIBRAW_OUT="$("$LIBRAW_DIR/dcraw_emu" 2>&1 || true)"
if ! grep -q "dcraw emulator" <<<"$LIBRAW_OUT"; then
    error "dcraw_emu smoke-test failed ($LIBRAW_DIR/dcraw_emu)"
fi
info "  ✓ LibRaw dcraw_emu"

# download_pinned <url> <dest_path> <expected_sha256>  (not on the mirror yet)
download_pinned() {
    local url="$1" dest="$2" expected_sha256="$3" actual
    info "Downloading $url ..."
    curl -L --retry 3 --max-redirs 10 --fail -o "$dest" "$url" || error "Download failed: $url"
    if command -v sha256sum &>/dev/null; then
        actual=$(sha256sum "$dest" | awk '{print $1}')
    else
        actual=$(shasum -a 256 "$dest" | awk '{print $1}')
    fi
    if [[ "$actual" != "$expected_sha256" ]]; then
        rm -f "$dest"
        error "SHA-256 mismatch: $url\n  expected: $expected_sha256\n  got:      $actual"
    fi
    info "  ✓ SHA-256 OK"
}

# ── 7. ONNX Runtime (on-device AI, ADR-035) ──────────────────────────────────
# Microsoft's official 1.31.1 build for Apple Silicon (MIT, includes CoreML),
# pinned by SHA-256. Loaded by Keepsake at run time, never linked.
# Install layout: vendor/onnxruntime/libonnxruntime.dylib (+ licences)
ORT_DIR="$VENDOR/onnxruntime"
if [[ "$(uname)" == "Darwin" && "$(uname -m)" == "arm64" ]]; then
    if [[ ! -f "$ORT_DIR/libonnxruntime.dylib" ]]; then
        ORT_TGZ="$VENDOR/onnxruntime-osx-arm64-1.31.1.tgz"
        download_pinned \
            "https://github.com/microsoft/onnxruntime/releases/download/v1.31.1/onnxruntime-osx-arm64-1.31.1.tgz" \
            "$ORT_TGZ" "2e1344a1926ee50eb25b38153cb6992c3567515aef0e677a1a520a1798896d6f"
        TMPX="$VENDOR/ort-extract"
        rm -rf "$TMPX" && mkdir -p "$TMPX" "$ORT_DIR"
        tar -xzf "$ORT_TGZ" -C "$TMPX" --strip-components=1
        cp -L "$TMPX/lib/libonnxruntime.dylib" "$ORT_DIR/libonnxruntime.dylib"
        cp "$TMPX/LICENSE" "$ORT_DIR/LICENSE-onnxruntime.txt"
        cp "$TMPX/ThirdPartyNotices.txt" "$ORT_DIR/ThirdPartyNotices.txt"
        xattr -d com.apple.quarantine "$ORT_DIR/libonnxruntime.dylib" 2>/dev/null || true
        rm -rf "$TMPX" "$ORT_TGZ"
        info "  Installed $ORT_DIR/libonnxruntime.dylib"
    else
        info "  ONNX Runtime already present, skipping download."
    fi
else
    warn "ONNX Runtime is set up for Apple Silicon only; AI features are off here."
fi

# ── 8. AI model (optional, --with-ai) ────────────────────────────────────────
# SigLIP 2 base image encoder (Apache-2.0), onnx-community revision ba1f3b08.
# The app downloads it into the user's cache when AI is turned on; this copy
# is for developers and the model tests.
if [[ "$WITH_AI" == 1 ]]; then
    MODEL_DIR="$VENDOR/models/siglip2-b16-224"
    MODEL_FILE="$MODEL_DIR/vision_model.onnx"
    MODEL_SHA="c0573e3f4140c3a7c4e9cc5912bd6b26a033b46a6a8e8af26cbea262b163bcad"
    if [[ ! -f "$MODEL_FILE" ]]; then
        mkdir -p "$MODEL_DIR"
        download_pinned \
            "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/ba1f3b0843f24bc5417d38e19c37b287d719b2f4/onnx/vision_model.onnx" \
            "$MODEL_FILE" "$MODEL_SHA"
    else
        info "  AI model already present, skipping download."
    fi
    # Face models (ADR-040): YuNet (MIT) and SFace (Apache-2.0), OpenCV Zoo
    # on Hugging Face at pinned revisions.
    for spec in         "yunet-2023mar|face_detection_yunet_2023mar.onnx|https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx|8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4"         "sface-2021dec|face_recognition_sface_2021dec.onnx|https://huggingface.co/opencv/face_recognition_sface/resolve/3d7082438a6e4551e840c9b2bb60b71e8da4b524/face_recognition_sface_2021dec.onnx|0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79"; do
        IFS='|' read -r dir file url sha <<< "$spec"
        if [[ ! -f "$VENDOR/models/$dir/$file" ]]; then
            mkdir -p "$VENDOR/models/$dir"
            download_pinned "$url" "$VENDOR/models/$dir/$file" "$sha"
        else
            info "  $file already present, skipping download."
        fi
    done
fi

# ── Done ─────────────────────────────────────────────────────────────────────
info ""
info "Dev environment ready. Next steps:"
info "  cd app/ui && pnpm install"
info "  cargo build --workspace"
info "  pnpm tauri dev"
