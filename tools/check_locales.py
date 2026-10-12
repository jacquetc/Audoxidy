#!/usr/bin/env python3
"""Check the Teksilo UI's Fluent catalogues for parity with each other and with the Iced UI.

The strings of the Teksilo UI live in `locales/<locale>/<file>.ftl`. en-US is
the source catalogue that `tr!` validates keys against; es-ES is the fallback
locale and the language of the Iced UI, so its messages port the Iced UI's
Spanish literals verbatim. This script enforces six rules:

1. Keys both ways: both locales have the same files (also the files that
   `compile_in_locales!` lists in `src/teksilo_ui/startup.rs`), and every
   message and attribute of one file exists in the same file of the other.
2. Every message key is referenced from `src/teksilo_ui` (with underscores,
   as `tr!` writes it, or with dashes). Unused keys fail the check.
3. Spanish is the Iced literal: an es-ES message with no placeable and no
   `# Not in the Iced UI: ...` line above it must occur as a string literal in
   `src/gui/*.rs`, after the quirk-fix-15 corrections are reversed (see
   docs/teksilo-port-quirk-fixes.md, fix 15). Messages with placeables or
   selectors are listed for a manual check.
4. (Informational) a message marked `# Not in the Iced UI` whose text is an Iced
   literal is listed as a doubtful marker; it does not fail the check.
5. (Not checked here) English is a faithful translation of the Spanish: that
   is a manual review.
6. Fluent syntax: no duplicate keys in a locale, every selector has exactly one
   default variant (`*[...]`), braces are balanced, and every line that is not
   a comment, a section header, a key or an indented continuation is an error.

Usage:
    python3 tools/check_locales.py
"""

from __future__ import annotations

import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
LOCALES_DIR = ROOT / "locales"
SOURCE_LOCALE = "en-US"
FALLBACK_LOCALE = "es-ES"
TEKSILO_SRC = ROOT / "src" / "teksilo_ui"
# The Iced UI's Spanish literals: the panels in src/gui, and the labels they
# share with the logic (src/logic, e.g. the channel count) and the utilities
# (src/utils, e.g. the SortColumn column headers).
ICED_SOURCES = [ROOT / "src" / "gui", ROOT / "src" / "logic", ROOT / "src" / "utils"]
STARTUP = TEKSILO_SRC / "startup.rs"

MARKER = "# Not in the Iced UI:"
MESSAGE = re.compile(r"^([A-Za-z][A-Za-z0-9_-]*)\s*=(.*)$")
ATTRIBUTE = re.compile(r"^\s+\.([A-Za-z][A-Za-z0-9_-]*)\s*=(.*)$")
TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_-]*")
RUST_TOKEN = re.compile(r'(//[^\n]*)|"((?:[^"\\]|\\.)*)"', re.S)
RUST_ESCAPES = {"n": "\n", "t": "\t", "r": "\r", "0": "\0", '"': '"', "'": "'", "\\": "\\"}

# Quirk fix 15 (docs/teksilo-port-quirk-fixes.md): Teksilo's spelling -> Iced's.
CORRECTIONS = [
    ("Automático", "Automatico"),
    ("canción", "cancion"),
    ("Canción", "Cancion"),
    ("Reproducción", "Reproduccion"),
    ("Personalización", "Personalizacion"),
    ("Lyrics", "Lirycs"),
    ("eliminarán", "elimarán"),
]


class Message:
    """One message or attribute of a catalogue, with the text of its value."""

    def __init__(self, file: str, name: str, text: str, marked: bool, line: int) -> None:
        self.file = file
        self.name = name
        self.text = text
        self.marked = marked
        self.line = line


def parse_ftl(path: pathlib.Path, errors: list[str]) -> dict[str, Message]:
    """Parse one Fluent file into its messages and attributes (keyed `key` or `key.attr`)."""
    messages: dict[str, Message] = {}
    lines = path.read_text(encoding="utf-8").splitlines()
    current: str | None = None
    pieces: dict[str, list[str]] = {}
    starts: dict[str, int] = {}
    marked: dict[str, bool] = {}
    pending_attr: str | None = None
    previous = ""

    def flush() -> None:
        for name, parts in pieces.items():
            messages[name] = Message(path.name, name, "\n".join(parts), marked[name], starts[name])
        pieces.clear()

    for lineno, raw in enumerate(lines, start=1):
        if not raw.strip():
            current = None
            pending_attr = None
            previous = raw
            continue
        if raw.lstrip().startswith("#") and not raw.startswith(" "):
            previous = raw
            current = None
            pending_attr = None
            continue
        if not raw[0].isspace():
            match = MESSAGE.match(raw)
            if not match:
                errors.append(f"{path.name}:{lineno}: not a message, comment or section: {raw.strip()}")
                previous = raw
                continue
            name = match.group(1)
            if name in pieces or name in messages:
                errors.append(f"{path.name}:{lineno}: duplicate key {name}")
            first = match.group(2).strip()
            pieces[name] = [first] if first else []
            starts[name] = lineno
            marked[name] = previous.startswith(MARKER)
            current = name
            pending_attr = None
        else:
            attr = ATTRIBUTE.match(raw)
            if attr and current is not None:
                pending_attr = f"{current}.{attr.group(1)}"
                if pending_attr in pieces:
                    errors.append(f"{path.name}:{lineno}: duplicate attribute {pending_attr}")
                first = attr.group(2).strip()
                pieces[pending_attr] = [first] if first else []
                starts[pending_attr] = lineno
                marked[pending_attr] = previous.startswith(MARKER)
            elif current is not None:
                target = pending_attr or current
                pieces[target].append(raw.strip())
            else:
                errors.append(f"{path.name}:{lineno}: continuation line outside a message: {raw.strip()}")
        previous = raw
    flush()
    return messages


def check_syntax(name: str, text: str) -> list[str]:
    """Check the braces and the selectors of one value (a Fluent pattern)."""
    errors: list[str] = []
    stack: list[dict[str, int | bool]] = []
    line_start = True
    pos = 0
    while pos < len(text):
        ch = text[pos]
        if ch == "\n":
            line_start = True
            pos += 1
            continue
        if line_start and ch in " \t":
            pos += 1
            continue
        if line_start and stack and stack[-1]["selector"]:
            if text.startswith("*[", pos):
                stack[-1]["defaults"] += 1  # type: ignore[operator]
                pos += 2
                line_start = False
                continue
            if ch == "[":
                pos += 1
                line_start = False
                continue
        line_start = False
        if ch == "{":
            stack.append({"selector": False, "defaults": 0})
        elif ch == "}":
            if not stack:
                errors.append(f"{name}: unbalanced '}}'")
            else:
                top = stack.pop()
                if top["selector"] and top["defaults"] != 1:
                    errors.append(f"{name}: selector has {top['defaults']} default variant(s), needs one '*[...]'")
        elif text.startswith("->", pos) and stack:
            stack[-1]["selector"] = True
            pos += 2
            continue
        pos += 1
    if stack:
        errors.append(f"{name}: unclosed '{{'")
    return errors


def iced_literals() -> set[str]:
    """Every string literal of the Iced UI's sources (comments skipped, escapes resolved)."""
    literals: set[str] = set()
    paths = [p for base in ICED_SOURCES for p in sorted(base.glob("*.rs"))]
    for path in paths:
        for match in RUST_TOKEN.finditer(path.read_text(encoding="utf-8")):
            if match.group(1) is not None:
                continue
            literals.add(unescape_rust(match.group(2)))
    return literals


def unescape_rust(body: str) -> str:
    """Resolve the escapes of a Rust string literal body."""
    out: list[str] = []
    i = 0
    while i < len(body):
        ch = body[i]
        if ch == "\\" and i + 1 < len(body):
            nxt = body[i + 1]
            if nxt == "\n":
                i += 2
                while i < len(body) and body[i] in " \t\n":
                    i += 1
                continue
            out.append(RUST_ESCAPES.get(nxt, "\\" + nxt))
            i += 2
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def reverse_corrections(text: str) -> str:
    """Undo the quirk-fix-15 spelling corrections, giving the Iced literal."""
    for teksilo, iced in CORRECTIONS:
        text = text.replace(teksilo, iced)
    return text


def used_identifiers() -> set[str]:
    """Every identifier-like token in the code of src/teksilo_ui (comment lines skipped)."""
    tokens: set[str] = set()
    for path in sorted(TEKSILO_SRC.rglob("*.rs")):
        for line in path.read_text(encoding="utf-8").splitlines():
            code = line.split("//", 1)[0]
            if line.lstrip().startswith("//"):
                continue
            tokens.update(TOKEN.findall(code))
    return tokens


def startup_files() -> set[str]:
    """The file names that `compile_in_locales!` lists in startup.rs."""
    text = STARTUP.read_text(encoding="utf-8")
    block = re.search(r"compile_in_locales!\((.*?)\n\s*\)", text, re.S)
    if not block:
        return set()
    listed = re.search(r"files\s*=\s*\[(.*?)\]", block.group(1), re.S)
    return set(re.findall(r'"([^"]+\.ftl)"', listed.group(1))) if listed else set()


def main() -> int:
    errors: list[str] = []
    locales = [SOURCE_LOCALE, FALLBACK_LOCALE]
    files = {loc: {p.name for p in (LOCALES_DIR / loc).glob("*.ftl")} for loc in locales}

    # Rule 1: the same file set, and the file set the build compiles in.
    all_files = sorted(files[SOURCE_LOCALE] | files[FALLBACK_LOCALE])
    for name in all_files:
        for loc in locales:
            if name not in files[loc]:
                errors.append(f"{loc}/{name}: missing (the other locale has it)")
    compiled = startup_files()
    for name in sorted(compiled ^ set(all_files)):
        errors.append(f"{name}: {'listed in startup.rs but not on disk' if name in compiled else 'on disk but not listed in startup.rs'}")

    # Parse both locales, file by file.
    parsed: dict[str, dict[str, dict[str, Message]]] = {loc: {} for loc in locales}
    for loc in locales:
        for name in all_files:
            path = LOCALES_DIR / loc / name
            if path.exists():
                parsed[loc][name] = parse_ftl(path, errors)

    # Rule 1 (continued): the same keys and attributes in each file of both locales.
    for name in all_files:
        source = parsed[SOURCE_LOCALE].get(name, {})
        fallback = parsed[FALLBACK_LOCALE].get(name, {})
        for key in sorted(set(source) - set(fallback)):
            errors.append(f"{name}: {key} is in {SOURCE_LOCALE} but not in {FALLBACK_LOCALE}")
        for key in sorted(set(fallback) - set(source)):
            errors.append(f"{name}: {key} is in {FALLBACK_LOCALE} but not in {SOURCE_LOCALE}")

    # Rule 6: Fluent syntax, per message and attribute.
    for loc in locales:
        for name, messages in parsed[loc].items():
            for key, message in messages.items():
                for problem in check_syntax(f"{loc}/{name}:{message.line} {key}", message.text):
                    errors.append(problem)

    # Rule 2: every message key is used by src/teksilo_ui.
    tokens = used_identifiers()
    unused: list[str] = []
    message_count = 0
    for name in all_files:
        for key in parsed[SOURCE_LOCALE].get(name, {}):
            if "." in key:
                continue
            message_count += 1
            if key not in tokens and key.replace("-", "_") not in tokens:
                unused.append(f"{name}: {key}")
    for item in unused:
        errors.append(f"unused key {item}")

    # Rule 3: es-ES messages without placeables are Iced literals.
    literals = iced_literals()
    by_hand: list[str] = []
    not_found: list[str] = []
    corrected: list[str] = []
    checked = 0
    for name in all_files:
        for key, message in parsed[FALLBACK_LOCALE].get(name, {}).items():
            if "{" in message.text or "->" in message.text:
                by_hand.append(f"{name}: {key}")
                continue
            if message.marked:
                if reverse_corrections(message.text) in literals or message.text in literals:
                    print(f"doubtful marker (text is an Iced literal): {name}: {key}: {message.text}")
                continue
            checked += 1
            if message.text in literals:
                continue
            iced_text = reverse_corrections(message.text)
            if iced_text in literals:
                corrected.append(f"{name}: {key}: Iced {iced_text!r}, es-ES {message.text!r}")
                continue
            not_found.append(f"{name}: {key}: {message.text!r}")
    if corrected:
        print(f"quirk-fix-15 corrections (Iced literal differs only by a fix-15 spelling): {len(corrected)}")
        for item in corrected:
            print(f"  {item}")
    for item in not_found:
        errors.append(f"not an Iced literal {item}")

    # A message in en-US that uses a placeable must do so in es-ES too (same variables).
    for name in all_files:
        for key, message in parsed[SOURCE_LOCALE].get(name, {}).items():
            twin = parsed[FALLBACK_LOCALE].get(name, {}).get(key)
            if twin is None:
                continue
            if set(re.findall(r"\$([A-Za-z_][A-Za-z0-9_]*)", message.text)) != set(
                re.findall(r"\$([A-Za-z_][A-Za-z0-9_]*)", twin.text)
            ):
                errors.append(f"{name}: {key}: the variables differ between {SOURCE_LOCALE} and {FALLBACK_LOCALE}")

    if by_hand:
        print(f"checked by hand (placeables or selectors): {len(by_hand)} message(s)")
        for item in by_hand:
            print(f"  {item}")
    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(f"locales failed: {len(errors)} problem(s), {checked} Iced literal(s) checked", file=sys.stderr)
        return 1
    print(f"checked against the Iced UI: {checked} message(s)")
    print(f"locales ok: {message_count} messages in {len(all_files)} files")
    return 0


if __name__ == "__main__":
    sys.exit(main())
