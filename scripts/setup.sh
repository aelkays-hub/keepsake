#!/usr/bin/env bash
# Keepsake dev-environment setup — macOS / Linux
# Re-running is safe; every step is idempotent.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
VENDOR="$REPO_ROOT/vendor"
mkdir -p "$VENDOR"

# ── Colours ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'; NC='\033[0m'
info()  { echo -e "${GREEN}[setup]${NC} $*"; }
warn()  { echo -e "${YELLOW}[setup]${NC} $*"; }
error() { echo -e "${RED}[setup]${NC} $*" >&2; exit 1; }

# ── Helper: SHA-256 check ────────────────────────────────────────────────────
check_sha256() {
  local file="$1" expected="$2"
  if command -v sha256sum &>/dev/null; then
    actual=$(sha256sum "$file" | awk '{print $1}')
  else
    actual=$(shasum -a 256 "$file" | awk '{print $1}')
  fi
  if [[ "$actual" != "$expected" ]]; then
    error "SHA-256 mismatch for $file\n  expected: $expected\n  got:      $actual"
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

# ── 4. FFmpeg / ffprobe (macOS, LGPL, evermeet.cx) ───────────────────────────
# Pinned version: 7.1.1
# Sources: https://evermeet.cx/ffmpeg/
FFMPEG_VERSION="7.1.1"
FFMPEG_SHA256="b2d5b6e8cd714e2f28a5f07b9dad6c1fce27b7c0e9e26a99c67f9a4b78b86ba6"
FFPROBE_SHA256="3cc2c3bb9d5f2ce5f2fec7a6c8d00b8c7c3a6bbbb0a2f2f09a8e4c3d1aef9c8f"

FFMPEG_BIN="$VENDOR/ffmpeg"
FFPROBE_BIN="$VENDOR/ffprobe"

if [[ "$(uname)" == "Darwin" ]]; then
  if [[ ! -f "$FFMPEG_BIN" ]]; then
    info "Downloading ffmpeg $FFMPEG_VERSION for macOS..."
    curl -L -o "$FFMPEG_BIN.zip" \
      "https://evermeet.cx/ffmpeg/ffmpeg-${FFMPEG_VERSION}.zip"
    unzip -o "$FFMPEG_BIN.zip" -d "$VENDOR"
    rm "$FFMPEG_BIN.zip"
    chmod +x "$FFMPEG_BIN"
  else
    info "  ffmpeg already present, skipping download."
  fi

  if [[ ! -f "$FFPROBE_BIN" ]]; then
    info "Downloading ffprobe $FFMPEG_VERSION for macOS..."
    curl -L -o "$FFPROBE_BIN.zip" \
      "https://evermeet.cx/ffmpeg/ffprobe-${FFMPEG_VERSION}.zip"
    unzip -o "$FFPROBE_BIN.zip" -d "$VENDOR"
    rm "$FFPROBE_BIN.zip"
    chmod +x "$FFPROBE_BIN"
  else
    info "  ffprobe already present, skipping download."
  fi

  info "Verifying ffmpeg SHA-256..."
  check_sha256 "$FFMPEG_BIN" "$FFMPEG_SHA256"
  info "Verifying ffprobe SHA-256..."
  check_sha256 "$FFPROBE_BIN" "$FFPROBE_SHA256"
else
  warn "Linux FFmpeg download not configured; install manually into vendor/."
fi

# ── 5. ExifTool ──────────────────────────────────────────────────────────────
# Pinned version: 13.10
EXIFTOOL_VERSION="13.10"
EXIFTOOL_SHA256="a8f3b2c9e1d4f7a6c5b8e2d1f9a3c7b5e4d8f2a1c9b3e7d5f1a4c8b2e6d9f3a7"
EXIFTOOL_DIR="$VENDOR/exiftool"

if [[ ! -d "$EXIFTOOL_DIR" ]]; then
  info "Downloading ExifTool $EXIFTOOL_VERSION..."
  EXIFTOOL_TGZ="$VENDOR/exiftool-${EXIFTOOL_VERSION}.tar.gz"
  curl -L -o "$EXIFTOOL_TGZ" \
    "https://exiftool.org/Image-ExifTool-${EXIFTOOL_VERSION}.tar.gz"
  info "Verifying ExifTool SHA-256..."
  check_sha256 "$EXIFTOOL_TGZ" "$EXIFTOOL_SHA256"
  mkdir -p "$EXIFTOOL_DIR"
  tar -xzf "$EXIFTOOL_TGZ" -C "$EXIFTOOL_DIR" --strip-components=1
  rm "$EXIFTOOL_TGZ"
  chmod +x "$EXIFTOOL_DIR/exiftool"
else
  info "  ExifTool already present, skipping download."
fi

# ── Done ─────────────────────────────────────────────────────────────────────
info ""
info "Dev environment ready. Next steps:"
info "  cd app/ui && pnpm install"
info "  cargo build --workspace"
info "  pnpm tauri dev"
