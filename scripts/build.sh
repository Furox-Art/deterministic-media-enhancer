#!/usr/bin/env bash
set -euo pipefail
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd -P)"
for tool in cargo rustc ffmpeg ffprobe python3; do
    command -v "$tool" >/dev/null || { printf 'Missing local dependency: %s\n' "$tool" >&2; exit 1; }
done
export LC_ALL=C TZ=UTC CARGO_NET_OFFLINE=true
export CARGO_TARGET_DIR="$root/build/rust"
cargo test --manifest-path "$root/Cargo.toml" --locked --offline
cargo build --manifest-path "$root/Cargo.toml" --release --locked --offline
python3 "$root/scripts/test.py" "$CARGO_TARGET_DIR/release/dme"
mkdir -p "$root/build/skill"
# Replace only the installed engine after tests succeed.
temp="$(mktemp "$root/build/skill/.dme-install-XXXXXX")"
trap 'rm -f "$temp"' EXIT
cp "$CARGO_TARGET_DIR/release/dme" "$temp"
chmod 755 "$temp"
mv -f "$temp" "$root/build/skill/dme"
python3 "$root/scripts/agent.py" doctor
