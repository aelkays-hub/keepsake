# Keepsake dev-environment setup — Windows
# Re-running is safe; every step is idempotent.
#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Vendor   = Join-Path $RepoRoot "vendor"
New-Item -ItemType Directory -Force $Vendor | Out-Null

# ── Helpers ───────────────────────────────────────────────────────────────────
function Write-Info { param($msg) Write-Host "[setup] $msg" -ForegroundColor Green  }
function Write-Warn { param($msg) Write-Host "[setup] $msg" -ForegroundColor Yellow }
function Write-Err  { param($msg) Write-Host "[setup] $msg" -ForegroundColor Red; exit 1 }

# Download $FileName from the vendor-binaries mirror, save to $OutFile, and
# verify its SHA-256.  Exits with a clear message (naming file + URL) on any
# failure.
$MirrorBase = "https://github.com/aelkays-hub/keepsake/releases/download/vendor-binaries-v1"

function Invoke-MirroredDownload {
    param(
        [string]$FileName,
        [string]$OutFile,
        [string]$ExpectedSHA256
    )
    $url = "$MirrorBase/$FileName"
    Write-Info "Downloading $FileName ..."
    try {
        Invoke-WebRequest -Uri $url -OutFile $OutFile -UseBasicParsing -MaximumRedirection 10
    } catch {
        Write-Err "Download failed: $FileName`n  URL:   $url`n  Error: $_"
    }
    if (-not (Test-Path $OutFile) -or (Get-Item $OutFile).Length -lt 1024) {
        Write-Err "Downloaded file is missing or suspiciously small: $OutFile`n  URL: $url"
    }
    Write-Info "Verifying SHA-256 for $FileName ..."
    $actual = (Get-FileHash -Algorithm SHA256 $OutFile).Hash.ToLower()
    if ($actual -ne $ExpectedSHA256.ToLower()) {
        # Remove the corrupt/wrong file so a re-run retries the download.
        Remove-Item $OutFile -Force -ErrorAction SilentlyContinue
        Write-Err "SHA-256 mismatch: $FileName`n  URL:      $url`n  expected: $ExpectedSHA256`n  got:      $actual"
    }
    Write-Info "  OK SHA-256 verified"
}

# ── Detect architecture ────────────────────────────────────────────────────────
# $env:PROCESSOR_ARCHITECTURE is 'AMD64' on x64 and 'ARM64' on ARM64.
$Arch = $env:PROCESSOR_ARCHITECTURE
Write-Info "Architecture: $Arch"

# ── 1. Rust stable ────────────────────────────────────────────────────────────
Write-Info "Checking Rust toolchain..."
if (-not (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Write-Info "Installing rustup via winget..."
    winget install --id Rustlang.Rustup -e --accept-package-agreements --accept-source-agreements
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
} else {
    rustup toolchain install stable --no-self-update
    rustup default stable
}
$rustVer = rustc --version
Write-Info "  OK $rustVer"

# ── 2. Node LTS via winget ────────────────────────────────────────────────────
Write-Info "Checking Node.js..."
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Info "Installing Node.js LTS via winget..."
    winget install --id OpenJS.NodeJS.LTS -e --accept-package-agreements --accept-source-agreements
    $env:PATH = "$env:ProgramFiles\nodejs;$env:PATH"
}
$nodeVer = node --version
Write-Info "  OK Node $nodeVer"

# ── 3. pnpm ───────────────────────────────────────────────────────────────────
Write-Info "Checking pnpm..."
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    npm install -g pnpm
}
$pnpmVer = pnpm --version
Write-Info "  OK pnpm $pnpmVer"

# ── 4. FFmpeg / ffprobe (LGPL, BtbN build, mirrored) ─────────────────────────
# Source: https://github.com/BtbN/FFmpeg-Builds — LGPL builds include no GPL
# encoders.  We pin to FFmpeg 9.0 (BtbN autobuild tag "latest" at time of
# vendor-binaries-v1 release).
#
# Architecture-specific assets:
#   x64  : ffmpeg-n9.0-latest-win64-lgpl-9.0.zip   (SHA-256 below)
#   ARM64 : ffmpeg-n9.0-latest-winarm64-lgpl-9.0.zip (SHA-256 below)
$FfmpegBin   = Join-Path $Vendor "ffmpeg.exe"
$FfprobeBin  = Join-Path $Vendor "ffprobe.exe"

if (-not (Test-Path $FfmpegBin) -or -not (Test-Path $FfprobeBin)) {
    if ($Arch -eq "ARM64") {
        $FfmpegZipName = "ffmpeg-n9.0-latest-winarm64-lgpl-9.0.zip"
        $FfmpegZipSHA  = "33cb51e1529c1d658eb814c0cd557272f35a83951d7bb8009ac6bbe7854b0cc6"
    } else {
        # Default to x64 (covers AMD64 and any future alias).
        $FfmpegZipName = "ffmpeg-n9.0-latest-win64-lgpl-9.0.zip"
        $FfmpegZipSHA  = "73950cc6d7548c08d435bfb9c65b60798b6cd59647e59bb41e9af3876c90ebef"
    }

    $FfmpegZip = Join-Path $Vendor $FfmpegZipName
    Invoke-MirroredDownload -FileName $FfmpegZipName -OutFile $FfmpegZip -ExpectedSHA256 $FfmpegZipSHA

    Write-Info "Extracting FFmpeg..."
    $ExtractDir = Join-Path $Vendor "ffmpeg-extract"
    Expand-Archive -Path $FfmpegZip -DestinationPath $ExtractDir -Force
    # BtbN ZIPs have a single top-level folder with a bin\ subdirectory.
    $BinDir = Get-ChildItem $ExtractDir -Directory | Select-Object -First 1
    if (-not $BinDir) {
        Write-Err "Unexpected archive layout in $FfmpegZipName — no top-level folder found."
    }
    $src_ffmpeg  = Join-Path $BinDir.FullName "bin\ffmpeg.exe"
    $src_ffprobe = Join-Path $BinDir.FullName "bin\ffprobe.exe"
    foreach ($f in @($src_ffmpeg, $src_ffprobe)) {
        if (-not (Test-Path $f)) {
            Write-Err "Expected binary not found in archive: $f"
        }
    }
    Copy-Item $src_ffmpeg  $FfmpegBin  -Force
    Copy-Item $src_ffprobe $FfprobeBin -Force
    Remove-Item $ExtractDir -Recurse -Force
    Remove-Item $FfmpegZip  -Force
    Write-Info "  Installed ffmpeg.exe and ffprobe.exe"
} else {
    Write-Info "  ffmpeg.exe and ffprobe.exe already present, skipping download."
}

# Verify the installed build is LGPL (no --enable-gpl, no --enable-version3).
Write-Info "Verifying FFmpeg is an LGPL build..."
$ffVer = (& $FfmpegBin -version 2>&1) -join " "
if ($ffVer -match "--enable-gpl") {
    Write-Err "LGPL check failed: ffmpeg reports --enable-gpl.`n  This is a GPL build — replace vendor\ffmpeg.exe with an LGPL build."
}
if ($ffVer -match "--enable-version3") {
    Write-Err "LGPL check failed: ffmpeg reports --enable-version3.`n  Expected a pure LGPLv2.1 build without version3 extensions."
}
Write-Info "  OK LGPL verified (no --enable-gpl, no --enable-version3)"
$ffVerLine = ($ffVer -split "ffmpeg version")[1] -split " "| Select-Object -First 1
Write-Info "  ffmpeg version: $ffVerLine"

# ── 5. ExifTool (Windows 64-bit standalone, SourceForge mirror) ───────────────
# Source: https://exiftool.org (downloads via SourceForge).
# We use the _64 standalone build (Perl bundled; no external dependencies).
$ExiftoolVersion = "13.59"
$ExiftoolZipName = "exiftool-13.59_64.zip"
$ExiftoolZipSHA  = "44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec"
$ExiftoolBin     = Join-Path $Vendor "exiftool.exe"

if (-not (Test-Path $ExiftoolBin)) {
    $ExiftoolZip = Join-Path $Vendor $ExiftoolZipName
    Invoke-MirroredDownload -FileName $ExiftoolZipName -OutFile $ExiftoolZip -ExpectedSHA256 $ExiftoolZipSHA

    Write-Info "Extracting ExifTool..."
    $ExiftoolExtract = Join-Path $Vendor "exiftool-extract"
    Expand-Archive -Path $ExiftoolZip -DestinationPath $ExiftoolExtract -Force
    # The Windows package may name the exe 'exiftool.exe' or 'exiftool(-k).exe'.
    $ExiftoolExe = Get-ChildItem $ExiftoolExtract -Recurse -Filter "exiftool*.exe" |
        Sort-Object Name | Select-Object -First 1
    if (-not $ExiftoolExe) {
        Write-Err "exiftool*.exe not found after extracting $ExiftoolZipName"
    }
    Copy-Item $ExiftoolExe.FullName $ExiftoolBin -Force
    Remove-Item $ExiftoolExtract -Recurse -Force
    Remove-Item $ExiftoolZip     -Force
    Write-Info "  Installed exiftool.exe"
} else {
    Write-Info "  exiftool.exe already present, skipping download."
}

# Smoke-test ExifTool.
$etVer = (& $ExiftoolBin -ver 2>&1) | Select-Object -First 1
if (-not ($etVer -match "^\d")) {
    Write-Err "ExifTool smoke-test failed (unexpected output: $etVer)"
}
Write-Info "  OK ExifTool $etVer"

# ── Done ──────────────────────────────────────────────────────────────────────
Write-Info ""
Write-Info "Dev environment ready. Next steps:"
Write-Info "  cd app\ui; pnpm install"
Write-Info "  cargo build --workspace"
Write-Info "  pnpm tauri dev"
