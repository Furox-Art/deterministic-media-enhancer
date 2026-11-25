#!/usr/bin/env bash
set -euo pipefail
dme="$1"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ffmpeg -v error -f lavfi -i 'nullsrc=s=256x192,geq=lum=128+6*cos(PI*X+PI*Y):cb=128:cr=128' -frames:v 1 -threads 1 "$tmp/noise.png"
ffmpeg -v error -f lavfi -i 'color=c=gray:s=128x96' -frames:v 1 -threads 1 "$tmp/flat.png"
"$dme" --analyze "$tmp/flat.png" 1 | grep -q '"selected":"baseline"'
"$dme" --analyze "$tmp/noise.png" 1 > "$tmp/report1"
"$dme" --analyze "$tmp/noise.png" 1 > "$tmp/report2"
cmp "$tmp/report1" "$tmp/report2"
grep -q '"selected":"bilateral"' "$tmp/report1"
"$dme" "$tmp/noise.png" "$tmp/a.png" 1 auto
"$dme" "$tmp/noise.png" "$tmp/b.png" 1 auto
cmp "$tmp/a.png" "$tmp/b.png"
before=$(sed -n 's/.*"noise_proxy_x100":\([0-9]*\),.*/\1/p' "$tmp/report1")
after=$("$dme" --analyze "$tmp/a.png" 1 | sed -n 's/.*"noise_proxy_x100":\([0-9]*\),.*/\1/p')
test "$after" -lt "$before"
echo "Noise proxy reduced from $before to $after; baseline retained for flat image."
