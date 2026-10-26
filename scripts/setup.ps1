# Keepsake dev-environment setup - Windows
# Re-running is safe; every step is idempotent.
#Requires -Version 5.1
# -VendorOnly: only download the pinned FFmpeg/ffprobe/ExifTool/LibRaw into vendor# (used by CI; skips installing Rust, Node and pnpm).
param([switch]$VendorOnly)
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Vendor   = Join-Path $RepoRoot "vendor"
New-Item -ItemType Directory -Force $Vendor | Out-Null

# -- Helpers ------------------------------------------------------------------
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

# -- Detect architecture ------------------------------------------------------
# $env:PROCESSOR_ARCHITECTURE is AMD64 on x64 and ARM64 on ARM64.
$Arch = $env:PROCESSOR_ARCHITECTURE
Write-Info "Architecture: $Arch"

if (-not $VendorOnly) {
# -- 1. Rust stable -----------------------------------------------------------
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

# -- 2. Node LTS via winget ---------------------------------------------------
Write-Info "Checking Node.js..."
if (-not (Get-Command node -ErrorAction SilentlyContinue)) {
    Write-Info "Installing Node.js LTS via winget..."
    winget install --id OpenJS.NodeJS.LTS -e --accept-package-agreements --accept-source-agreements
    $env:PATH = "$env:ProgramFiles\nodejs;$env:PATH"
}
$nodeVer = node --version
Write-Info "  OK Node $nodeVer"

# -- 3. pnpm ------------------------------------------------------------------
Write-Info "Checking pnpm..."
if (-not (Get-Command pnpm -ErrorAction SilentlyContinue)) {
    npm install -g pnpm
}
$pnpmVer = pnpm --version
Write-Info "  OK pnpm $pnpmVer"
} # end if (-not $VendorOnly)

# -- 4. FFmpeg / ffprobe (LGPL, BtbN build, mirrored) ------------------------
# Source: https://github.com/BtbN/FFmpeg-Builds - LGPL builds include no GPL
# encoders.  We pin to FFmpeg 9.0 (BtbN autobuild tag latest at time of
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
        Write-Err "Unexpected archive layout in $FfmpegZipName - no top-level folder found."
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

# Verify the installed build is LGPL (neither GPL nor nonfree).
# --enable-version3 means LGPLv3, which is acceptable under ADR-005.
Write-Info "Verifying FFmpeg licence..."
$ffVer = (& $FfmpegBin -version 2>&1) -join " "
if ($ffVer -match "--enable-gpl") {
    Write-Err "Licence check failed: ffmpeg reports --enable-gpl.`n  This is a GPL build - replace vendor\ffmpeg.exe with an LGPL build.`n  Binary: $FfmpegBin"
}
if ($ffVer -match "--enable-nonfree") {
    Write-Err "Licence check failed: ffmpeg reports --enable-nonfree.`n  Binary: $FfmpegBin"
}
$licenceLabel = if ($ffVer -match "--enable-version3") { "LGPLv3" } else { "LGPLv2.1" }
Write-Info "  OK licence verified: $licenceLabel (no --enable-gpl, no --enable-nonfree)"
$ffVerLine = ($ffVer -split "ffmpeg version")[1] -split " " | Select-Object -First 1
Write-Info "  ffmpeg version: $ffVerLine"

# -- 5. ExifTool (Windows 64-bit standalone, SourceForge mirror) --------------
# Source: https://exiftool.org (downloads via SourceForge).
# The _64 standalone build ships exiftool(-k).exe PLUS an exiftool_files\
# directory that contains the bundled Perl DLLs.  Both must live side-by-side
# or the exe fails with: Could not find exiftool_files\perl5*.dll
#
# Install layout:  vendor\exiftool\exiftool.exe
#                  vendor\exiftool\exiftool_files\  (Perl runtime)
$ExiftoolVersion = "13.59"
$ExiftoolZipName = "exiftool-13.59_64.zip"
$ExiftoolZipSHA  = "44b512b25af500724ba579d0a53c8fc5851628b692dd5e5d94ae4a15c2cba9ec"
$ExiftoolDir     = Join-Path $Vendor "exiftool"
$ExiftoolBin     = Join-Path $ExiftoolDir "exiftool.exe"
$ExiftoolFiles   = Join-Path $ExiftoolDir "exiftool_files"

# A complete install requires both the exe and the Perl runtime folder.
# Remove any half-extracted attempt so a re-run starts fresh.
if ((Test-Path $ExiftoolDir) -and ((-not (Test-Path $ExiftoolBin)) -or (-not (Test-Path $ExiftoolFiles)))) {
    Write-Warn "Incomplete ExifTool installation detected - cleaning up for fresh extraction."
    Remove-Item $ExiftoolDir -Recurse -Force
}

if (-not (Test-Path $ExiftoolBin)) {
    $ExiftoolZip = Join-Path $Vendor $ExiftoolZipName
    Invoke-MirroredDownload -FileName $ExiftoolZipName -OutFile $ExiftoolZip -ExpectedSHA256 $ExiftoolZipSHA

    Write-Info "Extracting ExifTool..."
    $ExiftoolExtract = Join-Path $Vendor "exiftool-extract"
    # Wipe any leftover extract temp from a prior failed run.
    if (Test-Path $ExiftoolExtract) { Remove-Item $ExiftoolExtract -Recurse -Force }
    Expand-Archive -Path $ExiftoolZip -DestinationPath $ExiftoolExtract -Force

    # The ZIP may have a top-level folder or dump files at the root.
    # Locate exiftool(-k).exe wherever it lands.
    $ExiftoolExe = Get-ChildItem $ExiftoolExtract -Recurse -Filter "exiftool*.exe" |
        Sort-Object Name | Select-Object -First 1
    if (-not $ExiftoolExe) {
        Write-Err "exiftool*.exe not found after extracting $ExiftoolZipName"
    }
    $ExiftoolFilesSource = Join-Path $ExiftoolExe.DirectoryName "exiftool_files"
    if (-not (Test-Path $ExiftoolFilesSource)) {
        Write-Err ("exiftool_files\ not found next to " + $ExiftoolExe.Name + " in $ExiftoolZipName" +
                   "`n  The Perl runtime DLLs are required alongside the exe.")
    }

    New-Item -ItemType Directory -Force $ExiftoolDir | Out-Null
    Copy-Item $ExiftoolExe.FullName      $ExiftoolBin    -Force
    Copy-Item $ExiftoolFilesSource       $ExiftoolDir    -Recurse -Force

    Remove-Item $ExiftoolExtract -Recurse -Force
    Remove-Item $ExiftoolZip     -Force
    Write-Info "  Installed exiftool.exe and exiftool_files\"
} else {
    Write-Info "  ExifTool already present, skipping download."
}

# Smoke-test: -ver must return a bare version number.
$etVer = (& $ExiftoolBin -ver 2>&1) | Select-Object -First 1
if (-not ($etVer -match "^\d")) {
    Write-Err ("ExifTool smoke-test failed (unexpected output: " + $etVer + ")`n  Binary: $ExiftoolBin")
}
Write-Info "  OK ExifTool $etVer"

# -- 6. LibRaw dcraw_emu (RAW decoding sidecar, ADR-023) -----------------------
# Official LibRaw 0.22.2 Windows build (MSVC, x64), mirrored unchanged.
# LGPL-2.1 or CDDL-1.0; invoked as a process, never linked. Windows on ARM
# runs the x64 build under emulation (LibRaw publishes no ARM64 binary).
# Install layout:  vendor\libraw\dcraw_emu.exe + libraw.dll (+ licences)
$LibrawZipName = "LibRaw-0.22.2-Win64.zip"
$LibrawZipSHA  = "ac64fa12bb00a7581332d4c6ab918c0533fb3f119d6b668d47a6875410dca948"
$LibrawDir     = Join-Path $Vendor "libraw"
$LibrawBin     = Join-Path $LibrawDir "dcraw_emu.exe"

if (-not (Test-Path $LibrawBin)) {
    $LibrawZip = Join-Path $Vendor $LibrawZipName
    Invoke-MirroredDownload -FileName $LibrawZipName -OutFile $LibrawZip -ExpectedSHA256 $LibrawZipSHA
    $LibrawExtract = Join-Path $Vendor "libraw-extract"
    if (Test-Path $LibrawExtract) { Remove-Item $LibrawExtract -Recurse -Force }
    Expand-Archive -Path $LibrawZip -DestinationPath $LibrawExtract -Force
    $Root = Get-ChildItem $LibrawExtract -Directory | Select-Object -First 1
    New-Item -ItemType Directory -Force $LibrawDir | Out-Null
    foreach ($f in @("bin\dcraw_emu.exe", "bin\libraw.dll", "LICENSE.LGPL", "LICENSE.CDDL", "COPYRIGHT")) {
        $src = Join-Path $Root.FullName $f
        if (-not (Test-Path $src)) { Write-Err "$f not found in $LibrawZipName" }
        Copy-Item $src $LibrawDir -Force
    }
    Remove-Item $LibrawExtract -Recurse -Force
    Remove-Item $LibrawZip -Force
    Write-Info "  Installed dcraw_emu.exe and libraw.dll"
} else {
    Write-Info "  LibRaw already present, skipping download."
}
# Smoke-test: dcraw_emu prints its usage (and version) with no arguments.
$lrOut = (& $LibrawBin 2>&1 | Out-String)
if ($lrOut -notmatch "LibRaw") {
    Write-Err "dcraw_emu smoke-test failed`n  Binary: $LibrawBin"
}
# Its usage exit status (non-zero) must not become the script's.
$global:LASTEXITCODE = 0
Write-Info "  OK LibRaw dcraw_emu"

# -- Done ---------------------------------------------------------------------
Write-Info ""
Write-Info "Dev environment ready. Next steps:"
Write-Info "  cd app\ui; pnpm install"
Write-Info "  cargo build --workspace"
Write-Info "  pnpm tauri dev"
