# Keepsake dev-environment setup - Windows
# Re-running is safe; every step is idempotent.
#Requires -Version 5.1
# -VendorOnly: only download the pinned FFmpeg/ffprobe/ExifTool/LibRaw into vendor
# (used by CI; skips installing Rust, Node and pnpm).
# -Arch ARM64|AMD64: fetch binaries for another architecture (the ARM64
# release is built on an x64 runner). Default: this machine's.
# -WithAi: also download the AI models (372 MB, and 39 MB for faces) into vendor\models (ADR-035, ADR-040).
param([switch]$VendorOnly, [ValidateSet("", "AMD64", "ARM64")][string]$Arch = "", [switch]$WithAi)
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
$HostArch = $env:PROCESSOR_ARCHITECTURE
if (-not $Arch) { $Arch = $HostArch }
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

$FfmpegLicence = Join-Path $Vendor "LICENSE-ffmpeg.txt"
if (-not (Test-Path $FfmpegBin) -or -not (Test-Path $FfprobeBin) -or -not (Test-Path $FfmpegLicence)) {
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
    # The licence ships with the app beside the binaries (LGPL).
    $src_licence = Join-Path $BinDir.FullName "LICENSE.txt"
    if (-not (Test-Path $src_licence)) {
        Write-Err "Expected LICENSE.txt not found in archive: $src_licence"
    }
    Copy-Item $src_licence $FfmpegLicence -Force
    Remove-Item $ExtractDir -Recurse -Force
    Remove-Item $FfmpegZip  -Force
    Write-Info "  Installed ffmpeg.exe and ffprobe.exe"
} else {
    Write-Info "  ffmpeg.exe and ffprobe.exe already present, skipping download."
}

# Verify the installed build is LGPL (neither GPL nor nonfree).
# --enable-version3 means LGPLv3, which is acceptable under ADR-005.
Write-Info "Verifying FFmpeg licence..."
if ($Arch -ne $HostArch) {
    # Another architecture's binary cannot run here. The archive is pinned
    # by SHA-256 to a build checked as LGPL when it was mirrored (ADR-005).
    $ffVer = "ffmpeg version $Arch-not-run-here --enable-version3"
} else {
    $ffVer = (& $FfmpegBin -version 2>&1) -join " "
}
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
# Smoke-test: dcraw_emu prints its usage banner with no arguments.
$lrOut = (& $LibrawBin 2>&1 | Out-String)
if ($lrOut -cnotmatch "dcraw emulator") {
    Write-Err "dcraw_emu smoke-test failed`n  Binary: $LibrawBin"
}
# Its usage exit status (non-zero) must not become the script's.
$global:LASTEXITCODE = 0
Write-Info "  OK LibRaw dcraw_emu"

# -- 7. ONNX Runtime with DirectML (on-device AI, ADR-035) --------------------
# Microsoft's own NuGet packages, pinned by SHA-256 (not yet on the mirror):
# Microsoft.ML.OnnxRuntime.DirectML 1.24.4 (MIT) and Microsoft.AI.DirectML
# 1.15.4 (Microsoft DirectML licence, redistributable). Loaded by Keepsake at
# run time, never linked. Install layout: vendor\onnxruntime\ (this $Arch).
$OrtDir = Join-Path $Vendor "onnxruntime"
$OrtDll = Join-Path $OrtDir "onnxruntime.dll"
$OrtRid = if ($Arch -eq "ARM64") { "win-arm64" } else { "win-x64" }
$DmlRid = if ($Arch -eq "ARM64") { "arm64-win" } else { "x64-win" }
$OrtStamp = Join-Path $OrtDir "arch.txt"

function Invoke-PinnedDownload {
    param([string]$Url, [string]$OutFile, [string]$ExpectedSHA256)
    Write-Info "Downloading $Url ..."
    try {
        Invoke-WebRequest -Uri $Url -OutFile $OutFile -UseBasicParsing -MaximumRedirection 10
    } catch {
        Write-Err "Download failed: $Url`n  Error: $_"
    }
    $actual = (Get-FileHash -Algorithm SHA256 $OutFile).Hash.ToLower()
    if ($actual -ne $ExpectedSHA256.ToLower()) {
        Remove-Item $OutFile -Force -ErrorAction SilentlyContinue
        Write-Err "SHA-256 mismatch: $Url`n  expected: $ExpectedSHA256`n  got:      $actual"
    }
    Write-Info "  OK SHA-256 verified"
}

$haveOrt = (Test-Path $OrtDll) -and (Test-Path $OrtStamp) -and ((Get-Content $OrtStamp -Raw).Trim() -eq $Arch)
if (-not $haveOrt) {
    $OrtPkg = Join-Path $Vendor "ort-dml.zip"
    $DmlPkg = Join-Path $Vendor "directml.zip"
    Invoke-PinnedDownload -Url "https://api.nuget.org/v3-flatcontainer/microsoft.ml.onnxruntime.directml/1.24.4/microsoft.ml.onnxruntime.directml.1.24.4.nupkg" `
        -OutFile $OrtPkg -ExpectedSHA256 "57e9f11b73437bef7a309496135d4c1f96b1a8e9ddba60013fa27bfc1d788681"
    Invoke-PinnedDownload -Url "https://api.nuget.org/v3-flatcontainer/microsoft.ai.directml/1.15.4/microsoft.ai.directml.1.15.4.nupkg" `
        -OutFile $DmlPkg -ExpectedSHA256 "4e7cb7ddce8cf837a7a75dc029209b520ca0101470fcdf275c1f49736a3615b9"
    $X = Join-Path $Vendor "ort-extract"
    foreach ($pair in @(@($OrtPkg, "ort"), @($DmlPkg, "dml"))) {
        $dest = Join-Path $X $pair[1]
        if (Test-Path $dest) { Remove-Item $dest -Recurse -Force }
        Expand-Archive -Path $pair[0] -DestinationPath $dest -Force
    }
    if (Test-Path $OrtDir) { Remove-Item $OrtDir -Recurse -Force }
    New-Item -ItemType Directory -Force $OrtDir | Out-Null
    foreach ($f in @("onnxruntime.dll", "onnxruntime_providers_shared.dll")) {
        Copy-Item (Join-Path $X "ort\runtimes\$OrtRid\native\$f") $OrtDir -Force
    }
    Copy-Item (Join-Path $X "dml\bin\$DmlRid\DirectML.dll") $OrtDir -Force
    Copy-Item (Join-Path $X "ort\LICENSE") (Join-Path $OrtDir "LICENSE-onnxruntime.txt") -Force
    Copy-Item (Join-Path $X "ort\ThirdPartyNotices.txt") $OrtDir -Force
    Copy-Item (Join-Path $X "dml\LICENSE.txt") (Join-Path $OrtDir "LICENSE-DirectML.txt") -Force
    Set-Content -Path $OrtStamp -Value $Arch
    Remove-Item $X -Recurse -Force
    Remove-Item $OrtPkg, $DmlPkg -Force
    Write-Info "  Installed ONNX Runtime 1.24.4 + DirectML 1.15.4 ($OrtRid)"
} else {
    Write-Info "  ONNX Runtime already present, skipping download."
}

# -- 8. AI model (optional, -WithAi) ------------------------------------------
# SigLIP 2 base image encoder (Apache-2.0), onnx-community revision ba1f3b08.
# The app downloads it into the user's cache when AI is turned on; this copy
# is for developers and the model tests.
if ($WithAi) {
    $ModelDir  = Join-Path $Vendor "models\siglip2-b16-224"
    $ModelFile = Join-Path $ModelDir "vision_model.onnx"
    $ModelSHA  = "c0573e3f4140c3a7c4e9cc5912bd6b26a033b46a6a8e8af26cbea262b163bcad"
    $ok = (Test-Path $ModelFile) -and ((Get-FileHash -Algorithm SHA256 $ModelFile).Hash.ToLower() -eq $ModelSHA)
    if (-not $ok) {
        New-Item -ItemType Directory -Force $ModelDir | Out-Null
        Invoke-PinnedDownload -Url "https://huggingface.co/onnx-community/siglip2-base-patch16-224-ONNX/resolve/ba1f3b0843f24bc5417d38e19c37b287d719b2f4/onnx/vision_model.onnx" `
            -OutFile $ModelFile -ExpectedSHA256 $ModelSHA
    } else {
        Write-Info "  AI model already present, skipping download."
    }
    # Face models (ADR-040): YuNet (MIT) and SFace (Apache-2.0), OpenCV Zoo
    # on Hugging Face at pinned revisions.
    $Faces = @(
        @{ Dir = "yunet-2023mar"; File = "face_detection_yunet_2023mar.onnx";
           Url = "https://huggingface.co/opencv/face_detection_yunet/resolve/3cc26e7f1014a5ee5d74a42acee58bafc9d0a310/face_detection_yunet_2023mar.onnx";
           SHA = "8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4" },
        @{ Dir = "sface-2021dec"; File = "face_recognition_sface_2021dec.onnx";
           Url = "https://huggingface.co/opencv/face_recognition_sface/resolve/3d7082438a6e4551e840c9b2bb60b71e8da4b524/face_recognition_sface_2021dec.onnx";
           SHA = "0ba9fbfa01b5270c96627c4ef784da859931e02f04419c829e83484087c34e79" }
    )
    foreach ($m in $Faces) {
        $Dir  = Join-Path $Vendor ("models\" + $m.Dir)
        $File = Join-Path $Dir $m.File
        $ok = (Test-Path $File) -and ((Get-FileHash -Algorithm SHA256 $File).Hash.ToLower() -eq $m.SHA)
        if (-not $ok) {
            New-Item -ItemType Directory -Force $Dir | Out-Null
            Invoke-PinnedDownload -Url $m.Url -OutFile $File -ExpectedSHA256 $m.SHA
        } else {
            Write-Info "  $($m.File) already present, skipping download."
        }
    }
}

# -- Done ---------------------------------------------------------------------
Write-Info ""
Write-Info "Dev environment ready. Next steps:"
Write-Info "  cd app\ui; pnpm install"
Write-Info "  cargo build --workspace"
Write-Info "  pnpm tauri dev"
