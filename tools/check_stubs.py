#!/usr/bin/env python3
"""Check the Iced-stub markers in the new module trees against docs/teksilo-port-stubs.md.

The Teksilo port keeps the Iced UI's stubs as stubs, marked so they stay
visible:

    // TODO(iced-stub): <what the Iced UI does instead>
    unimplemented!("<feature>: stub in the Iced UI")

These markers are the only TODO / FIXME / todo!() / unimplemented!() allowed in
src/logic, src/frontend and src/teksilo_ui. This script enforces three rules:

1. Every TODO, FIXME, todo!() or unimplemented!() in those trees is an Iced-stub
   marker.
2. Every file holding a marker is named in the "Teksilo site" column of
   docs/teksilo-port-stubs.md.
3. Every file named in that column exists and holds at least one marker.

With --final, every row of the stubs table must name its Teksilo site, which is
the state the finished port must reach.

Usage:
    python3 tools/check_stubs.py [--final]
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
TREES = ["src/logic", "src/frontend", "src/teksilo_ui"]
DOC = ROOT / "docs" / "teksilo-port-stubs.md"

STUB_TODO = re.compile(r"//\s*TODO\(iced-stub\):\s*\S")
STUB_UNIMPLEMENTED = re.compile(r'unimplemented!\(\s*"[^"]*: stub in the Iced UI"\s*\)')
FORBIDDEN = re.compile(r"\bTODO\b|\bFIXME\b|\btodo!\s*\(|\bunimplemented!\s*\(")
SITE_PATH = re.compile(r"src/[A-Za-z0-9_/.-]+\.rs")


def scan_sources() -> tuple[set[str], list[str]]:
    """Return the files holding a marker and the list of forbidden occurrences."""
    marker_files: set[str] = set()
    problems: list[str] = []
    for tree in TREES:
        base = ROOT / tree
        if not base.exists():
            continue
        for path in sorted(base.rglob("*.rs")):
            rel = path.relative_to(ROOT).as_posix()
            for lineno, line in enumerate(path.read_text().splitlines(), start=1):
                is_marker = bool(STUB_TODO.search(line) or STUB_UNIMPLEMENTED.search(line))
                if is_marker:
                    marker_files.add(rel)
                    continue
                if FORBIDDEN.search(line):
                    problems.append(f"{rel}:{lineno}: not an Iced-stub marker: {line.strip()}")
    return marker_files, problems


def read_doc_rows() -> list[tuple[str, str]]:
    """Return (feature, teksilo-site cell) for every data row of the stub tables."""
    rows: list[tuple[str, str]] = []
    for line in DOC.read_text().splitlines():
        if not line.startswith("|") or line.startswith("| ---") or line.startswith("| Feature"):
            continue
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) < 4:
            continue
        rows.append((cells[0], cells[-1]))
    return rows


def main(argv: list[str]) -> int:
    final = "--final" in argv
    marker_files, problems = scan_sources()
    rows = read_doc_rows()
    documented: set[str] = set()
    for feature, site in rows:
        paths = SITE_PATH.findall(site)
        if not paths and final:
            problems.append(f"{DOC.name}: row '{feature}' has no Teksilo site")
        for rel in paths:
            documented.add(rel)
            if not (ROOT / rel).exists():
                problems.append(f"{DOC.name}: row '{feature}' names {rel}, which does not exist")
            elif rel not in marker_files:
                problems.append(f"{DOC.name}: row '{feature}' names {rel}, which holds no Iced-stub marker")
    for rel in sorted(marker_files - documented):
        problems.append(f"{rel}: holds an Iced-stub marker but is not listed in {DOC.name}")
    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"stub markers ok: {len(marker_files)} file(s), {len(rows)} documented stub(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
