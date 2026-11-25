#!/usr/bin/env bash
set -euo pipefail
dme="$1"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ffmpeg -v error -f lavfi -i testsrc2=size=128x96:rate=30 -frames:v 1 -threads 1 "$tmp/input.png"
ffmpeg -v error -f lavfi -i testsrc2=size=128x96:rate=30 -t 1.2 -threads 1 -c:v ffv1 "$tmp/input.mkv"
for ext in png mkv; do
  "$dme" --analyze "$tmp/input.$ext" 2 > "$tmp/analysis1"
  "$dme" --analyze "$tmp/input.$ext" 2 > "$tmp/analysis2"
  cmp "$tmp/analysis1" "$tmp/analysis2"
  grep -q '"filter":"scale=\|"filter":"deblock=\|"filter":"hqdn3d=' "$tmp/analysis1"
  for preset in auto restore denoise; do
    "$dme" "$tmp/input.$ext" "$tmp/a.$ext" 2 "$preset"
    "$dme" "$tmp/input.$ext" "$tmp/b.$ext" 2 "$preset"
    cmp "$tmp/a.$ext" "$tmp/b.$ext"
    test "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$tmp/a.$ext")" = "256,192"
    rm "$tmp/a.$ext" "$tmp/b.$ext"
  done
done
"$dme" --analyze "$tmp/input.mkv" 2 | grep -q '"frames":3'
echo "Adaptive reports and new presets reproduce byte-for-byte."
