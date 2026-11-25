#!/usr/bin/env bash
set -euo pipefail
dme="$1"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ffmpeg -v error -f lavfi -i testsrc2=size=160x120:rate=5 -frames:v 1 -threads 1 "$tmp/input.png"
ffmpeg -v error -f lavfi -i testsrc2=size=160x120:rate=5 -t 1 -threads 1 -c:v ffv1 "$tmp/input.mkv"
for ext in png mkv; do
  for preset in regional deblur wavelet patch bm3d; do
    "$dme" "$tmp/input.$ext" "$tmp/a.$ext" 2 "$preset"
    "$dme" "$tmp/input.$ext" "$tmp/b.$ext" 2 "$preset"
    cmp "$tmp/a.$ext" "$tmp/b.$ext"
    test "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$tmp/a.$ext")" = "320,240"
    rm "$tmp/a.$ext" "$tmp/b.$ext"
  done
done
"$dme" "$tmp/input.png" "$tmp/auto.png" 2 auto 2>"$tmp/report"
grep -q '"selected":' "$tmp/report"
sha256sum "$tmp/auto.png" > "$tmp/before"
if "$dme" "$tmp/input.png" "$tmp/auto.png" 2 resize; then exit 1; fi
sha256sum -c "$tmp/before"
printf 'not a valid image' > "$tmp/bad.png"
if "$dme" "$tmp/bad.png" "$tmp/failed.png" 2 resize; then exit 1; fi
test ! -e "$tmp/failed.png"
test -z "$(find "$tmp" -maxdepth 1 -type d -name '.dme-stage-*' -print)"
echo "New presets reproduce; output staging prevents incomplete publication."
