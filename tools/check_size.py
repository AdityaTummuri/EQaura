#!/usr/bin/env python3
"""Fails when a download is larger than promised."""
import os
import sys

# In millions of bytes. The macOS image holds two processors' code.
LIMITS = {".exe": 20, ".dmg": 16, ".deb": 10, ".rpm": 10, ".xz": 10, ".apk": 10}

bad = False
for path in sys.argv[1:]:
    limit = LIMITS.get(os.path.splitext(path)[1])
    if limit is None:
        continue
    size = os.path.getsize(path) / 1e6
    over = size > limit
    bad |= over
    print(f"{'TOO LARGE ' if over else ''}{os.path.basename(path)}: {size:.1f} MB of {limit} MB")
sys.exit(bad)
