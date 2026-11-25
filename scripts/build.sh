#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
for tool in cmake c++ ffmpeg ffprobe python3; do
    command -v "$tool" >/dev/null || { printf 'Missing local dependency: %s\n' "$tool" >&2; exit 1; }
done
export LC_ALL=C TZ=UTC
cmake -S "$root" -B "$root/build/skill" -DCMAKE_BUILD_TYPE=Release
cmake --build "$root/build/skill" --parallel 2
ctest --test-dir "$root/build/skill" --output-on-failure -j2
python3 "$root/scripts/agent.py" doctor
