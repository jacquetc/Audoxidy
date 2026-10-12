#!/usr/bin/env python3
"""Run every live probe, each against its own freshly started app (a fresh isolated home), and print a table.

Builds nothing: build the mocks binary first,
    cargo build --no-default-features --features teksilo-mocks --bin audoxidy-teksilo --target-dir target/mocks

    scripts/probes/run_all.py [--strict] [probe-name-prefix ...]

Exit code: 0 when no probe failed (known failures are reported as XFAIL and do not count), 1 otherwise.  With
--strict an XPASS (a known failure that now passes: the finding is fixed, remove its marker) also fails the run.
"""
import re
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import lib  # noqa: E402


def main(argv):
    strict = "--strict" in argv
    wanted = [a for a in argv if not a.startswith("--")]
    probes = sorted(p for p in HERE.glob("p[0-9][0-9]_*.py") if not wanted or any(p.stem.startswith(w) for w in wanted))
    for need, what in ((lib.APP_BIN, "the mocks binary (cargo build --no-default-features --features teksilo-mocks "
                                    "--bin audoxidy-teksilo --target-dir target/mocks)"), (Path(lib.MCP_BIN), "the 0.16.0 teksilo-automation-mcp (TEKSILO_MCP_BIN)")):
        if not Path(need).exists():
            print(f"missing {what}: {need}")
            return 1
    rows = []
    for probe in probes:
        t0 = time.time()
        proc = subprocess.run([sys.executable, str(probe)], capture_output=True, text=True, timeout=900)
        out = proc.stdout + proc.stderr
        counts = re.findall(r"-- \w+: (\d+) ok, (\d+) FAIL, (\d+) xfail, (\d+) XPASS", out)
        if counts:
            ok, fail, xfail, xpass = map(int, counts[-1])
        else:
            ok = fail = xfail = xpass = 0
        skipped = "skipped" in out.splitlines()[-1] if out.strip() else False
        status = "SKIP" if skipped else {0: "PASS", 2: "FAIL", 3: "XPASS", 1: "ERROR"}.get(proc.returncode, f"exit {proc.returncode}")
        if proc.returncode == 0 and xfail:
            status = "PASS*"
        rows.append((probe.stem, status, ok, fail, xfail, xpass, time.time() - t0))
        if proc.returncode not in (0, 3):
            print(out[-3000:])
        if proc.returncode == 3:
            print("\n".join(l for l in out.splitlines() if "XPASS" in l))
    print()
    print(f"{'probe':26} {'result':7} {'ok':>4} {'FAIL':>5} {'xfail':>6} {'XPASS':>6} {'time':>7}")
    for name, status, ok, fail, xfail, xpass, dt in rows:
        print(f"{name:26} {status:7} {ok:4} {fail:5} {xfail:6} {xpass:6} {dt:6.0f}s")
    print("PASS* = passes, with known failures (xfail) that stand for findings; see the README")
    bad = [r for r in rows if r[1] in ("FAIL", "ERROR") or r[1].startswith("exit")]
    if strict:
        bad += [r for r in rows if r[1] == "XPASS"]
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
