#!/usr/bin/env python3
"""Check docs/teksilo-port-parity.md against the Iced UI and the Teksilo sources.

The parity document maps every behaviour of the Iced UI to the Teksilo code that
provides it and to the test (or probe) that checks it. Its tables share the
columns `Iced | Behaviour | Teksilo | Check | Status`. This script fails when:

1. a variant of the Iced `Message` (src/gui/app.rs) or `AudioCenterMessage`
   (src/gui/audio_center.rs) has no row: the Iced cell of some row must name it
   as `Message::Variant` or `AudioCenterMessage::Variant`;
2. a test named in a Check cell does not exist: every backticked
   `path::to::module::tests::name` must resolve to a source file under src/
   holding `fn name(` (`path::tests::{a, b}` names two tests);
3. a `fixed #N` status names a quirk number with no `## N.` entry in
   docs/teksilo-port-quirk-fixes.md;
4. a Status is not one of: ported, fixed #N, stub, dead, deviation, pending, GAP;
5. a `pending` row names a test (pending rows are filled at merge, their Check
   is a dash), or a row has an empty cell;
6. an `APP_SETTINGS` key of src/frontend/settings.rs (`pub mod keys`) is missing
   from the "Persistence keys" section.

It prints `parity ok: N rows, G gaps, P pending` when clean. GAP and pending
rows do not fail it; they are counted.

Usage:
    python3 tools/check_parity.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOC = ROOT / "docs" / "teksilo-port-parity.md"
QUIRKS = ROOT / "docs" / "teksilo-port-quirk-fixes.md"
SRC = ROOT / "src"
MESSAGE_FILE = SRC / "gui" / "app.rs"
AUDIO_CENTER_FILE = SRC / "gui" / "audio_center.rs"
SETTINGS_FILE = SRC / "frontend" / "settings.rs"

HEADER = ["Iced", "Behaviour", "Teksilo", "Check", "Status"]
STATUS = re.compile(r"^(ported|stub|dead|deviation|pending|GAP|fixed #\d+(?:, #\d+)*)$")
CELL_SPLIT = re.compile(r"(?<!\\)\|")
PERSISTENCE_HEADING = "Persistence keys"


def enum_variants(path: pathlib.Path, name: str) -> list[str]:
    """Return the variant names of `pub enum <name>` in `path`."""
    text = path.read_text()
    match = re.search(r"pub enum " + name + r" \{(.*?)\n\}\n", text, re.S)
    if not match:
        raise SystemExit(f"check_parity: enum {name} not found in {path}")
    return re.findall(r"^ {4}([A-Z]\w*)", match.group(1), re.M)


def quirk_numbers() -> set[int]:
    """Return the numbers of the `## N.` entries of the quirk-fix document."""
    return {int(n) for n in re.findall(r"^## (\d+)\.", QUIRKS.read_text(), re.M)}


def settings_keys() -> list[str]:
    """Return the key strings of `pub mod keys` in the facade's settings."""
    text = SETTINGS_FILE.read_text()
    match = re.search(r"pub mod keys \{(.*?)\n\}\n", text, re.S)
    if not match:
        raise SystemExit(f"check_parity: pub mod keys not found in {SETTINGS_FILE}")
    return re.findall(r'pub const [A-Z0-9_]+: &str = "([a-z0-9_]+)";', match.group(1))


class Row:
    """One data row of a parity table."""

    def __init__(self, section: str, lineno: int, cells: list[str]) -> None:
        self.section = section
        self.lineno = lineno
        self.iced, self.behaviour, self.teksilo, self.check, self.status = cells


def read_rows() -> tuple[list[Row], dict[str, str]]:
    """Return the rows of every parity table and the text of each section."""
    rows: list[Row] = []
    sections: dict[str, list[str]] = {}
    section = ""
    in_parity_table = False
    for lineno, line in enumerate(DOC.read_text().splitlines(), start=1):
        if line.startswith("## "):
            section = line[3:].strip()
            in_parity_table = False
            sections.setdefault(section, [])
            continue
        sections.setdefault(section, []).append(line)
        if not line.startswith("|"):
            in_parity_table = False
            continue
        cells = [c.strip() for c in CELL_SPLIT.split(line.strip().strip("|"))]
        if cells == HEADER:
            in_parity_table = True
            continue
        if not in_parity_table or set("".join(cells)) <= set("-: "):
            continue
        if len(cells) != len(HEADER):
            raise SystemExit(f"{DOC.name}:{lineno}: {len(cells)} cells, expected {len(HEADER)}")
        rows.append(Row(section, lineno, cells))
    return rows, {name: "\n".join(lines) for name, lines in sections.items()}


def expand_tests(cell: str) -> list[str]:
    """Return the test paths named in a Check cell (backticked, braces expanded)."""
    tests: list[str] = []
    for span in re.findall(r"`([^`]+)`", cell):
        if "::" not in span:
            continue
        brace = re.match(r"^(.*::)\{(.*)\}$", span)
        if brace:
            tests.extend(brace.group(1) + part.strip() for part in brace.group(2).split(","))
        else:
            tests.append(span.strip())
    return tests


_FILE_CACHE: dict[pathlib.Path, str] = {}


def source(path: pathlib.Path) -> str:
    """Return the text of `path`, cached."""
    if path not in _FILE_CACHE:
        _FILE_CACHE[path] = path.read_text() if path.exists() else ""
    return _FILE_CACHE[path]


def test_exists(path: str) -> str | None:
    """Return an error message when the test `path` does not exist, else None."""
    segments = path.split("::")
    if len(segments) < 2:
        return "not a module path"
    name, module = segments[-1], segments[:-1]
    if not re.fullmatch(r"\w+", name):
        return "not a function name"
    # `a::b::tests` is `a/b/tests.rs` or an inline `mod tests` of `a/b.rs`
    # (or `a/b/mod.rs`); a module `a::b` is `a/b.rs` or `a/b/mod.rs`.
    base = SRC.joinpath(*module)
    candidates = [base.with_suffix(".rs"), base / "mod.rs"]
    if len(module) > 1:
        # an inline `mod tests` (or `mod live_tests`) of the parent module
        parent = SRC.joinpath(*module[:-1])
        candidates += [parent.with_suffix(".rs"), parent / "mod.rs"]
    existing = [c for c in candidates if c.exists()]
    if not existing:
        return "module path matches no source file"
    needle = re.compile(r"\bfn " + re.escape(name) + r"\(")
    if not any(needle.search(source(c)) for c in existing):
        return "no `fn " + name + "(` in the module's source"
    return None


def main() -> int:
    rows, sections = read_rows()
    problems: list[str] = []
    quirks = quirk_numbers()

    iced_text = "\n".join(row.iced for row in rows)
    for variant in enum_variants(MESSAGE_FILE, "Message"):
        if not re.search(r"(?<![A-Za-z])Message::" + variant + r"\b", iced_text):
            problems.append(f"{DOC.name}: Message::{variant} has no row")
    for variant in enum_variants(AUDIO_CENTER_FILE, "AudioCenterMessage"):
        if not re.search(r"AudioCenterMessage::" + variant + r"\b", iced_text):
            problems.append(f"{DOC.name}: AudioCenterMessage::{variant} has no row")

    gaps = pending = 0
    for row in rows:
        where = f"{DOC.name}:{row.lineno}"
        for column, cell in zip(HEADER, [row.iced, row.behaviour, row.teksilo, row.check, row.status]):
            if not cell:
                problems.append(f"{where}: empty {column} cell")
        if not STATUS.match(row.status):
            problems.append(f"{where}: status '{row.status}' is not one of the allowed values")
            continue
        if row.status == "GAP":
            gaps += 1
        if row.status == "pending":
            pending += 1
            if expand_tests(row.check):
                problems.append(f"{where}: a pending row names a test; its Check is a dash")
        for number in re.findall(r"#(\d+)", row.status):
            if int(number) not in quirks:
                problems.append(f"{where}: fixed #{number} has no entry in {QUIRKS.name}")
        for test in expand_tests(row.check):
            error = test_exists(test)
            if error:
                problems.append(f"{where}: test `{test}`: {error}")

    persistence = sections.get(PERSISTENCE_HEADING, "")
    for key in settings_keys():
        if not re.search(r"(?<![a-z0-9_])" + re.escape(key) + r"(?![a-z0-9_])", persistence):
            problems.append(f"{DOC.name}: settings key `{key}` is missing from '{PERSISTENCE_HEADING}'")

    if problems:
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"parity ok: {len(rows)} rows, {gaps} gaps, {pending} pending")
    return 0


if __name__ == "__main__":
    sys.exit(main())
