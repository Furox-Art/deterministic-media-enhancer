#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
command -v nvcc >/dev/null || { echo "CUDA compiler not available" >&2; exit 1; }
mkdir -p "$root/build/rust/release" "$root/build/skill"
arch="${DME_CUDA_ARCH:-sm_89}"
[[ "$arch" =~ ^sm_[0-9]+$ ]] || { echo "Invalid CUDA architecture" >&2; exit 1; }
temp="$(mktemp "$root/build/rust/release/.cuda-XXXXXX")"
trap 'rm -f "$temp"' EXIT
nvcc -O3 -arch="$arch" "$root/gpu/cubic.cu" -o "$temp"
mv -f "$temp" "$root/build/rust/release/dme-cuda"
python3 "$root/tests/advanced.py" "$root/scripts/agent.py" "$root/build/rust/release/dme" --gpu
install -m 755 "$root/build/rust/release/dme-cuda" "$root/build/skill/dme-cuda"
