"""Text fields keep their keys: in every text field of the app, a space, Enter, Delete and the arrows edit the
text and never act on a panel (no song played or deleted, no selection moved, no confirmation opened)."""
from lib import *

PLAYER_TITLE = (0, 280, 340, 330)
METER_FINDING = ("in the VU meter popup, a click on a stepper's value closes the popup instead of turning the value into a "
                 "text field (the + and - buttons work; the same stepper in the Audio Center takes the click and the typing)")


def state(a):
    """What a panel would change if a key reached it."""
    cards = tuple(n["label"] for n in a.find_all(role="GridCell") if n.get("selected"))
    song_sel = tuple(n["label"] for n in a.find_all(role="ListItem") if n["bounds"]["x"] < 400 and n["bounds"]["y"] >= 440 and n.get("selected"))
    rows = len([n for n in a.find_all(role="ListItem") if n["bounds"]["x"] < 400 and n["bounds"]["y"] >= 440])
    status = a.label_texts((800, 960, 1450, 1000))
    title = tuple(a.label_texts(PLAYER_TITLE))
    dl = tuple(d["label"] for d in dialogs(a))
    expanded = tuple(n["label"] for n in a.find_all(role="GridCell") if n.get("expanded"))
    return {"cards": cards, "playlist selection": song_sel, "playlist rows": rows, "status bar": status, "player title": title,
            "dialogs": dl, "open albums": expanded}


def focused_input(a):
    r = [n for n in a.nodes() if n["role"] == "TextInput" and n.get("focused")]
    return r[0] if r else None


def prep(a):
    """A selected card in the library and a selected song in the playlist."""
    a.click(a.find(role="GridCell", label_contains="Anna Keller"))
    a.settle(0.4)
    rows = [n for n in a.find_all(role="ListItem") if n["bounds"]["x"] < 400 and n["bounds"]["y"] >= 440 and n["label"][:1].isdigit()]
    a.click(rows[1])
    a.settle(0.4)


def hammer(p, a, name, field_finder, click_target=None, with_enter=True, expect_final=None, ignore=()):
    """Type into a field and press the keys; check the text and that no panel reacted."""
    before = state(a)
    f = field_finder()
    a.click(click_target() if click_target else f)
    a.settle(0.4)
    f = focused_input(a)
    if not f:
        p.check(False, f"{name}: the field takes the keyboard focus")
        return
    a.type(f, "ab")
    a.type(f, " ")
    t1 = field_text(focused_input(a) or f)
    p.check(t1 == "ab ", f"{name}: a typed space is part of the text ({t1!r})")
    if with_enter:
        a.key("Enter")
        a.settle(0.4)
    for k in ("ArrowLeft", "ArrowLeft", "Delete", "ArrowDown", "ArrowUp", "ArrowRight", "Home", "End"):
        a.key(k)
    a.settle(0.4)
    cur = focused_input(a)
    t2 = field_text(cur) if cur else None
    p.check(t2 == (expect_final if expect_final is not None else "a "), f"{name}: Delete and the arrows edit the text ({t2!r})")
    p.check(cur is not None, f"{name}: the field keeps its focus")
    after = state(a)
    diff = {k: (before[k], after[k]) for k in before if before[k] != after[k] and k not in ignore}
    p.check(not diff, f"{name}: no panel reacted" + (f": {diff}" if diff else ""))


def body(p):
    a = start(p)
    prep(a)
    s0 = state(a)
    p.check(s0["cards"] and s0["playlist selection"], f"setup: a card and a playlist song are selected")

    # ---- the three search fields
    hammer(p, a, "playlist search", lambda: a.find(role="TextInput", label="Buscar...", pred=lambda n: n["bounds"]["x"] < 100))
    a.key("Escape")
    a.settle(0.4)
    hammer(p, a, "filter search", lambda: a.find(role="TextInput", label="Buscar...", pred=lambda n: 400 < n["bounds"]["x"] < 600))
    a.key("Escape")
    a.settle(0.4)
    hammer(p, a, "library search", lambda: a.find(role="TextInput", label="Buscar en la biblioteca"),
           ignore=("cards", "status bar"))  # typing in this field filters the library by design
    a.key("Escape")
    a.settle(0.4)

    # ---- the create dialog's field (Enter confirms a dialog: that is the dialog's own action)
    prep(a)
    before = state(a)
    a.click_at(340, 420, button="secondary")
    a.settle(0.4)
    a.click(a.find(role="MenuItem", label="Nueva lista"))
    a.settle(0.6)
    f = focused_input(a)
    a.type(f, "ab")
    a.type(f, " ")
    for k in ("ArrowLeft", "ArrowLeft", "Delete", "ArrowDown", "ArrowUp", "ArrowRight", "Home", "End"):
        a.key(k)
    a.settle(0.3)
    cur = focused_input(a)
    p.check(cur is not None and field_text(cur) == "a ", f"create dialog: Delete and the arrows edit the text ({field_text(cur) if cur else None!r})")
    mid = state(a)
    p.check({k: v for k, v in mid.items() if k != "dialogs"} == {k: v for k, v in before.items() if k != "dialogs"}, "create dialog: no panel reacted behind it")
    a.key("Escape")
    a.settle(0.5)
    p.check(not dialogs(a) and state(a) == before, "create dialog: Escape closes it and nothing else changed")

    # ---- the equalizer's save dialog field
    open_ac(a, "Ecualizador")
    a.click(a.find(role="Button", label="Guardar preset"))
    a.settle(0.7)
    f = focused_input(a)
    p.check(f is not None, "save dialog: the name field has the focus")
    if f:
        a.type(f, "ab")
        a.type(f, " ")
        for k in ("ArrowLeft", "ArrowLeft", "Delete", "ArrowDown", "ArrowUp", "ArrowRight", "Home", "End"):
            a.key(k)
        a.settle(0.3)
        cur = focused_input(a)
        p.check(cur is not None and field_text(cur) == "a ", f"save dialog: Delete and the arrows edit the text ({field_text(cur) if cur else None!r})")
        a.key("Escape")
        a.settle(0.5)
    p.check(ac_dialog(a) is not None and not [d for d in dialogs(a) if d["label"] != "Centro de Audio Avanzado"], "save dialog: Escape closes only the dialog")
    cur = state(a)
    p.check(cur["cards"] == before["cards"] and cur["playlist selection"] == before["playlist selection"] and cur["playlist rows"] == before["playlist rows"],
            "save dialog: nothing behind the Audio Center changed")

    # ---- the steppers' value fields (Audio Center, then the meter popup)
    a.click(a.find(role="Tab", label="Volumen y Mezcla"))
    a.settle(0.6)
    sp = a.find(role="SpinButton", label="Silencio más corto eliminado")
    b = sp["bounds"]
    a.click_at(b["x"] + b["width"] / 2, b["y"] + b["height"] / 2)
    a.settle(0.4)
    f = focused_input(a)
    p.check(f is not None, "stepper field: a click on the value gives it the focus")
    if f:
        a.key("a", text="a", command=True)
        a.type(f, "12")
        a.type(f, " ")
        for k in ("ArrowLeft", "Delete", "ArrowDown", "ArrowUp", "ArrowRight", "Home", "End"):
            a.key(k)
        a.settle(0.3)
        cur = focused_input(a)
        t = field_text(cur) if cur else None
        p.check(cur is not None and t is not None and t.strip().startswith("1") and len(t.strip()) <= 3, f"stepper field: Delete and the arrows edit the text ({t!r})")
        a.key("Enter")
        a.settle(0.4)
        p.check(a.find(role="SpinButton", label="Silencio más corto eliminado")["numeric_value"] != 1000.0, "stepper field: Enter commits the value")
    cur = state(a)
    p.check(cur["cards"] == before["cards"] and cur["playlist selection"] == before["playlist selection"] and cur["player title"] == before["player title"],
            "stepper field: no panel reacted")
    a.key("Escape")
    a.settle(0.6)
    # the meter popup's steppers
    a.click(a.find(role="Button", label="Ajustes del medidor"), button="secondary")
    a.settle(0.6)
    sp = a.find(role="SpinButton", label="Ventana RMS")
    b = sp["bounds"]
    a.click_at(b["x"] + b["width"] / 2, b["y"] + b["height"] / 2)
    a.settle(0.4)
    f = focused_input(a)
    p.check(f is not None, "meter stepper field: a click on the value gives it the focus (the + and - buttons of the same stepper work)",
            known_failure=METER_FINDING)
    if f is None:
        p.note(f"after the click the popup is {'open' if a.n_overlays() else 'closed'}")
    if f:
        a.key("a", text="a", command=True)
        a.type(f, "250")
        a.type(f, " ")
        for k in ("ArrowLeft", "Delete", "ArrowDown", "ArrowUp", "ArrowRight", "Home", "End"):
            a.key(k)
        a.settle(0.3)
        cur = focused_input(a)
        t = field_text(cur) if cur else None
        p.check(cur is not None and t is not None and t.strip().startswith("2"), f"meter stepper field: Delete and the arrows edit the text ({t!r})")
        a.key("Enter")
        a.settle(0.4)
    cur = state(a)
    p.check(cur["cards"] == before["cards"] and cur["playlist selection"] == before["playlist selection"] and cur["player title"] == before["player title"] and not cur["dialogs"],
            "meter stepper field: no panel reacted")
    a.key("Escape")
    a.settle(0.4)
    no_hazards(p, a)
    clean_log(p, a)


run("p09_text_fields", body)
