#!/usr/bin/env python3
"""Packs AutoEq's parametric presets into crates/eq-core/data/autoeq.bin.

Usage:
  git clone --filter=blob:none --no-checkout --depth 1 https://github.com/jaakkopasanen/AutoEq
  cd AutoEq && git sparse-checkout set --no-cone '*ParametricEQ.txt' && git checkout
  python3 tools/pack_autoeq.py path/to/AutoEq
"""
import pathlib
import re
import sys
import zlib

KINDS = {"PK": "P", "LSC": "L", "HSC": "H", "LS": "L", "HS": "H"}
LINE = re.compile(r"Filter \d+: ON (\w+) Fc ([\d.]+) Hz Gain (-?[\d.]+) dB Q ([\d.]+)")


def main() -> None:
    repo = pathlib.Path(sys.argv[1])
    results = repo / "results"
    rows = []
    for path in results.rglob("*ParametricEQ.txt"):
        rel = path.relative_to(results).parts  # source / form / name / file
        if len(rel) < 4:
            print("skipped", rel)
            continue
        source, rig, name = rel[0], rel[1], rel[-2]
        form = rig.split()[-1]
        rig = rig[: -len(form)].strip()
        filters = []
        for line in path.read_text(encoding="utf-8").splitlines():
            if m := LINE.match(line):
                kind, freq, gain, q = m.groups()
                if float(gain) != 0:
                    filters.append(f"{KINDS[kind]},{float(freq):g},{float(gain):g},{float(q):g}")
        if filters:
            label = f"{source} ({rig})" if rig else source
            rows.append((name.replace("\t", " "), label, form, ";".join(filters)))
    rows.sort(key=lambda r: (r[0].lower(), r[1]))
    text = "\n".join("\t".join(r) for r in rows)
    out = pathlib.Path(__file__).parents[1] / "crates/eq-core/data/autoeq.bin"
    out.write_bytes(zlib.compress(text.encode(), 9))
    print(f"{len(rows)} presets, {len(text):,} bytes packed to {out.stat().st_size:,}")


main()
