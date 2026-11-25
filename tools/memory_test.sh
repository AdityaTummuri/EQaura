#!/usr/bin/env bash
# Opens the real window on a virtual screen and checks how much memory it needs.
set -euo pipefail
binary="${1:-target/release/eqaura}"
limit_kb=$((60 * 1024))
export EQAURA_DEMO=1 EQAURA_HOME
EQAURA_HOME=$(mktemp -d)
Xvfb :97 -screen 0 1280x900x24 >/dev/null 2>&1 &
trap 'kill $(jobs -p) 2>/dev/null || true' EXIT
sleep 2
DISPLAY=:97 WAYLAND_DISPLAY= "$binary" &
pid=$!
sleep 8
kill -0 "$pid" || { echo "The app did not stay open"; exit 1; }
# Pss: shared libraries count only for this program's share.
kb=$(awk '/^Pss:/ {print $2}' "/proc/$pid/smaps_rollup")
echo "Window open: $((kb / 1024)) MB of 60 MB"
[ "$kb" -le "$limit_kb" ]
