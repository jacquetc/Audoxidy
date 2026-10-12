"""Every button answers the pointer: press every visible button with a real pointer click, parking the pointer
away and waiting about a second for playback state, and check that the screen (or the overlays) changed.
The exceptions are the documented stubs (disabled), the meter slot's left click, and the buttons that would leave
the app (folder picker, quit, close, minimize)."""
from lib import *

SKIP = {
    "Cerrar": "closes the app or the window (checked in p01 / p07)",
    "Minimizar": "minimizes the real window (checked in p01)",
    "Maximizar": "resizes the real window (checked in p01)",
    "Restaurar": "resizes the real window (checked in p01)",
    "Añadir una carpeta": "opens the system's folder picker, outside the app",
    "Abrir Carpeta": "opens the system's folder picker, outside the app",
    "Salir": "quits the app",
    "Ajustes del medidor": "a left click on the meter slot does nothing by design (the right click is p08)",
    "Importar": "opens the system's file picker",
    "Exportar": "opens the system's file picker",
    "Reiniciar Servicio de Audio": "restarts the real audio service",
}
# Pressed buttons whose effect is not visible with the mocks, with the reason.
INVISIBLE = {
    "Restablecer pico": "resets the meter's held peak, and the mocks never write any level",
}


def visible(n):
    b = n.get("bounds")
    return bool(b) and b["width"] >= 4 and b["height"] >= 4 and b["x"] >= 0 and b["y"] >= 0 and b["x"] + b["width"] <= 1600 and b["y"] + b["height"] <= 1000


def close_layers(a):
    for _ in range(4):
        if a.n_overlays() or [d for d in dialogs(a) if d["label"] != "Centro de Audio Avanzado"]:
            a.key("Escape")
            a.settle(0.4)
        else:
            break


def close_all(a):
    """Close every overlay and dialog, the Audio Center included."""
    for _ in range(5):
        if a.n_overlays() or dialogs(a):
            a.key("Escape")
            a.settle(0.4)
        else:
            break


def grid_to_top_and_closed(a):
    """Scroll the grid to the top and close any open album, so that the chevrons are where the next press expects them."""
    for _ in range(4):
        g = a.find(role="Grid", label="Álbumes")
        a.call("scroll", node=g["id"], dy=-300.0, lines=True, settle=SETTLE)
        a.settle(0.8)
        opened = [c for c in a.find_all(role="GridCell") if c.get("expanded")]
        if not opened:
            return
        title = opened[0]["label"].split(" — ", 1)[-1]
        chev = a.find(role="Button", label=f"Abrir o cerrar {title}")
        if chev:
            a.click(chev)
            a.settle(0.8)


def press(a, finder, *, restore=None):
    """Press one control; returns (changed, pixels, overlays before/after)."""
    a.park()
    a.settle(1.0)
    before, _ = a.shot("before")
    ob = a.n_overlays()
    a.click(finder())
    a.park()
    a.settle(1.0)
    after, _ = a.shot("after")
    oa = a.n_overlays()
    d = a.diff(before, after)
    return (d > 30 or ob != oa), d, ob, oa


def sweep(p, a, nodes_fn, scope, results, before_each=None, after_each=None):
    labels = []
    for n in nodes_fn():
        key = (n["role"], n.get("label"))
        if key not in labels:
            labels.append(key)
    for role, label in labels:
        if label in SKIP:
            results.append((scope, role, label, "skipped: " + SKIP[label]))
            continue
        def finder(role=role, label=label):
            cur = [n for n in nodes_fn() if n["role"] == role and n.get("label") == label]
            return cur[0] if cur else None
        n = finder()
        if n is None:
            results.append((scope, role, label, "gone"))
            continue
        if n.get("disabled"):
            results.append((scope, role, label, "disabled (stub)"))
            continue
        if before_each:
            before_each(role, label)
            n = finder()
            if n is None:
                results.append((scope, role, label, "gone"))
                continue
        changed, d, ob, oa = press(a, finder)
        tag = "changed" if changed else ("no visible effect, accepted: " + INVISIBLE[label] if label in INVISIBLE else "NO EFFECT")
        results.append((scope, role, label, f"{tag} ({d} px, overlays {ob}->{oa})"))
        if after_each:
            after_each(role, label)
        close_layers(a)


def body(p):
    a = start(p)
    results = []

    def to_third_song():
        a.click(a.find(role="ListItem", label_contains="3. Drive-In Saturday"), double=True)
        a.settle(1.5)

    def prep(role=None, label=None):
        # a playing third song (so that previous and next both act), a selected card (so that "play selection" acts)
        if label in ("Pista anterior", "Pista siguiente", "Reproducir o pausar"):
            to_third_song()
        if label in ("Reproducir la selección",):
            a.click(a.find(role="GridCell", label_contains="Anna Keller"))
            a.settle(0.5)

    # ---- the main window's buttons (the view button is last: it rebuilds the library)
    def main_nodes():
        out = [n for n in a.nodes() if n["role"] == "Button" and visible(n) and not str(n.get("label", "")).startswith("Abrir o cerrar")
               and n.get("label") not in ("Cambiar de vista", "Mostrar lo que suena")]
        return out
    to_third_song()
    sweep(p, a, main_nodes, "main window", results, before_each=prep, after_each=lambda r, l: close_all(a))
    close_layers(a)
    if ac_dialog(a):
        a.key("Escape")
        a.settle(0.5)

    # ---- the column headers
    sweep(p, a, lambda: [n for n in a.nodes() if n["role"] == "ColumnHeader" and visible(n) and n["bounds"]["x"] < 1500 and n.get("label")],
          "headers", results)
    # ---- the album chevrons
    sweep(p, a, lambda: [n for n in a.nodes() if n["role"] == "Button" and visible(n) and str(n.get("label", "")).startswith("Abrir o cerrar")
                         and n["bounds"]["y"] >= 70 and n["bounds"]["y"] + n["bounds"]["height"] <= 958][:6],
          "chevrons", results,
          before_each=lambda r, l: grid_to_top_and_closed(a),
          after_each=lambda r, l: grid_to_top_and_closed(a))
    # ---- the player menu's entries
    def menu_entries():
        if not a.find(role="Menu"):
            a.click(a.find(role="Button", label="Menú"))
            a.settle(0.6)
        return [n for n in a.nodes() if n["role"] == "MenuItem"]
    def reopen(role, label):
        close_layers(a)
        if ac_dialog(a):
            a.key("Escape")
            a.settle(0.5)
        if not a.find(role="Menu"):
            a.click(a.find(role="Button", label="Menú"))
            a.settle(0.6)
    sweep(p, a, menu_entries, "player menu", results, before_each=reopen,
          after_each=lambda r, l: (ac_dialog(a) and (a.key("Escape"), a.settle(0.5))))
    close_layers(a)
    if a.find(role="Menu"):
        a.key("Escape")
        a.settle(0.4)

    # ---- inside the Audio Center, tab by tab
    open_ac(a, "Configuración de Audio")
    for tab in ("Configuración de Audio", "Ecualizador", "Efectos de Audio", "Volumen y Mezcla"):
        a.click(a.find(role="Tab", label=tab))
        a.settle(0.6)
        def ac_nodes(tab=tab):
            d = ac_dialog(a)["bounds"]
            return [n for n in a.nodes() if n["role"] in ("Button", "Switch", "CheckBox", "RadioButton") and visible(n)
                    and d["x"] <= n["bounds"]["x"] < d["x"] + d["width"] and d["y"] + 50 <= n["bounds"]["y"] < d["y"] + d["height"]
                    and n.get("label") not in ("Cerrar",) and not str(n.get("label", "")).startswith("Abrir o cerrar")]
        def ac_prep(role, label, tab=tab):
            if label == "Predeterminado":
                cb = a.find(role="ComboBox", label="Frecuencia de Muestreo")
                a.click(cb)
                a.settle(0.4)
                a.click(a.find(role="ListBoxOption", label="96000 Hz"))
                a.settle(0.4)
            if label in ("Restablecer ecualizador", "Predeterminado") and tab == "Ecualizador":
                band = a.find(role="Slider", label="Banda de 125 Hz")
                cx, cy = a.center(band)
                a.drag(cx, cy, cx, cy - 50, steps=3)
                a.settle(0.4)
        sweep(p, a, ac_nodes, f"Audio Center / {tab}", results, before_each=ac_prep)
        close_layers(a)
        if not ac_dialog(a):
            open_ac(a, "Configuración de Audio")
    # the AC's own tabs
    for tab in ("Configuración de Audio", "Ecualizador", "Efectos de Audio", "Volumen y Mezcla"):
        a.click(a.find(role="Tab", label="Ecualizador" if tab != "Ecualizador" else "Efectos de Audio"))
        a.settle(0.6)
        changed, d, ob, oa = press(a, lambda tab=tab: a.find(role="Tab", label=tab))
        results.append(("Audio Center", "Tab", tab, f"{'changed' if changed else 'NO EFFECT'} ({d} px)"))
    a.key("Escape")
    a.settle(0.6)

    # ---- the meter popup's buttons
    a.click(a.find(role="Button", label="Ajustes del medidor"), button="secondary")
    a.settle(0.6)
    sweep(p, a, lambda: [n for n in a.nodes() if n["role"] == "Button" and n.get("label") == "Restablecer pico"] if a.find(role="Label", pred=lambda x: x.get("value") == "Configuración del Meter") else [n for n in []],
          "meter popup", results)
    close_layers(a)
    # ---- the view button, last
    seen = []
    for i in range(4):
        before = a.shot("vm_before")[0]
        a.park()
        a.click(a.find(role="Button", label="Cambiar de vista"))
        a.park()
        a.settle(1.2)
        after = a.shot("vm_after")[0]
        seen.append(a.diff(before, after))
    results.append(("main window", "Button", "Cambiar de vista", f"{'changed' if all(d > 30 for d in seen) else 'NO EFFECT'} ({seen} px over 4 presses)"))
    a.click(a.find(role="GridCell", label_contains="Anna Keller"))
    a.settle(0.4)
    changed, d, ob, oa = press(a, lambda: a.find(role="Button", label="Mostrar lo que suena"))
    results.append(("main window", "Button", "Mostrar lo que suena", f"{'changed' if changed else 'NO EFFECT'} ({d} px)"))

    for scope, role, label, res in results:
        print(f"    {scope:30} {role:12} {str(label)[:48]!r:50} {res}")
    bad = [r for r in results if "NO EFFECT" in r[3]]
    pressed = [r for r in results if r[3].startswith(("changed", "NO EFFECT", "no visible"))]
    p.check(not bad, f"every enabled control answers the pointer ({len(pressed)} pressed" + (f"; no effect: {[(r[0], r[2]) for r in bad]}" if bad else "") + ")")
    p.check(len([r for r in results if r[3].startswith("disabled")]) >= 1, "the disabled stubs were left alone")
    p.check(not [r for r in results if r[3] == "gone"], f"every control found could be pressed (gone: {[r[2] for r in results if r[3] == 'gone']})")
    no_hazards(p, a, known_failure=HAZARD_FINDING)
    clean_log(p, a)


run("p10_every_button", body)
