"""VU meter: a secondary click on the slot opens the settings popup anchored above it; enabling the meter shows
it; the popup stays open while the meter's values change; Escape and a click outside close it."""
from lib import *

SLOT = (1199, 960, 1433, 1000)


def slot(a):
    return a.find(role="Button", label="Ajustes del medidor")


def popup_title(a):
    return a.find(role="Label", pred=lambda n: n.get("value") == "Configuración del Meter")


def body(p):
    a = start(p)
    s = slot(a)
    sb = s["bounds"]
    p.check(not s.get("expanded"), "the settings popup is closed at start")
    a.park()
    a.settle(0.4)
    off, _ = a.shot("meter_off")
    # a left click on the slot does nothing
    a.click(s)
    a.settle(0.5)
    p.check(popup_title(a) is None and a.n_overlays() == 0, "a left click on the slot opens nothing")
    # a secondary click opens the popup, anchored above the slot
    a.click(s, button="secondary")
    a.settle(0.6)
    t = popup_title(a)
    p.check(t is not None and a.find(role="Button", label="Ajustes del medidor").get("expanded"), "a secondary click on the slot opens the settings popup")
    if t:
        tb = t["bounds"]
        reset = a.find(role="Button", label="Restablecer pico")
        bottom = reset["bounds"]["y"] + reset["bounds"]["height"]
        p.check(bottom <= sb["y"] + 1 and sb["y"] - bottom < 40, f"...above the slot (its last row ends at y={bottom:.0f}, the slot starts at y={sb['y']:.0f})")
        p.check(abs(tb["x"] - sb["x"]) < 20, f"...at the slot's left edge (popup text x={tb['x']:.0f}, slot x={sb['x']:.0f})")
    a.shot("popup")
    chk = a.find(role="CheckBox", label="Mostrar medidor")
    p.check(chk and chk.get("toggled") == "false", "the meter is off by default")
    a.click(chk)
    a.settle(0.8)
    p.check(a.find(role="CheckBox", label="Mostrar medidor").get("toggled") == "true", "a click on 'Mostrar medidor' turns it on")
    p.check(popup_title(a) is not None, "...and the popup stays open")
    on, _ = a.shot("meter_on")
    p.check(a.crop_diff(off, on, (SLOT[0], SLOT[1], SLOT[2], SLOT[3])) > 100, "the meter appears in the slot")
    # other options toggle without closing it
    a.click(a.find(role="CheckBox", label="Modo M/S"))
    a.settle(0.4)
    p.check(a.find(role="CheckBox", label="Modo M/S").get("toggled") == "true" and popup_title(a), "'Modo M/S' toggles and the popup stays")
    a.click(a.find(role="CheckBox", label="Modo M/S"))
    a.settle(0.3)
    # Escape closes it
    a.key("Escape")
    a.settle(0.5)
    p.check(popup_title(a) is None and not slot(a).get("expanded"), "Escape closes the popup")
    p.check(a.crop_diff(off, a.shot("meter_still_on")[0], SLOT) > 100, "...and the meter stays on")
    # while playing, the values change and the popup stays open
    a.click(a.find(role="ListItem", label_contains="1. Watch That Man"), double=True)
    a.settle(1.0)
    a.click(slot(a), button="secondary")
    a.settle(0.6)
    p.check(popup_title(a) is not None, "reopened while a song plays")
    # The mocks never write meter levels, so the values stay still; what can be checked is that the popup survives
    # the other things that redraw the bottom bar: a playback state change and a seek made through the assistive route.
    p.note("the mocks' fake engine never writes meter levels (set_meter is test-only): the meter's values do not move")
    play = a.find(role="Button", label="Reproducir o pausar")
    a.call("invoke_action", node=play["id"], action="click", settle=SETTLE)
    a.settle(1.2)
    p.check(popup_title(a) is not None and slot(a).get("expanded"), "the popup stays open when playback is paused")
    a.call("invoke_action", node=play["id"], action="click", settle=SETTLE)
    a.settle(1.2)
    p.check(popup_title(a) is not None and slot(a).get("expanded"), "...and when it resumes")
    sk = a.find(role="Slider", label="Posición de reproducción")
    a.call("set_value", node=sk["id"], value="0.5", settle=SETTLE)
    a.settle(1.0)
    p.check(popup_title(a) is not None, "...and when the position is moved")
    a.settle(2.5)
    p.check(popup_title(a) is not None, "...and a few seconds later")
    # a click outside closes it
    a.click_at(900, 500)
    a.settle(0.6)
    p.check(popup_title(a) is None, "a click outside closes the popup")
    p.note(f"cards selected after the outside click: {len([c for c in a.find_all(role='GridCell') if c.get('selected')])}")
    # the peak reset button answers
    a.click(slot(a), button="secondary")
    a.settle(0.5)
    a.click(a.find(role="Button", label="Restablecer pico"))
    a.settle(0.4)
    p.check(popup_title(a) is not None, "'Restablecer pico' answers and the popup stays")
    a.key("Escape")
    a.settle(0.4)
    no_hazards(p, a)


run("p08_vu_meter", body)
