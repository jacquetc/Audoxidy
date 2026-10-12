"""Window geometry: size and placement return after close and reopen (window_state.toml).

The compositor (Wayland) gives a client no way to be moved by the automation tools, so the probe
seeds the isolated home's `window_state.toml` with a floating geometry, checks the app opens at it,
maximizes, closes, checks what the file says and what the next start restores."""
from lib import *

SEED = """version = 2

[[windows]]
label = "main"
x = 120
y = 90
width = 1250
height = 790
placement = "Floating"
"""


def toml_window(app):
    import tomllib
    f = app.state_dir / "window_state.toml"
    return tomllib.loads(f.read_text())["windows"][0] if f.exists() else None


MINWIDTH_FINDING = ("main_window.rs says MIN_SIZE = 900 x 500, but the layout needs about 1204 px of width: a window restored at "
                    "1100 px wide has its title bar's close button at x = 1170, outside the window")
FINDING = ("a window that starts maximized has no restore size: the state file keeps the maximized size as "
           "width/height, so Restaurar after a restart gives a screen-sized floating window and the old floating size is lost")


def body(p):
    # A. a floating window: close and reopen keep the size
    a = start(p, window_state=SEED)
    p.check(root_size(a) == (1250.0, 790.0), f"a seeded floating geometry is restored at start: {root_size(a)}")
    p.check(a.find(role="Button", label="Maximizar"), "...and the window is not maximized")
    a.click(a.find(role="Button", label="Cerrar"))
    p.check(a.wait_exit(20) == 0, "closing exits 0")
    w = toml_window(a)
    p.check(w and w.get("placement") == "Floating" and (w["width"], w["height"]) == (1250, 790),
            f"close saves the floating geometry: {w}")
    a.restart()
    p.check(root_size(a) == (1250.0, 790.0), f"the next start opens at the same size {root_size(a)}")
    # B. maximize, close, reopen
    a.click(a.find(role="Button", label="Maximizar"))
    a.settle(1.5)
    big = root_size(a)
    a.click(a.find(role="Button", label="Cerrar"))
    p.check(a.wait_exit(20) == 0, "closing a maximized window exits 0")
    w = toml_window(a)
    p.check(w and w.get("placement") == "Maximized", f"close saves the placement: {w}")
    a.restart()
    p.check(root_size(a) == big and a.find(role="Button", label="Restaurar"),
            f"the next start is maximized again ({root_size(a)})")
    a.click(a.find(role="Button", label="Restaurar"))
    a.settle(1.5)
    p.check(root_size(a) == (1250.0, 790.0), f"restoring after the restart brings back the floating size ({root_size(a)})",
            known_failure=FINDING)
    a.click(a.find(role="Button", label="Cerrar"))
    p.check(a.wait_exit(20) == 0, "closing exits 0")
    w = toml_window(a)
    p.check(w and w.get("placement") == "Floating" and (w["width"], w["height"]) == (1250, 790),
            f"close saves the floating size {w}", known_failure=FINDING)
    a.cleanup()
    # C. a narrow window: the title bar's controls must stay inside it
    NARROW = SEED.replace("width = 1250", "width = 1100").replace("height = 790", "height = 760")
    a = start(p, window_state=NARROW)
    w, h = root_size(a)
    close = a.find(role="Button", label="Cerrar")
    p.check(w == 1100.0, f"a 1100 px wide window is restored at that width ({w})")
    p.check(close["bounds"]["x"] + close["bounds"]["width"] <= w + 0.5,
            f"its close button lies inside the window (right edge {close['bounds']['x'] + close['bounds']['width']:.0f} for a {w:.0f} px window)",
            known_failure=MINWIDTH_FINDING)
    # D. an oversized saved size
    BIG = SEED.replace("width = 1250", "width = 1500").replace("height = 790", "height = 900")
    a2 = start(p, window_state=BIG)
    p.note(f"a saved size of 1500 x 900 is restored as {root_size(a2)} on this display (maximized: see p01)")
    p.note("the window position is saved to window_state.toml but cannot be read back or set through the bridge (Wayland)")


run("p02_window_geometry", body)
