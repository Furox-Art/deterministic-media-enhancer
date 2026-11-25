#!/usr/bin/env bash
set -euo pipefail
dme="$1"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
ffmpeg -v error -f lavfi -i testsrc2=size=128x96:rate=24 -f lavfi -i sine=frequency=440 -t 1 -threads 1 -c:v ffv1 -c:a flac "$tmp/input.mkv"
ffmpeg -v error -i "$tmp/input.mkv" -frames:v 1 -threads 1 "$tmp/input.png"
for preset in temporal stabilize; do
  "$dme" "$tmp/input.mkv" "$tmp/a.mkv" 1 "$preset" keep
  "$dme" "$tmp/input.mkv" "$tmp/b.mkv" 1 "$preset" keep
  cmp "$tmp/a.mkv" "$tmp/b.mkv"
  rm "$tmp/a.mkv" "$tmp/b.mkv"
done
for fps in 30 60 120; do
  "$dme" "$tmp/input.mkv" "$tmp/a.mkv" 1 resize "$fps" 2>"$tmp/report"
  "$dme" "$tmp/input.mkv" "$tmp/b.mkv" 1 resize "$fps"
  cmp "$tmp/a.mkv" "$tmp/b.mkv"
  grep -q "\"output_frames\":$fps," "$tmp/report"
  test "$(ffprobe -v error -select_streams a:0 -show_entries stream=codec_name -of csv=p=0 "$tmp/a.mkv")" = flac
  rm "$tmp/a.mkv" "$tmp/b.mkv"
done
"$dme" "$tmp/input.mkv" "$tmp/output.mp4" 2 resize 60
test "$(ffprobe -v error -select_streams v:0 -show_entries stream=width,height,avg_frame_rate -of csv=p=0 "$tmp/output.mp4")" = "256,192,60/1"
if "$dme" "$tmp/input.png" "$tmp/bad.png" 1 temporal; then exit 1; fi
if "$dme" "$tmp/input.png" "$tmp/bad.png" 1 resize 60; then exit 1; fi
test ! -e "$tmp/bad.png"
echo "Temporal, stabilization, FPS conversion, audio presence and deterministic video tests passed."
