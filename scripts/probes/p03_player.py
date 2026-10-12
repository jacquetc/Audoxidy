"""Player: the menu, the three zones, the wheel volume bubble, the seek bar."""
from lib import *

PLAYER = (0, 0, 400, 400)
DISABLED = ["Abrir Archivo", "Biblioteca", "Lista de Reproducción", "Lyrics", "Apagado Automático",
            "Personalización", "Preferencias", "Complementos", "Acerca"]
ENABLED = ["Abrir Carpeta", "Configuración de Audio", "Ecualizador", "Efectos de Audio", "Salir"]


def title(a):
    t = a.label_texts((0, 280, 340, 330))
    return t[0] if t else None


def body(p):
    a = start(p)
    # ---- the menu
    p.check(a.n_overlays() == 0, "no overlay at start")
    a.click(a.find(role="Button", label="Menú"))
    a.settle(0.5)
    menu = a.find(role="Menu", label="Menú del reproductor")
    p.check(menu and a.n_overlays() == 1, "a pointer click on the menu button opens the player menu")
    if menu:
        mb = menu["bounds"]
        p.check(mb["x"] < 60 and mb["y"] < 60, f"...anchored under the button ({mb['x']:.0f},{mb['y']:.0f})")
    for label in DISABLED:
        it = a.find(role="MenuItem", label=label)
        p.check(it and it.get("disabled"), f"menu entry {label!r} is disabled (stub)")
    for label in ENABLED:
        it = a.find(role="MenuItem", label=label)
        p.check(it and not it.get("disabled"), f"menu entry {label!r} is enabled")
    # a pointer click on a disabled entry does nothing, the menu stays
    a.click(a.find(role="MenuItem", label="Preferencias"))
    a.settle(0.3)
    p.check(a.n_overlays() == 1 and a.find(role="Menu"), "a pointer click on a disabled entry leaves the menu open and does nothing")
    a.key("Escape")
    a.settle(0.3)
    p.check(a.n_overlays() == 0 and not a.find(role="Menu"), "Escape closes the menu")

    # ---- the three zones
    row = a.find(role="ListItem", label_contains="1. Watch That Man")
    a.click(row, double=True)
    a.settle(1.5)
    p.check(title(a) and title(a).startswith("Watch That Man"), f"a double click on a playlist row plays it: {title(a)!r}")
    bubble = (166, 166, 234, 234)
    play = a.find(role="Button", label="Reproducir o pausar")
    p.check(a.find(role="Button", label="Pista anterior") and a.find(role="Button", label="Pista siguiente"), "the three zones exist")
    a.park(); a.settle(0.5)
    pa, _ = a.shot("playing")
    a.click(play)
    a.park(); a.settle(1.0)
    pb, _ = a.shot("paused")
    p.check(a.crop_diff(pa, pb, bubble) > 20, "a pointer click on the middle zone toggles the bubble icon (pause)")
    a.click(play)
    a.park(); a.settle(1.0)
    pc, _ = a.shot("resumed")
    p.check(a.crop_diff(pa, pc, bubble) < 5 and a.crop_diff(pb, pc, bubble) > 20, "a second click toggles it back (play)")
    a.click(a.find(role="Button", label="Pista siguiente"))
    a.settle(1.5)
    p.check((title(a) or "").startswith("Aladdin Sane") or "addin Sane" in (title(a) or ""), f"a pointer click on the right zone plays the next track: {title(a)!r}")
    a.click(a.find(role="Button", label="Pista siguiente"))
    a.settle(1.5)
    p.check("Drive-In" in (title(a) or ""), f"...and again: {title(a)!r}")
    a.click(a.find(role="Button", label="Pista anterior"))
    a.settle(1.5)
    p.check("addin Sane" in (title(a) or ""), f"a pointer click on the left zone plays the previous track: {title(a)!r}")

    # ---- the wheel steps the volume and shows the bubble
    def vol():
        for t in a.label_texts((150, 150, 260, 260)):
            if t.strip().isdigit():
                return int(t)
        return None
    p.check(vol() is None, "no volume bubble while idle")
    a.call("scroll", node=play["id"], dy=-2.0, lines=True, settle=SETTLE)
    a.settle(0.3)
    v1 = vol()
    p.check(v1 is not None, f"a wheel notch shows the volume bubble ({v1})")
    a.shot("bubble")
    a.call("scroll", node=play["id"], dy=-2.0, lines=True, settle=SETTLE)
    a.settle(0.3)
    v2 = vol()
    a.call("scroll", node=play["id"], dy=4.0, lines=True, settle=SETTLE)
    a.settle(0.3)
    v3 = vol()
    p.check(v1 is not None and v2 is not None and v3 is not None and v2 != v1 and (v2 - v1) * (v3 - v2) < 0,
            f"the wheel steps the volume in both directions ({v1} -> {v2} -> {v3})")
    a.settle(3.5)
    p.check(vol() is None, "the bubble goes away after a moment")

    # ---- the seek bar
    sl = a.find(role="Slider", label="Posición de reproducción")
    p.check(sl is not None, "the seek bar is a Slider named Posición de reproducción")
    if sl:
        b = sl["bounds"]
        a.click_at(b["x"] + b["width"] * 0.75, b["y"] + b["height"] / 2)
        a.settle(0.8)
        t75 = a.label_texts((300, 320, 400, 360))
        a.click_at(b["x"] + b["width"] * 0.25, b["y"] + b["height"] / 2)
        a.settle(0.8)
        t25 = a.label_texts((300, 320, 400, 360))

        def secs(ts):
            for t in ts:
                if ":" in t:
                    m, s = t.split(":")[-2:]
                    return int(m) * 60 + int(s)
        s75, s25 = secs(t75), secs(t25)
        p.check(s75 and s25 and s75 > s25 > 0, f"clicking the seek bar seeks: 75% -> {t75}, 25% -> {t25}")


run("p03_player", body)
