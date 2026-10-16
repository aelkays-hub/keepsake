<#
.SYNOPSIS
    Spike-playback backend timing script (Gate 1).

.DESCRIPTION
    Measures / verifies items 2-4 of the Prompt 2 spike:
      Item 2 - H.264 proxy generation from XAVC S clips and synthetic sources
      Item 3 - AVCHD capture-date extraction and segment join
      Item 4 - ARW embedded preview extraction timing

    Prerequisites:
      - $env:KEEPSAKE_CORPUS set to the corpus root
      - vendor\ffmpeg.exe, vendor\ffprobe.exe, vendor\exiftool\exiftool.exe present

    Output:
      docs\spikes\playback-timings.txt (appended each run)
#>

param(
    [switch]$SkipProxy,
    [switch]$SkipAvchd,
    [switch]$SkipArw
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

$Root     = Split-Path -Parent $PSScriptRoot
$Vendor   = Join-Path $Root "vendor"
$FFmpeg   = Join-Path $Vendor "ffmpeg.exe"
$FFprobe  = Join-Path $Vendor "ffprobe.exe"
$ExifTool = Join-Path $Vendor "exiftool\exiftool.exe"
$Corpus   = $env:KEEPSAKE_CORPUS
$OutDir   = Join-Path $Root "docs\spikes"
$OutFile  = Join-Path $OutDir "playback-timings.txt"

if (-not $Corpus) { throw "KEEPSAKE_CORPUS is not set." }

$null = New-Item -ItemType Directory -Force $OutDir

function Log([string]$msg) {
    $ts = Get-Date -Format "HH:mm:ss"
    $line = "[$ts] $msg"
    Write-Host $line
    Add-Content $OutFile $line
}

function Separator([string]$title) {
    $line = "`n--- $title ---"
    Write-Host $line
    Add-Content $OutFile $line
}

$timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
Add-Content $OutFile "`n==== spike-playback run $timestamp ===="

# ── helpers ────────────────────────────────────────────────────────────────────

function Get-Duration([string]$path) {
    $json = & $FFprobe -v quiet -print_format json -show_format $path | ConvertFrom-Json
    return [double]$json.format.duration
}

function Run-Proxy {
    param([string]$InputPath, [string]$OutputPath, [string]$Label)
    if (Test-Path $OutputPath) { Remove-Item $OutputPath }
    $durSec = Get-Duration $InputPath

    # Encoder preference order (LGPL build: no libx264/libx265):
    #   1. h264_nvenc (NVIDIA) — fastest if GPU present
    #   2. h264_qsv  (Intel Quick Sync)
    #   3. h264_mf   (Windows MediaFoundation, always available)
    $encoders = @(
        @{name="h264_nvenc"; extra=@("-preset","p4","-b:v","4M")},
        @{name="h264_qsv";   extra=@("-global_quality","25")},
        @{name="h264_mf";    extra=@("-q:v","20")}
    )

    $usedEncoder = "unknown"
    $logFile = Join-Path $env:TEMP "ffmpeg-proxy.log"
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    foreach ($enc in $encoders) {
        $encArgs = @(
            "-i", $InputPath,
            "-c:v", $enc.name
        ) + $enc.extra + @(
            "-vf", "scale=-2:720",
            "-c:a", "aac", "-b:a", "128k",
            "-movflags", "+faststart",
            "-y", $OutputPath
        )
        $sw.Restart()
        & $FFmpeg @encArgs 2>$logFile | Out-Null
        if ($LASTEXITCODE -eq 0 -and (Test-Path $OutputPath)) {
            $usedEncoder = $enc.name
            break
        }
    }
    $sw.Stop()

    if ($usedEncoder -eq "unknown") {
        Log "$Label  ERROR: all encoders failed — check $env:TEMP\ffmpeg-proxy.log"
        return
    }

    $elapsedSec = [math]::Round($sw.Elapsed.TotalSeconds, 3)
    $speedup    = if ($durSec -gt 0) { [math]::Round($durSec / $elapsedSec, 2) } else { "N/A" }
    $perMin     = if ($durSec -gt 0) { [math]::Round($elapsedSec / ($durSec / 60), 1) } else { "N/A" }
    Log "$Label  encoder=$usedEncoder  clip=${durSec}s  wall=${elapsedSec}s  ${perMin}s/min  ${speedup}x"
    return $usedEncoder
}

# ── Item 2a: XAVC S clips (H.264 in corpus) ───────────────────────────────────

if (-not $SkipProxy) {
    Separator "Item 2a: XAVC S proxy generation"

    $ClipDir = Join-Path $Corpus "cards\ILCE-7M4\PRIVATE\M4ROOT\CLIP"
    $TmpDir  = Join-Path $env:TEMP "keepsake-spike-proxies"
    $null = New-Item -ItemType Directory -Force $TmpDir

    foreach ($clip in @("C0006", "C0007", "C0008")) {
        $src = Join-Path $ClipDir "$clip.MP4"
        if (-not (Test-Path $src)) {
            Log "SKIP $clip — not found at $src"
            continue
        }
        $dst = Join-Path $TmpDir "$clip-proxy.mp4"
        Run-Proxy -InputPath $src -OutputPath $dst -Label "XAVC-S $clip"
    }

    # ── Item 2b: synthetic "heavy 4K" source (stand-in for XAVC HS 10-bit) ────
    # LGPL FFmpeg: libx264/libx265 disabled. Use h264_mf (high quality) as
    # source so we measure transcoding-to-proxy speed regardless of source codec.
    Separator "Item 2b: Synthetic 4K source → proxy (XAVC HS 10-bit stand-in)"

    $syntheticHevc = Join-Path $TmpDir "synthetic_4k_hq.mp4"
    if (-not (Test-Path $syntheticHevc)) {
        Log "Generating synthetic 30s 4K clip via h264_mf (high quality)..."
        $genLog = Join-Path $env:TEMP "ffmpeg-gen.log"
        & $FFmpeg -y -f lavfi -i "testsrc2=size=3840x2160:rate=30:duration=30" `
                  -f lavfi -i "sine=frequency=440:duration=30" `
                  -c:v h264_mf -q:v 5 -c:a aac -b:a 64k $syntheticHevc 2>$genLog | Out-Null
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $syntheticHevc)) {
            Log "WARNING: synthetic 4K generation failed — trying libopenh264..."
            & $FFmpeg -y -f lavfi -i "testsrc2=size=3840x2160:rate=30:duration=30" `
                      -f lavfi -i "sine=frequency=440:duration=30" `
                      -c:v libopenh264 -b:v 40M -c:a aac -b:a 64k $syntheticHevc 2>$genLog | Out-Null
            if ($LASTEXITCODE -ne 0 -or -not (Test-Path $syntheticHevc)) {
                Log "SKIP 2b: both encoder attempts failed (see $genLog)"
                $syntheticHevc = $null
            }
        }
    }
    if ($syntheticHevc -and (Test-Path $syntheticHevc)) {
        $dstHevc = Join-Path $TmpDir "synthetic_4k_proxy.mp4"
        Run-Proxy -InputPath $syntheticHevc -OutputPath $dstHevc -Label "Synthetic-4K"
    }

    # ── Item 2c: synthetic XAVC S-I (H.264 intra) source ─────────────────────
    Separator "Item 2c: Synthetic H.264-intra source → proxy (XAVC S-I stand-in)"

    $syntheticIntra = Join-Path $TmpDir "synthetic_h264_intra.mp4"
    if (-not (Test-Path $syntheticIntra)) {
        Log "Generating synthetic 30s 1080p60 intra-only clip via h264_mf..."
        $genLog = Join-Path $env:TEMP "ffmpeg-gen.log"
        & $FFmpeg -y -f lavfi -i "testsrc2=size=1920x1080:rate=60:duration=30" `
                  -f lavfi -i "sine=frequency=440:duration=30" `
                  -c:v h264_mf -q:v 5 -c:a aac -b:a 64k $syntheticIntra 2>$genLog | Out-Null
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path $syntheticIntra)) {
            Log "WARNING: 1080p60 intra generation failed — trying libopenh264..."
            & $FFmpeg -y -f lavfi -i "testsrc2=size=1920x1080:rate=60:duration=30" `
                      -f lavfi -i "sine=frequency=440:duration=30" `
                      -c:v libopenh264 -b:v 20M -c:a aac -b:a 64k $syntheticIntra 2>$genLog | Out-Null
            if ($LASTEXITCODE -ne 0 -or -not (Test-Path $syntheticIntra)) {
                Log "SKIP 2c: both encoder attempts failed (see $genLog)"
                $syntheticIntra = $null
            }
        }
    }
    if ($syntheticIntra -and (Test-Path $syntheticIntra)) {
        $dstIntra = Join-Path $TmpDir "synthetic_intra_proxy.mp4"
        Run-Proxy -InputPath $syntheticIntra -OutputPath $dstIntra -Label "XAVC-S-I-intra"
    }

    Log "Proxy files written to: $TmpDir"
}

# ── Item 3: AVCHD capture-date + segment join ──────────────────────────────────

if (-not $SkipAvchd) {
    Separator "Item 3: AVCHD capture date + segment join"

    # Find MTS files in library-sample
    $LibSample = Join-Path $Corpus "library-sample"
    $mtsFiles  = @(Get-ChildItem $LibSample -Recurse -Filter "*.MTS" -ErrorAction SilentlyContinue |
                 Sort-Object FullName |
                 Select-Object -First 3)

    if ($mtsFiles.Count -eq 0) {
        Log "No MTS files found under $LibSample — skipping AVCHD tests"
    } else {
        foreach ($mts in $mtsFiles) {
            Log "Extracting capture date from: $($mts.FullName)"
            $exifRaw = & $ExifTool -DateTimeOriginal -CreateDate -TrackCreateDate `
                           -json $mts.FullName 2>$null
            $exifOut = try { $exifRaw | ConvertFrom-Json } catch { $null }
            if ($exifOut) {
                $e = if ($exifOut -is [array]) { $exifOut[0] } else { $exifOut }
                $dto = if ($e.PSObject.Properties['DateTimeOriginal']) { $e.DateTimeOriginal } else { "(none)" }
                $cd  = if ($e.PSObject.Properties['CreateDate'])       { $e.CreateDate }       else { "(none)" }
                $tcd = if ($e.PSObject.Properties['TrackCreateDate'])  { $e.TrackCreateDate }  else { "(none)" }
                Log "  DateTimeOriginal=$dto  CreateDate=$cd  TrackCreateDate=$tcd"
            } else {
                Log "  (no date tags found)"
            }
        }

        # Segment join via FFmpeg concat
        if ($mtsFiles.Count -ge 2) {
            Log "Joining $($mtsFiles.Count) MTS segments..."
            $concatList = Join-Path $env:TEMP "keepsake-spike-concat.txt"
            $mtsFiles | ForEach-Object { "file '$($_.FullName.Replace('\','/'))'`n" } | Set-Content $concatList
            $joinedOut  = Join-Path $env:TEMP "keepsake-spike-joined.mp4"
            if (Test-Path $joinedOut) { Remove-Item $joinedOut }

            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            & $FFmpeg -y -f concat -safe 0 -i $concatList -c copy $joinedOut 2>$null | Out-Null
            $sw.Stop()
            $totalDur = $mtsFiles | ForEach-Object { Get-Duration $_.FullName } |
                        Measure-Object -Sum | Select-Object -ExpandProperty Sum
            Log "Join complete  segments=$($mtsFiles.Count)  total=${totalDur}s  wall=$([math]::Round($sw.Elapsed.TotalSeconds,2))s"
        }
    }
}

# ── Item 4: ARW embedded preview timing ───────────────────────────────────────

if (-not $SkipArw) {
    Separator "Item 4: ARW embedded preview extraction"

    $LibSample = Join-Path $Corpus "library-sample"
    $arwFiles  = @(Get-ChildItem $LibSample -Recurse -Filter "*.ARW" -ErrorAction SilentlyContinue |
                 Sort-Object FullName |
                 Select-Object -First 5)

    if ($arwFiles.Count -eq 0) {
        # Try corpus cards
        $arwFiles = @(Get-ChildItem (Join-Path $Corpus "cards") -Recurse -Filter "*.ARW" -ErrorAction SilentlyContinue |
                    Sort-Object FullName |
                    Select-Object -First 5)
    }

    if ($arwFiles.Count -eq 0) {
        Log "No ARW files found — skipping ARW test"
    } else {
        $times = @()
        foreach ($arw in $arwFiles) {
            $outPreview = Join-Path $env:TEMP "keepsake-spike-preview.jpg"
            $sw = [System.Diagnostics.Stopwatch]::StartNew()
            & $ExifTool -b -PreviewImage $arw.FullName > $outPreview 2>$null
            $sw.Stop()
            $ms = [math]::Round($sw.Elapsed.TotalMilliseconds, 1)
            $times += $ms
            $sizeKb = if (Test-Path $outPreview) { [math]::Round((Get-Item $outPreview).Length / 1024) } else { 0 }
            Log "  $($arw.Name)  ${ms}ms  preview=${sizeKb}KB  $(if ($ms -le 150) { 'PASS' } else { 'FAIL (>150ms)' })"
        }
        $avg = [math]::Round(($times | Measure-Object -Average).Average, 1)
        Log "ARW preview avg: ${avg}ms  (target <= 150ms)"
    }
}

Separator "Done"
Log "Full results appended to: $OutFile"
Write-Host "`nTimings written to: $OutFile"
