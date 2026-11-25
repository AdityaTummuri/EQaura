#!/usr/bin/env python3
"""Writes THIRD-PARTY-LICENSES.txt from the crates the apps really use, and
fails when a crate's license is not on the allowed list.

Usage: third_party.py [target] [--android]"""
import json
import pathlib
import re
import subprocess
import sys

ALLOWED = {
    "MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Zlib", "Unicode-3.0",
    "Unicode-DFS-2016", "MPL-2.0", "0BSD", "CC0-1.0", "Unlicense", "BSL-1.0", "MIT-0",
    "CDLA-Permissive-2.0", "LicenseRef-Slint-Royalty-free-2.0", "Apache-2.0 WITH LLVM-exception",
}


ANDROID = """Jetpack Compose and the AndroidX libraries, Copyright The Android Open Source Project,
and Kotlin, kotlinx.coroutines and kotlinx.serialization, Copyright JetBrains s.r.o.,
are used under the Apache License 2.0. https://www.apache.org/licenses/LICENSE-2.0
"""


def allowed(expr: str) -> bool:
    """SPDX check: every AND part needs one allowed OR choice."""
    expr = expr.replace("/", " OR ")
    tokens = re.findall(r"\(|\)|\bAND\b|\bOR\b|[\w.\-+]+(?: WITH [\w.\-]+)?", expr)
    pos = 0

    def atom() -> bool:
        nonlocal pos
        t = tokens[pos]
        pos += 1
        if t == "(":
            v = either()
            pos += 1
            return v
        return t in ALLOWED

    def both() -> bool:
        nonlocal pos
        v = atom()
        while pos < len(tokens) and tokens[pos] == "AND":
            pos += 1
            v = atom() and v
        return v

    def either() -> bool:
        nonlocal pos
        v = both()
        while pos < len(tokens) and tokens[pos] == "OR":
            pos += 1
            v = both() or v
        return v

    return either()


def main() -> None:
    root = pathlib.Path(__file__).parents[1]
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked"], cwd=root))
    packages = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    # Only what ends up in the programs: follow normal dependencies.
    android = "--android" in sys.argv
    roots = [m for m in meta["workspace_members"] if packages[m]["name"] == "eq-ffi"] if android \
        else [m for m in meta["workspace_members"] if packages[m]["name"] != "eq-ffi"]
    todo, used = list(roots), set()
    while todo:
        i = todo.pop()
        if i in used:
            continue
        used.add(i)
        for d in nodes[i]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    bad, groups = [], {}
    for i in sorted(used - set(meta["workspace_members"]), key=lambda i: packages[i]["name"]):
        p = packages[i]
        lic = p["license"] or "see license file"
        if not p["license"] or not allowed(lic):
            bad.append(f'{p["name"]} {p["version"]}: {lic}')
        folder = pathlib.Path(p["manifest_path"]).parent
        texts = []
        for f in sorted(folder.iterdir()):
            if f.is_file() and re.match(r"(?i)(licen[cs]e|copying|notice|copyright)", f.name):
                texts.append(f.read_text(encoding="utf-8", errors="replace").strip())
        # One permissive text is enough; prefer MIT, which carries the copyright line.
        texts.sort(key=lambda t: ("Permission is hereby granted" not in t, len(t)))
        text = texts[0] if texts else f"Licensed under {lic}."
        groups.setdefault(text, []).append(f'{p["name"]} {p["version"]} ({lic})')
    out = [
        "EQaura uses the open source software below. Thank you to everyone who made it.\n",
        ANDROID if android else "Slint is used under the Slint Royalty-free License 2.0. https://slint.dev\n",
        "AutoEq (headphone corrections)\n\n"
        + (root / "crates/eq-core/data/AUTOEQ-LICENSE").read_text().strip() + "\n",
    ]
    for text, names in sorted(groups.items(), key=lambda g: g[1][0]):
        out.append("-" * 72 + "\n" + "\n".join(names) + "\n\n" + text + "\n")
    paths = [a for a in sys.argv[1:] if not a.startswith("--")]
    target = pathlib.Path(paths[0]) if paths else root / "dist/THIRD-PARTY-LICENSES.txt"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("\n".join(out), encoding="utf-8")
    print(f"{sum(map(len, groups.values()))} crates listed in {target}")
    if bad:
        sys.exit("These licenses need a decision:\n" + "\n".join(bad))


main()
