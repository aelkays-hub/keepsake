#!/usr/bin/env pwsh
# scripts/make-fixtures.ps1 — regenerate tests/fixtures/ from FFmpeg lavfi sources.
#
# Requires vendor/ffmpeg to be populated (run scripts/setup.ps1 first).
# When FFmpeg is absent, falls back to tiny text stubs — these are sufficient
# for path/role/grouping tests but lack real container metadata.
#
# Usage:
#   .\scripts\make-fixtures.ps1
#   .\scripts\make-fixtures.ps1 -Stubs   # force stub mode even if FFmpeg present

param([switch]$Stubs)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$Root        = Split-Path $PSScriptRoot -Parent
$FixDir      = Join-Path $Root 'tests\fixtures'
$FFmpeg      = Join-Path $Root 'vendor\ffmpeg\bin\ffmpeg.exe'
$UseFFmpeg   = (-not $Stubs) -and (Test-Path $FFmpeg)

if ($UseFFmpeg) {
    Write-Host "Using FFmpeg: $FFmpeg"
} else {
    Write-Warning 'FFmpeg not found or -Stubs passed — writing text stubs'
}

function EnsureDir([string]$p) {
    if (-not (Test-Path $p)) { New-Item -ItemType Directory -Path $p -Force | Out-Null }
}

# 1s 320x180 H.264 MP4 via lavfi, or a stub.
function New-Mp4([string]$dest, [string]$color = 'red') {
    if ($UseFFmpeg) {
        & $FFmpeg -y -loglevel error `
            -f lavfi -i "color=${color}:s=320x180:r=25" -t 1 `
            -c:v libx264 -preset ultrafast -crf 51 -an $dest
    } else {
        Set-Content -Encoding UTF8 -Path $dest -Value "STUB_MP4:$([IO.Path]::GetFileName($dest))"
    }
}

# 1s 320x180 MPEG-2 Transport Stream (MTS) via lavfi, or a stub.
function New-Mts([string]$dest) {
    if ($UseFFmpeg) {
        & $FFmpeg -y -loglevel error `
            -f lavfi -i 'color=blue:s=320x180:r=25' -t 1 `
            -c:v mpeg2video -b:v 1M -f mpegts $dest
    } else {
        Set-Content -Encoding UTF8 -Path $dest -Value "STUB_MTS:$([IO.Path]::GetFileName($dest))"
    }
}

# 320x180 JPEG still via lavfi, or a stub.
function New-Jpg([string]$dest, [string]$color = 'cyan') {
    if ($UseFFmpeg) {
        & $FFmpeg -y -loglevel error `
            -f lavfi -i "color=${color}:s=320x180" -vframes 1 $dest
    } else {
        Set-Content -Encoding UTF8 -Path $dest -Value "STUB_JPG:$([IO.Path]::GetFileName($dest))"
    }
}

# Minimal Sony XML sidecar.
function New-SonyXml([string]$dest, [string]$stem, [string]$ts) {
    @"
<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta xmlns="urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.00" lastUpdate="$ts">
  <Device manufacturer="Sony" modelName="ILCE-7M4"/>
  <CreationDate value="$ts"/>
  <Duration value="PT1S"/>
  <VideoFormat>
    <VideoFrame videoCodec="AVC" captureFps="25"/>
  </VideoFormat>
</NonRealTimeMeta>
"@ | Set-Content -Encoding UTF8 -Path $dest
}

# ── XAVC fixture ─────────────────────────────────────────────────────────────
$ClipDir = Join-Path $FixDir 'xavc\PRIVATE\M4ROOT\CLIP'
$SubDir  = Join-Path $FixDir 'xavc\PRIVATE\M4ROOT\SUB'
EnsureDir $ClipDir
EnsureDir $SubDir

$xavcs = @(
    @{ stem='C0001'; color='red';   ts='2024-01-15T10:30:00+09:00' },
    @{ stem='C0002'; color='green'; ts='2024-01-15T10:35:00+09:00' }
)
foreach ($x in $xavcs) {
    New-Mp4  (Join-Path $ClipDir "$($x.stem).MP4") $x.color
    New-Mp4  (Join-Path $SubDir  "$($x.stem)S01.MP4") $x.color
    New-SonyXml (Join-Path $ClipDir "$($x.stem)M01.XML") $x.stem $x.ts
}

# ── AVCHD fixture ─────────────────────────────────────────────────────────────
$BdmvDir = Join-Path $FixDir 'avchd\PRIVATE\AVCHD\BDMV'
EnsureDir (Join-Path $BdmvDir 'STREAM')
EnsureDir (Join-Path $BdmvDir 'CLIPINF')
EnsureDir (Join-Path $BdmvDir 'PLAYLIST')

foreach ($n in '00001','00002') {
    New-Mts (Join-Path $BdmvDir "STREAM\$n.MTS")
    # CPI and MPL are binary; write minimal identifying stubs
    [IO.File]::WriteAllBytes((Join-Path $BdmvDir "CLIPINF\$n.CPI"), [byte[]]@(0x43,0x50,0x49,0x00,0x00,0x00,0x00,0x00))
    [IO.File]::WriteAllBytes((Join-Path $BdmvDir "PLAYLIST\$n.MPL"), [byte[]]@(0x4D,0x50,0x4C,0x53,0x00,0x00,0x00,0x00))
}
[IO.File]::WriteAllBytes((Join-Path $BdmvDir 'INDEX.BDM'),    [byte[]]@(0x49,0x4E,0x44,0x58,0x30,0x32,0x30,0x30))
[IO.File]::WriteAllBytes((Join-Path $BdmvDir 'MOVIEOBJ.BDM'), [byte[]]@(0x4D,0x4F,0x42,0x4A,0x30,0x32,0x30,0x30))

# ── DCIM fixture ──────────────────────────────────────────────────────────────
$DcimDir = Join-Path $FixDir 'dcim\DCIM\100MSDCF'
EnsureDir $DcimDir

New-Jpg (Join-Path $DcimDir 'DSC00001.JPG') 'cyan'
New-Jpg (Join-Path $DcimDir 'DSC00002.JPG') 'yellow'
# ARW: TIFF little-endian magic bytes + stub
[IO.File]::WriteAllBytes((Join-Path $DcimDir 'DSC00001.ARW'), [byte[]]@(0x49,0x49,0x2A,0x00,0x08,0x00,0x00,0x00))
# HIF: HEIF ftyp box stub
[IO.File]::WriteAllBytes((Join-Path $DcimDir 'DSC00001.HIF'), [byte[]]@(0x00,0x00,0x00,0x18,0x66,0x74,0x79,0x70,0x6D,0x69,0x66,0x31))

# ── MP_ROOT fixture ───────────────────────────────────────────────────────────
$MpDir = Join-Path $FixDir 'mp_root\MP_ROOT\100ANV01'
EnsureDir $MpDir
New-Mp4 (Join-Path $MpDir 'MAH00001.MP4') 'purple'
New-Mp4 (Join-Path $MpDir 'MAH00002.MP4') 'orange'

Write-Host "`nFixtures written to $FixDir"
