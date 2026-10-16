#!/usr/bin/env bash
# scripts/make-fixtures.sh — regenerate tests/fixtures/ from FFmpeg lavfi sources.
#
# Requires vendor/ffmpeg to be populated (run scripts/setup.sh first).
# Falls back to text stubs when FFmpeg is absent.
#
# Usage:
#   ./scripts/make-fixtures.sh
#   ./scripts/make-fixtures.sh --stubs   # force stub mode

set -euo pipefail

STUBS=0
for arg in "$@"; do [[ "$arg" == "--stubs" ]] && STUBS=1; done

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FIXDIR="$ROOT/tests/fixtures"
FFMPEG="$ROOT/vendor/ffmpeg/bin/ffmpeg"

if [[ $STUBS -eq 0 && -x "$FFMPEG" ]]; then
    USE_FFMPEG=1
    echo "Using FFmpeg: $FFMPEG"
else
    USE_FFMPEG=0
    echo "WARNING: FFmpeg absent or --stubs passed — writing text stubs"
fi

new_mp4() {
    local dest="$1" color="${2:-red}"
    if [[ $USE_FFMPEG -eq 1 ]]; then
        "$FFMPEG" -y -loglevel error \
            -f lavfi -i "color=${color}:s=320x180:r=25" -t 1 \
            -c:v libx264 -preset ultrafast -crf 51 -an "$dest"
    else
        printf 'STUB_MP4:%s\n' "$(basename "$dest")" > "$dest"
    fi
}

new_mts() {
    local dest="$1"
    if [[ $USE_FFMPEG -eq 1 ]]; then
        "$FFMPEG" -y -loglevel error \
            -f lavfi -i 'color=blue:s=320x180:r=25' -t 1 \
            -c:v mpeg2video -b:v 1M -f mpegts "$dest"
    else
        printf 'STUB_MTS:%s\n' "$(basename "$dest")" > "$dest"
    fi
}

new_jpg() {
    local dest="$1" color="${2:-cyan}"
    if [[ $USE_FFMPEG -eq 1 ]]; then
        "$FFMPEG" -y -loglevel error \
            -f lavfi -i "color=${color}:s=320x180" -vframes 1 "$dest"
    else
        printf 'STUB_JPG:%s\n' "$(basename "$dest")" > "$dest"
    fi
}

sony_xml() {
    local dest="$1" ts="$2"
    cat > "$dest" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<NonRealTimeMeta xmlns="urn:schemas-professionalDisc:nonRealTimeMeta:ver.2.00" lastUpdate="$ts">
  <Device manufacturer="Sony" modelName="ILCE-7M4"/>
  <CreationDate value="$ts"/>
  <Duration value="PT1S"/>
  <VideoFormat>
    <VideoFrame videoCodec="AVC" captureFps="25"/>
  </VideoFormat>
</NonRealTimeMeta>
EOF
}

# ── XAVC ──────────────────────────────────────────────────────────────────────
CLIP="$FIXDIR/xavc/PRIVATE/M4ROOT/CLIP"
SUB="$FIXDIR/xavc/PRIVATE/M4ROOT/SUB"
mkdir -p "$CLIP" "$SUB"

new_mp4  "$CLIP/C0001.MP4"    red
new_mp4  "$SUB/C0001S01.MP4"  red
sony_xml "$CLIP/C0001M01.XML" "2024-01-15T10:30:00+09:00"

new_mp4  "$CLIP/C0002.MP4"    green
new_mp4  "$SUB/C0002S01.MP4"  green
sony_xml "$CLIP/C0002M01.XML" "2024-01-15T10:35:00+09:00"

# ── AVCHD ─────────────────────────────────────────────────────────────────────
BDMV="$FIXDIR/avchd/PRIVATE/AVCHD/BDMV"
mkdir -p "$BDMV/STREAM" "$BDMV/CLIPINF" "$BDMV/PLAYLIST"

new_mts "$BDMV/STREAM/00001.MTS"
new_mts "$BDMV/STREAM/00002.MTS"
printf '\x43\x50\x49\x00\x00\x00\x00\x00' > "$BDMV/CLIPINF/00001.CPI"
printf '\x43\x50\x49\x00\x00\x00\x00\x00' > "$BDMV/CLIPINF/00002.CPI"
printf '\x4D\x50\x4C\x53\x00\x00\x00\x00' > "$BDMV/PLAYLIST/00001.MPL"
printf '\x49\x4E\x44\x58\x30\x32\x30\x30' > "$BDMV/INDEX.BDM"
printf '\x4D\x4F\x42\x4A\x30\x32\x30\x30' > "$BDMV/MOVIEOBJ.BDM"

# ── DCIM ──────────────────────────────────────────────────────────────────────
DCIM="$FIXDIR/dcim/DCIM/100MSDCF"
mkdir -p "$DCIM"

new_jpg "$DCIM/DSC00001.JPG" cyan
new_jpg "$DCIM/DSC00002.JPG" yellow
printf '\x49\x49\x2A\x00\x08\x00\x00\x00' > "$DCIM/DSC00001.ARW"
printf '\x00\x00\x00\x18\x66\x74\x79\x70\x6D\x69\x66\x31' > "$DCIM/DSC00001.HIF"

# ── MP_ROOT ───────────────────────────────────────────────────────────────────
MPROOT="$FIXDIR/mp_root/MP_ROOT/100ANV01"
mkdir -p "$MPROOT"

new_mp4 "$MPROOT/MAH00001.MP4" purple
new_mp4 "$MPROOT/MAH00002.MP4" orange

echo
echo "Fixtures written to $FIXDIR"
