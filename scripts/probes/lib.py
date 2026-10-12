"""Shared plumbing of the Audoxidy live probes (Wave F).

Every probe starts one fresh app of the mocks binary with an isolated home, drives it through the
automation bridge (`scripts/teksilo_probe`), asserts, and exits 0 (pass), 1 (the probe could not
run) or 2 (a check failed).  A check that is known to fail because of an app finding is declared
with `Probe.known_failure(...)`: it is reported as XFAIL and does not fail the probe, and it turns
into an XPASS (printed loudly, exit 3 under `--strict`) when the app is fixed.
"""
from __future__ import annotations

import base64
import hashlib
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
sys.path.insert(0, str(ROOT / "scripts"))

from teksilo_probe import bridge as _bridge  # noqa: E402
from teksilo_probe import connect, tree  # noqa: E402
from teksilo_probe.session import ToolError  # noqa: E402

APP_BIN = Path(os.environ.get("AUDOXIDY_PROBE_APP", ROOT / "target/mocks/debug/audoxidy-teksilo"))
#: The 0.16.0 build of the automation client (the one on $PATH is 0.15.0).
MCP_BIN = os.environ.get(
    "TEKSILO_MCP_BIN",
    "/tmp/claude-1000/-home-cyril-Devel-Audoxidy/6fcde8bc-3ed3-4e78-8346-b9a6b289f290/scratchpad/mcp/bin/teksilo-automation-mcp",
)
OUT_ROOT = Path(os.environ.get("AUDOXIDY_PROBE_OUT", Path(tempfile.gettempdir()) / "audoxidy-probes"))
#: Spanish is the shipped fallback; fixing it makes the strings deterministic.
LANG = os.environ.get("AUDOXIDY_PROBE_LANG", "es_ES.UTF-8")
SETTLE = {"layout_after": True}


class Failure(Exception):
    pass


class Probe:
    """A probe: checks, known failures, a final verdict."""

    def __init__(self, name: str):
        self.name = name
        self.ok: list[str] = []
        self.fail: list[str] = []
        self.xfail: list[str] = []
        self.xpass: list[str] = []
        self.notes: list[str] = []
        self.out = OUT_ROOT / name
        shutil.rmtree(self.out, ignore_errors=True)
        self.out.mkdir(parents=True, exist_ok=True)
        print(f"== {name} == (artifacts: {self.out})", flush=True)

    def check(self, cond, msg: str, *, known_failure: str | None = None) -> bool:
        cond = bool(cond)
        if known_failure:
            if cond:
                self.xpass.append(msg)
                print(f"  XPASS {msg}   <- finding fixed? ({known_failure})", flush=True)
            else:
                self.xfail.append(msg)
                print(f"  XFAIL {msg}   [finding: {known_failure}]", flush=True)
            return cond
        (self.ok if cond else self.fail).append(msg)
        print(("  ok    " if cond else "  FAIL  ") + msg, flush=True)
        return cond

    def note(self, msg: str):
        self.notes.append(msg)
        print("  note  " + msg, flush=True)

    def finish(self) -> int:
        print(f"-- {self.name}: {len(self.ok)} ok, {len(self.fail)} FAIL, {len(self.xfail)} xfail, {len(self.xpass)} XPASS", flush=True)
        if self.fail:
            return 2
        if self.xpass:
            return 3
        return 0


class App:
    """One app instance of ours, started with an isolated home."""

    def __init__(self, probe: Probe, home: Path | None = None, window_state: str | None = None):
        self.probe = probe
        self.home = home or Path(tempfile.mkdtemp(prefix="audoxidy-home-"))
        self.state_dir = self.home / ".config/audoxidy/teksilo-mocks"
        if window_state is not None:
            self.state_dir.mkdir(parents=True, exist_ok=True)
            (self.state_dir / "window_state.toml").write_text(window_state)
        self.proc = None
        self.session = None
        self.log = None
        self._shots = 0
        self.hazards = {}
        self.start()

    def start(self):
        env = {k: v for k, v in os.environ.items() if not k.startswith("LC_") and k not in ("LANGUAGE",)}
        env.update(HOME=str(self.home), LANG=LANG, XDG_CONFIG_HOME=str(self.home / ".config"),
                   XDG_DATA_HOME=str(self.home / ".local/share"),
                   XDG_STATE_HOME=str(self.home / ".local/state"))
        for d in (".config", ".local/share", ".local/state"):
            (self.home / d).mkdir(parents=True, exist_ok=True)
        la = _bridge.launch([str(APP_BIN)], env=env, label=self.probe.name, log=str(self.probe.out / f"app{self._shots}.log"))
        self.proc, self.log = la.proc, la.log
        self.launched = la
        try:
            found = _bridge.wait_for_bridge(la.proc.pid, timeout=90, log=la.log, proc=la.proc, token=la.token)
            self.session = connect(found, mcp=MCP_BIN)
        except BaseException:
            self.kill()
            raise
        self.wait_ready()

    def wait_ready(self, timeout=60):
        end = time.time() + timeout
        while time.time() < end:
            try:
                if self.find(role="Button", label="Menú"):
                    break
            except Exception:
                pass
            time.sleep(0.3)
        self.settle(1.0)

    # --- lifecycle
    @property
    def pid(self):
        return self.proc.pid

    def alive(self):
        return self.proc.poll() is None

    def cleanup(self):
        """Stop the app (by its pid) and delete the isolated home."""
        self.kill()
        shutil.rmtree(self.home, ignore_errors=True)

    def kill(self):
        try:
            if self.session:
                self.session.close()
        except Exception:
            pass
        if self.proc and self.proc.poll() is None:
            self.proc.terminate()
            try:
                self.proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()

    def wait_exit(self, timeout=15):
        try:
            return self.proc.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            return None

    def restart(self):
        """Close the session and start the app again in the same home."""
        try:
            self.session.close()
        except Exception:
            pass
        self.proc = None
        self._shots += 1
        self.start()

    # --- raw tools
    def call(self, tool, **a):
        return self.session.call(tool, **a)

    def settle(self, seconds=0.0):
        if seconds:
            time.sleep(seconds)
        self.session.call("settle")

    # --- tree
    def nodes(self):
        return tree.nodes(self.session)

    def find(self, role=None, label=None, label_contains=None, pred=None, nodes=None):
        return tree.find(nodes if nodes is not None else self.session, role=role, label=label,
                         label_contains=label_contains, pred=pred)

    def find_all(self, role=None, label=None, label_contains=None, pred=None, nodes=None):
        return tree.find_all(nodes if nodes is not None else self.session, role=role, label=label,
                             label_contains=label_contains, pred=pred)

    def wait_node(self, timeout=8, **kw):
        end = time.time() + timeout
        while time.time() < end:
            n = self.find(**kw)
            if n:
                return n
            time.sleep(0.2)
        return None

    def wait_gone(self, timeout=8, **kw):
        end = time.time() + timeout
        while time.time() < end:
            if not self.find(**kw):
                return True
            time.sleep(0.2)
        return False

    def center(self, n):
        b = n["bounds"]
        return b["x"] + b["width"] / 2, b["y"] + b["height"] / 2

    # --- input
    def move(self, x, y):
        self.call("inject_pointer", action="move", x=float(x), y=float(y), settle=SETTLE)

    def park(self):
        self.move(900.0, 12.0)

    def click_at(self, x, y, button="primary", double=False, **mods):
        """A real pointer click: move, down, up (or a double click)."""
        self.call("inject_pointer", action="move", x=float(x), y=float(y), settle=SETTLE)
        if double:
            self.call("inject_pointer", action="double_click", x=float(x), y=float(y), button=button, settle=SETTLE, **mods)
        else:
            self.call("inject_pointer", action="down", x=float(x), y=float(y), button=button, settle=SETTLE, **mods)
            self.call("inject_pointer", action="up", x=float(x), y=float(y), button=button, settle=SETTLE, **mods)

    def click(self, n, **kw):
        self.click_at(*self.center(n), **kw)

    def drag(self, x0, y0, x1, y1, steps=6, button="primary", **mods):
        self.call("inject_pointer", action="move", x=float(x0), y=float(y0), settle=SETTLE)
        self.call("inject_pointer", action="down", x=float(x0), y=float(y0), button=button, settle=SETTLE, **mods)
        for i in range(1, steps + 1):
            self.call("inject_pointer", action="move", x=x0 + (x1 - x0) * i / steps, y=y0 + (y1 - y0) * i / steps, settle=SETTLE)
        self.call("inject_pointer", action="up", x=float(x1), y=float(y1), button=button, settle=SETTLE)

    def key(self, key, text=None, **mods):
        a = dict(mods)
        if text is not None:
            a["text"] = text
        self.call("inject_key", key=key, settle=SETTLE, **a)

    def type(self, node, text):
        self.call("type_text", node=node["id"], text=text, settle=SETTLE)

    def wheel(self, x, y, dy, lines=True):
        # `scroll` takes a node; use the node under the point
        raise NotImplementedError

    def overlays(self):
        r = self.call("get_overlays")
        return r.get("overlays", []) if isinstance(r, dict) else r

    def n_overlays(self):
        r = self.call("get_overlays")
        return r.get("count", len(r.get("overlays", []))) if isinstance(r, dict) else len(r)

    # --- pictures
    def shot(self, name=None, node=None):
        """Save a screenshot; returns (path, sha1 of the pixels)."""
        r = self.session.call_result("screenshot", settle=SETTLE, **({"node": node["id"]} if node else {}))
        for c in r.content:
            if c.get("type") == "image":
                png = base64.b64decode(c["data"])
                path = self.probe.out / f"{name or 'shot%d' % int(time.time()*1000)}.png"
                path.write_bytes(png)
                self.hazards[path.stem] = hazard_pixels(path)
                return path, hashlib.sha1(png).hexdigest()
        raise Failure("no image in screenshot")

    def text_boxes(self):
        """Bounds of every text input: a blinking caret makes pixels there flicker."""
        return [n["bounds"] for n in self.nodes() if n.get("role") == "TextInput" and n.get("bounds")]

    def diff(self, path_a, path_b, ignore=None):
        """Number of pixels that differ between two screenshots outside `ignore` boxes (default: text inputs)."""
        from PIL import Image, ImageChops
        ia = Image.open(path_a).convert("RGB")
        ib = Image.open(path_b).convert("RGB")
        if ia.size != ib.size:
            return ia.size[0] * ia.size[1]
        d = ImageChops.difference(ia, ib)
        for b in (self.text_boxes() if ignore is None else ignore):
            x0, y0 = max(0, int(b["x"]) - 2), max(0, int(b["y"]) - 2)
            x1, y1 = int(b["x"] + b["width"]) + 2, int(b["y"] + b["height"]) + 2
            d.paste((0, 0, 0), (x0, y0, x1, y1))
        mask = d.convert("L").point(lambda v: 255 if v > 8 else 0)
        return sum(1 for v in mask.getdata() if v)

    def crop_diff(self, path_a, path_b, box):
        """Differing pixels between two screenshots inside `box` = (x0, y0, x1, y1)."""
        from PIL import Image, ImageChops
        d = ImageChops.difference(Image.open(path_a).convert("RGB").crop(box), Image.open(path_b).convert("RGB").crop(box))
        return sum(1 for v in d.convert("L").point(lambda v: 255 if v > 8 else 0).getdata() if v)

    def label_texts(self, region):
        """document_text of every Label whose top-left lies in region (x0, y0, x1, y1)."""
        x0, y0, x1, y1 = region
        out = []
        for n in self.nodes():
            b = n.get("bounds")
            if n.get("role") == "Label" and b and x0 <= b["x"] < x1 and y0 <= b["y"] < y1:
                out.append(((n.get("text") or {}).get("document_text")) or n.get("value") or "")
        return out

    def sha(self, node=None):
        return self.shot(f"_tmp", node)[1]

    def window_size(self):
        r = self.call("list_windows")
        return r


def root_size(app):
    """The size of the window's content, from the root group's bounds."""
    n = [x for x in app.nodes() if x.get("role") == "Group" and x.get("bounds", {}).get("x") == 0 and x["bounds"].get("y") == 0
         and x["bounds"]["width"] > 1000]
    b = n[0]["bounds"]
    return b["width"], b["height"]


def run(name, body):
    """Run `body(probe)`; the app it starts is stopped on every path.  Exits with the verdict."""
    p = Probe(name)
    apps = []
    p.apps = apps
    code = 1
    try:
        body(p)
        code = p.finish()
    except Exception as exc:  # the probe itself broke
        import traceback
        traceback.print_exc()
        print(f"  ERROR the probe could not run: {exc}", flush=True)
        code = 1
    finally:
        for a in apps:
            a.cleanup()
    sys.exit(code)


def start(p, **kw):
    a = App(p, **kw)
    p.apps.append(a)
    return a


_HAZARD = {(164, 133, 20), (131, 106, 14), (153, 124, 18), (145, 117, 16)}


def hazard_pixels(path) -> int:
    """Pixels of the debug inspector's overflow overlay (yellow hazard stripes painted where a container's
    children spill past its bounds; on by default in debug builds, docs/inspector.md of teksilo)."""
    from PIL import Image
    return sum(1 for px in Image.open(path).convert("RGB").getdata() if px in _HAZARD)


def no_hazards(probe, app, ignore=(), known_failure=None):
    """Check that no screenshot of `app` shows the overflow overlay (except those named in `ignore`)."""
    bad = {k: v for k, v in app.hazards.items() if v > 50 and k not in ignore}
    probe.check(not bad, "no screenshot shows the debug overflow overlay (hazard stripes)" + (f": {bad}" if bad else ""),
                known_failure=known_failure)


def dialogs(app):
    return [n for n in app.nodes() if n.get("role") in ("Dialog", "AlertDialog")]


def field_text(node):
    return (node.get("text") or {}).get("document_text", "")
