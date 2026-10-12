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
width = 1300
height = 850
placement = "Floating"
"""


def toml_window(app):
    import tomllib
    f = app.state_dir / "window_state.toml"
    return tomllib.loads(f.read_text())["windows"][0] if f.exists() else None


FINDING = ("a window that starts maximized has no restore size: the state file keeps the maximized size as "
           "width/height, so Restaurar after a restart gives a screen-sized floating window and the old floating size is lost")


def body(p):
    # A. a floating window: close and reopen keep the size
    a = start(p, window_state=SEED)
    p.check(root_size(a) == (1300.0, 850.0), f"a seeded floating geometry is restored at start: {root_size(a)}")
    p.check(a.find(role="Button", label="Maximizar"), "...and the window is not maximized")
    a.click(a.find(role="Button", label="Cerrar"))
    p.check(a.wait_exit(20) == 0, "closing exits 0")
    w = toml_window(a)
    p.check(w and w.get("placement") == "Floating" and (w["width"], w["height"]) == (1300, 850),
            f"close saves the floating geometry: {w}")
    a.restart()
    p.check(root_size(a) == (1300.0, 850.0), f"the next start opens at the same size {root_size(a)}")
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
    p.check(root_size(a) == (1300.0, 850.0), f"restoring after the restart brings back the floating size ({root_size(a)})",
            known_failure=FINDING)
    a.click(a.find(role="Button", label="Cerrar"))
    p.check(a.wait_exit(20) == 0, "closing exits 0")
    w = toml_window(a)
    p.check(w and w.get("placement") == "Floating" and (w["width"], w["height"]) == (1300, 850),
            f"close saves the floating size {w}", known_failure=FINDING)
    p.note("the window position is saved to window_state.toml but cannot be read back or set through the bridge (Wayland)")


run("p02_window_geometry", body)
