"""Title bar: source pills, minimize, maximize/restore (and the control's name), close."""
from lib import *


def body(p):
    a = start(p)
    # -- the source pills
    tabs = a.find_all(role="Tab", pred=lambda n: n["bounds"]["y"] < 40 and n["bounds"]["x"] > 600)
    names = [t["label"] for t in tabs]
    p.check(names == ["Biblioteca", "Spotify", "YouTube", "Qobuz", "Deezer", "Tidal"], f"the six source pills in order: {names}")
    lib = a.find(role="Tab", label="Biblioteca")
    p.check(lib.get("selected") and not lib.get("disabled"), "Biblioteca is selected and enabled")
    for n in ("Spotify", "YouTube", "Qobuz", "Deezer", "Tidal"):
        t = a.find(role="Tab", label=n)
        p.check(t.get("disabled"), f"{n} is disabled (streaming stub)")
    a.park()
    pb, _ = a.shot("pills_before")
    a.click(a.find(role="Tab", label="Spotify"))
    a.park()
    pa, _ = a.shot("pills_after")
    p.check(a.diff(pb, pa) == 0, "a pointer click on a disabled pill changes nothing on screen")
    p.check(a.find(role="Tab", label="Biblioteca").get("selected") and not a.find(role="Tab", label="Spotify").get("selected"),
            "...and Biblioteca stays the selected source")

    # -- maximize / restore, the control's name follows
    w0 = root_size(a)
    p.check(a.find(role="Button", label="Maximizar") and not a.find(role="Button", label="Restaurar"), "the control is named Maximizar at first")
    a.click(a.find(role="Button", label="Maximizar"))
    a.settle(1.5)
    w1 = root_size(a)
    p.check(w1 != w0 and w1[0] > w0[0], f"a pointer click on Maximizar maximizes the window {w0} -> {w1}")
    p.check(a.find(role="Button", label="Restaurar") and not a.find(role="Button", label="Maximizar"), "the control is now named Restaurar")
    a.shot("maximized")
    a.click(a.find(role="Button", label="Restaurar"))
    a.settle(1.5)
    w2 = root_size(a)
    p.check(w2 == w0, f"a pointer click on Restaurar restores the size {w2}")
    p.check(a.find(role="Button", label="Maximizar"), "the control is named Maximizar again")

    # -- minimize: the click is delivered and the app keeps running.  Whether the compositor really
    # minimized the window cannot be read through the automation tools (see README, "gaps").
    a.click(a.find(role="Button", label="Minimizar"))
    a.settle(1.0)
    p.check(a.alive(), "a click on Minimizar leaves the app running")
    p.note("minimized state itself is not observable through the automation bridge (no window-state tool)")

    # -- close: the process exits 0
    a.click(a.find(role="Button", label="Cerrar"))
    code = a.wait_exit(20)
    p.check(code == 0, f"a click on Cerrar closes the window and the process exits 0 (exit={code})")


run("p01_title_bar", body)
