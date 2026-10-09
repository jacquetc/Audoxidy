#!/usr/bin/env python3
"""Clippy gate that freezes pre-existing warnings and rejects new ones.

The code that predates the Teksilo port carries a few hundred clippy findings
under the current toolchain. They are not fixed as part of the port. Instead
this script records them per feature set in `tools/clippy-baseline/<set>.txt`
(count, file, lint) and fails when a run produces a finding that is not in the
baseline, or more of one than the baseline allows. The new module trees
(`src/logic`, `src/frontend`, `src/teksilo_ui`) deny warnings outright, so any
finding there fails the build before this comparison runs.

Usage:
    python3 tools/clippy_gate.py <set>            # check against the baseline
    python3 tools/clippy_gate.py <set> --update   # rewrite the baseline
    python3 tools/clippy_gate.py all              # check every set

Sets: default, teksilo-ui, teksilo-mocks.
"""

from __future__ import annotations

import collections
import json
import os
import pathlib
import subprocess
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
BASELINE_DIR = ROOT / "tools" / "clippy-baseline"

# The mocks build gets its own target directory so its binary never replaces
# the real one. With CARGO_TARGET_DIR set (worktrees sharing one target), it
# goes under that directory instead of under the checkout.
MOCKS_TARGET_DIR = (
    str(pathlib.Path(os.environ["CARGO_TARGET_DIR"]) / "mocks")
    if os.environ.get("CARGO_TARGET_DIR")
    else "target/mocks"
)

SETS: dict[str, list[str]] = {
    "default": [],
    "teksilo-ui": ["--no-default-features", "--features", "teksilo-ui"],
    "teksilo-mocks": [
        "--no-default-features",
        "--features",
        "teksilo-mocks",
        "--target-dir",
        MOCKS_TARGET_DIR,
    ],
}

Key = tuple[str, str]


def run_clippy(set_name: str) -> tuple[collections.Counter[Key], int, list[str]]:
    """Run clippy for one feature set; return finding counts, exit code, rendered errors."""
    cmd = [
        "cargo",
        "clippy",
        "--all-targets",
        "--message-format=json",
        *SETS[set_name],
    ]
    proc = subprocess.run(cmd, cwd=ROOT, capture_output=True, text=True)
    findings: collections.Counter[Key] = collections.Counter()
    rendered_errors: list[str] = []
    for line in proc.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        if msg.get("reason") != "compiler-message":
            continue
        diag = msg["message"]
        level = diag.get("level")
        if level not in ("warning", "error"):
            continue
        spans = [s for s in diag.get("spans", []) if s.get("is_primary")]
        if not spans:
            # Summary lines such as "N warnings emitted" have no span.
            continue
        file_name = spans[0]["file_name"]
        if file_name.startswith("/"):
            # A finding inside a dependency: not ours to gate.
            continue
        code = (diag.get("code") or {}).get("code") or diag.get("message", "")
        if level == "error":
            rendered_errors.append(diag.get("rendered") or diag.get("message", ""))
        findings[(file_name, code)] += 1
    return findings, proc.returncode, rendered_errors


def baseline_path(set_name: str) -> pathlib.Path:
    return BASELINE_DIR / f"{set_name}.txt"


def read_baseline(set_name: str) -> collections.Counter[Key]:
    path = baseline_path(set_name)
    counts: collections.Counter[Key] = collections.Counter()
    if not path.exists():
        return counts
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        count, file_name, code = line.split("\t", 2)
        counts[(file_name, code)] = int(count)
    return counts


def write_baseline(set_name: str, findings: collections.Counter[Key]) -> None:
    BASELINE_DIR.mkdir(parents=True, exist_ok=True)
    lines = [
        f"# Pre-existing clippy findings for feature set `{set_name}`.",
        "# Regenerate with: python3 tools/clippy_gate.py " + set_name + " --update",
        "# Format: count<TAB>file<TAB>lint",
    ]
    for (file_name, code), count in sorted(findings.items()):
        lines.append(f"{count}\t{file_name}\t{code}")
    baseline_path(set_name).write_text("\n".join(lines) + "\n")


def check(set_name: str, update: bool) -> bool:
    findings, returncode, rendered_errors = run_clippy(set_name)
    if rendered_errors or returncode != 0:
        # With no `-D warnings` on the command line, an error is either a compile
        # failure or a lint denied inside the new module trees. Both fail the gate.
        print(f"[{set_name}] clippy reported errors:", file=sys.stderr)
        for text in rendered_errors:
            print(text, file=sys.stderr)
        if returncode != 0 and not rendered_errors:
            print(f"[{set_name}] cargo exited with {returncode}", file=sys.stderr)
        return False
    if update:
        write_baseline(set_name, findings)
        print(f"[{set_name}] baseline written: {sum(findings.values())} findings")
        return True
    baseline = read_baseline(set_name)
    regressions = []
    for key, count in sorted(findings.items()):
        allowed = baseline.get(key, 0)
        if count > allowed:
            regressions.append((key, count, allowed))
    if regressions:
        print(f"[{set_name}] new clippy findings (not in the baseline):", file=sys.stderr)
        for (file_name, code), count, allowed in regressions:
            print(f"  {file_name}  {code}: {count} (baseline {allowed})", file=sys.stderr)
        return False
    improved = sum(baseline.values()) - sum(findings.values())
    note = f", {improved} fewer than the baseline (run --update)" if improved > 0 else ""
    print(f"[{set_name}] ok: {sum(findings.values())} pre-existing findings{note}")
    return True


def main(argv: list[str]) -> int:
    if not argv or argv[0] in ("-h", "--help"):
        print(__doc__)
        return 0
    target = argv[0]
    update = "--update" in argv[1:]
    names = list(SETS) if target == "all" else [target]
    unknown = [n for n in names if n not in SETS]
    if unknown:
        print(f"unknown set(s): {', '.join(unknown)}; expected one of {', '.join(SETS)}", file=sys.stderr)
        return 2
    ok = all([check(name, update) for name in names])
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
