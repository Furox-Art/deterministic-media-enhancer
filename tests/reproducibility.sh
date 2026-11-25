#!/usr/bin/env bash
set -euo pipefail
dme="$1"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ffmpeg -v error -f lavfi -i testsrc2=size=64x48:rate=5 -frames:v 1 -threads 1 "$tmp/input.png"
ffmpeg -v error -f lavfi -i testsrc2=size=64x48:rate=5 -f lavfi -i sine=frequency=440 -t 1 -c:v ffv1 -c:a flac -threads 1 "$tmp/input.mkv"
for preset in clean detail resize; do
  "$dme" "$tmp/input.png" "$tmp/a.png" 2 "$preset"
  "$dme" "$tmp/input.png" "$tmp/b.png" 2 "$preset"
  cmp "$tmp/a.png" "$tmp/b.png"
  test "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$tmp/a.png")" = "128,96"
  rm "$tmp/a.png" "$tmp/b.png"
done
for ext in mkv mp4; do
  "$dme" "$tmp/input.mkv" "$tmp/a.$ext" 2 clean
  "$dme" "$tmp/input.mkv" "$tmp/b.$ext" 2 clean
  cmp "$tmp/a.$ext" "$tmp/b.$ext"
  test "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$tmp/a.$ext")" = "128,96"
  test -n "$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 "$tmp/a.$ext")"
done
if "$dme" "$tmp/input.png" "$tmp/input.png" 2 clean; then exit 1; fi
if "$dme" "$tmp/input.png" "$tmp/invalid.png" 3 clean; then exit 1; fi
echo "Reproducibility and basic validation passed."
