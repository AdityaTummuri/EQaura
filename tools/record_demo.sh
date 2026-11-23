#!/usr/bin/env bash
# Rebuilds the README pictures. Needs ImageMagick.
set -euo pipefail
cd "$(dirname "$0")/.."
work=$(mktemp -d)
cargo run --release -q -p eqaura --example demo -- "$work"
mkdir -p docs/media
for clip in equalizer assistant headphones; do
  magick -delay 5 -loop 0 "$work/$clip"/*.png -resize 900x -dither None -colors 96 -layers OptimizePlus "docs/media/$clip.gif"
done
for f in "$work"/stills/*.png; do
  magick "$f" -resize 1770x -dither None -colors 255 -define png:compression-level=9 "docs/media/$(basename "$f")"
done
rm -rf "$work"
ls -l docs/media
