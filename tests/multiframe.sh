#!/usr/bin/env bash
set -euo pipefail
dme="$1"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
python3 - "$tmp" <<'PY'
import math, pathlib, sys
root=pathlib.Path(sys.argv[1])
for i,(dx,dy) in enumerate([(0,0),(1,0),(0,1),(0,0),(1,0),(0,1)]):
    raw=bytearray()
    for y in range(80):
        for x in range(96):
            X,Y=2*x+dx,2*y+dy
            v=round(128+43*math.sin(X*.24)+30*math.cos(Y*.19)+17*math.sin((X+Y)*.45))
            raw.extend((v,v,v))
    (root/f"frame{i:02d}.ppm").write_bytes(b"P6\n96 80\n255\n"+raw)
PY
ffmpeg -v error -framerate 24 -i "$tmp/frame%02d.ppm" -f lavfi -i sine=frequency=440 -t 0.25 -threads 1 -c:v ffv1 -pix_fmt bgr0 -c:a flac "$tmp/input.mkv"
for ext in mkv mp4; do
    "$dme" "$tmp/input.mkv" "$tmp/a.$ext" 2 multiframe keep 2>"$tmp/report"
    "$dme" "$tmp/input.mkv" "$tmp/b.$ext" 2 multiframe keep
    cmp "$tmp/a.$ext" "$tmp/b.$ext"
    grep -Eq '"accepted_neighbors":[1-9][0-9]*' "$tmp/report"
    grep -Eq '"fused_pixels":[1-9][0-9]*' "$tmp/report"
    grep -q '"output_frames":6,' "$tmp/report"
    test "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height -of csv=p=0 "$tmp/a.$ext")" = "192,160"
    test -n "$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 "$tmp/a.$ext")"
done
if "$dme" "$tmp/input.mkv" "$tmp/bad.mkv" 4 multiframe keep; then exit 1; fi
if "$dme" "$tmp/input.mkv" "$tmp/bad.mkv" 2 multiframe 60; then exit 1; fi
test ! -e "$tmp/bad.mkv"
echo "Multiframe video integration passed."
