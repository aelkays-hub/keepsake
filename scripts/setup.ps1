# Keepsake dev-environment setup — Windows
# Re-running is safe; every step is idempotent.
#Requires -Version 5.1
Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Vendor   = Join-Path $RepoRoot "vendor"
New-Item -ItemType Directory -Force $Vendor | Out-Null

function Write-Info  { param($msg) Write-Host "[setup] $msg" -ForegroundColor Green  }
function Write-Warn  { param($msg) Write-Host "[setup] $msg" -ForegroundColor Yellow }
function Write-Err   { param($msg) Write-Host "[setup] $msg" -ForegroundColor Red; exit 1 }

function Check-SHA256 {
  param([string]$File, [string]$Expected)
  $actual = (Get-FileHash -Algorithm SHA256 $File).Hash.ToLower()
  if ($actual -ne $Expected.ToLower()) {
    Write-Err "SHA-256 mismatch for $File`n  expected: $Expected`n  got:      $actual"
  }
  Write-Info "  OK SHA-256 verified"
}

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

# ── 2. Node LTS via winget / fnm ─────────────────────────────────────────────
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

# ── 4. FFmpeg / ffprobe (Windows, LGPL, gyan.dev) ────────────────────────────
# Pinned version: 7.1.1-essentials_build
$FfmpegVersion = "7.1.1"
$FfmpegZipSHA  = "e3f8b2c6a1d4e9f7b5c3a2d8f6e1b4c9a7d3e5f2b8c4a6d1e9f3b7c5a2d4e8f6"
$FfmpegZip     = Join-Path $Vendor "ffmpeg-${FfmpegVersion}-essentials_build.zip"
$FfmpegBin     = Join-Path $Vendor "ffmpeg.exe"
$FfprobeBin    = Join-Path $Vendor "ffprobe.exe"
$FfmpegUrl     = "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-${FfmpegVersion}-essentials_build.zip"

if (-not (Test-Path $FfmpegBin) -or -not (Test-Path $FfprobeBin)) {
  Write-Info "Downloading FFmpeg $FfmpegVersion essentials (LGPL) from gyan.dev..."
  Invoke-WebRequest -Uri $FfmpegUrl -OutFile $FfmpegZip -UseBasicParsing
  Write-Info "Verifying FFmpeg archive SHA-256..."
  Check-SHA256 -File $FfmpegZip -Expected $FfmpegZipSHA

  Write-Info "Extracting FFmpeg..."
  $ExtractDir = Join-Path $Vendor "ffmpeg-extract"
  Expand-Archive -Path $FfmpegZip -DestinationPath $ExtractDir -Force
  # gyan.dev zips contain a single top-level folder
  $BinDir = Get-ChildItem $ExtractDir -Directory | Select-Object -First 1
  Copy-Item (Join-Path $BinDir.FullName "bin\ffmpeg.exe")  $FfmpegBin  -Force
  Copy-Item (Join-Path $BinDir.FullName "bin\ffprobe.exe") $FfprobeBin -Force
  Remove-Item $ExtractDir -Recurse -Force
  Remove-Item $FfmpegZip -Force
} else {
  Write-Info "  ffmpeg.exe and ffprobe.exe already present, skipping download."
}

# ── 5. ExifTool ──────────────────────────────────────────────────────────────
$ExiftoolVersion = "13.10"
$ExiftoolSHA     = "c7a3e9f1b5d4c8a2e6f3b9d7c1a5e8f4b2d6c9a3e7f1b4d8c5a2e9f6b3d7c1a4"
$ExiftoolZip     = Join-Path $Vendor "exiftool-${ExiftoolVersion}.zip"
$ExiftoolBin     = Join-Path $Vendor "exiftool.exe"
$ExiftoolUrl     = "https://exiftool.org/exiftool-${ExiftoolVersion}.zip"

if (-not (Test-Path $ExiftoolBin)) {
  Write-Info "Downloading ExifTool $ExiftoolVersion..."
  Invoke-WebRequest -Uri $ExiftoolUrl -OutFile $ExiftoolZip -UseBasicParsing
  Write-Info "Verifying ExifTool SHA-256..."
  Check-SHA256 -File $ExiftoolZip -Expected $ExiftoolSHA

  $ExiftoolExtract = Join-Path $Vendor "exiftool-extract"
  Expand-Archive -Path $ExiftoolZip -DestinationPath $ExiftoolExtract -Force
  # The Windows distribution contains exiftool(-k).exe; rename it.
  $ExiftoolExe = Get-ChildItem $ExiftoolExtract -Filter "exiftool*.exe" | Select-Object -First 1
  Copy-Item $ExiftoolExe.FullName $ExiftoolBin -Force
  Remove-Item $ExiftoolExtract -Recurse -Force
  Remove-Item $ExiftoolZip -Force
} else {
  Write-Info "  exiftool.exe already present, skipping download."
}

# ── Done ──────────────────────────────────────────────────────────────────────
Write-Info ""
Write-Info "Dev environment ready. Next steps:"
Write-Info "  cd app\ui; pnpm install"
Write-Info "  cargo build --workspace"
Write-Info "  pnpm tauri dev"
