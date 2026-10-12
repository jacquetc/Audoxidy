"""Audio Center: opening on each tab from the player menu, dragging by the header, closing, the pick lists, the
equalizer (switch, 20/31 bands, a band by drag and by secondary click, the load dialog's preview and Cancel,
the save dialog), the effects, the volume and mix steppers, and Escape closing the innermost layer first."""
from lib import *

DRAG_FINDING = ("a drag of the Audio Center's header (and of a column grip) follows the pointer at about half its speed when "
                "the pointer moves in small steps (50 px of travel moves the panel 25 px; one 50 px jump is exact): "
                "the handlers seem to measure the pointer in the moving widget's own coordinates")
MOCK_NOTE = "the mocks' fake engine names bands beyond the 20th 'Banda de 0 Hz' (a mocks artefact, not asserted)"


def combo(a, label):
    return a.find(role="ComboBox", label=label)


def popup_list(a):
    """The open pick list: the unnamed ListBox (the playlist's one is named)."""
    return a.find(role="ListBox", pred=lambda n: not n.get("label"))


def bands(a):
    return [n for n in a.find_all(role="Slider") if n["label"] == "Preamplificación" or n["label"].startswith("Banda de")]


def band_values(a):
    return [n["numeric_value"] for n in bands(a)]


def preset_buttons(a):
    # The rows are list-box options (selected while highlighted), since Wave F.
    return [n["label"] for n in a.find_all(role="ListBoxOption") if n["bounds"]["x"] > 640 and n["bounds"]["x"] < 660 and n["bounds"]["width"] == 138]


def body(p):
    a = start(p)
    # ---- open on each tab from the player menu
    for entry, tab in (("Configuración de Audio", "Configuración de Audio"), ("Ecualizador", "Ecualizador"), ("Efectos de Audio", "Efectos de Audio")):
        d = open_ac(a, entry)
        p.check(d is not None, f"the player menu entry {entry!r} opens the Audio Center")
        t = a.find(role="Tab", label=tab)
        p.check(t and t.get("selected"), f"...on its {tab!r} tab")
        b = d["bounds"]
        p.check(abs(b["width"] - 940) < 2 and abs(b["height"] - 480) < 2 and abs((b["x"] + b["width"] / 2) - 800) < 3,
                f"...940 x 480, centred in the window ({b['x']:.0f},{b['y']:.0f})")
        a.click(ac_close_button(a))
        a.settle(0.6)
        p.check(ac_dialog(a) is None, "the close button closes it")
    d = open_ac(a, "Configuración de Audio")
    a.click(a.find(role="Tab", label="Volumen y Mezcla"))
    a.settle(0.5)
    p.check(a.find(role="Tab", label="Volumen y Mezcla").get("selected") and a.find(role="Group", label="Volumen y Mezcla"), "a pointer click on the fourth tab shows Volumen y Mezcla")
    a.click(a.find(role="Tab", label="Configuración de Audio"))
    a.settle(0.5)

    # ---- drag by the header
    d = ac_dialog(a)["bounds"]
    x0, y0 = d["x"], d["y"]
    sx, sy = x0 + 270, y0 + 20
    a.move(sx, sy)
    a.call("inject_pointer", action="down", x=sx, y=sy, settle=SETTLE)
    for i in range(1, 7):
        a.move(sx + 10 * i, sy + 5 * i)
    a.call("inject_pointer", action="up", x=sx + 60, y=sy + 30, settle=SETTLE)
    a.settle(0.4)
    d = ac_dialog(a)["bounds"]
    dx, dy = d["x"] - x0, d["y"] - y0
    p.check(dx > 0 and dy > 0, f"a drag on the header moves the dialog towards the pointer ({dx:+.0f},{dy:+.0f})")
    # The first drag of a centred panel starts from Iced's centre on a 474 px height, 3 px
    # under the drawn 480 px panel's (src/gui/app.rs, the view-model's CENTRE_HEIGHT).
    p.check(abs(dx - 60) < 1 and abs(dy - 33) < 1, f"...by the distance the pointer travelled ({dx:+.0f},{dy:+.0f} for +60,+30, plus Iced's 3 px on a first drag)")
    a.shot("dragged")
    # a one-jump drag is exact and clamps inside the window
    d = ac_dialog(a)["bounds"]
    a.drag(d["x"] + 270, d["y"] + 20, d["x"] + 270 - 1200, d["y"] + 20 - 600, steps=1)
    a.settle(0.4)
    d = ac_dialog(a)["bounds"]
    p.check(d["x"] >= -1 and d["y"] >= -1, f"dragging far up-left keeps the dialog inside the window ({d['x']:.0f},{d['y']:.0f})")
    a.drag(d["x"] + 270, d["y"] + 20, 600, 280, steps=1)
    a.settle(0.4)
    d = ac_dialog(a)["bounds"]
    p.check(abs(d["x"] - 330) < 3 and abs(d["y"] - 260) < 3, f"a single jump puts it back where it was ({d['x']:.0f},{d['y']:.0f})")

    # ---- the config tab: pick lists open under their box
    for label in ("Núcleo de Audio", "Dispositivo de Salida", "Frecuencia de Muestreo", "Profundidad de Bits"):
        cb = combo(a, label)
        a.click(cb)
        a.settle(0.5)
        lb = popup_list(a)
        cbb = cb["bounds"]
        p.check(lb and abs(lb["bounds"]["x"] - cbb["x"]) < 3 and 0 <= lb["bounds"]["y"] - (cbb["y"] + cbb["height"]) <= 8,
                f"the {label!r} pick list opens under its box (box y+h={cbb['y'] + cbb['height']:.0f}, list y={lb['bounds']['y'] if lb else None})")
        a.key("Escape")
        a.settle(0.4)
        p.check(popup_list(a) is None and ac_dialog(a) is not None, "...Escape closes the list first and leaves the Audio Center open")
    cb = combo(a, "Frecuencia de Muestreo")
    a.click(cb)
    a.settle(0.5)
    a.click(a.find(role="ListBoxOption", label="96000 Hz"))
    a.settle(0.5)
    p.check(combo(a, "Frecuencia de Muestreo").get("value") == "96000 Hz" and popup_list(a) is None, "picking an option sets the box and closes the list")
    apply_btn = a.find(role="Button", label="Aplicar")
    p.check(apply_btn and not apply_btn.get("disabled"), "...which enables Aplicar")
    a.click(a.find(role="Button", label="Predeterminado"))
    a.settle(0.5)
    p.check(combo(a, "Frecuencia de Muestreo").get("value") == "48000 Hz", "Predeterminado puts the defaults back")

    # ---- the equalizer
    a.click(a.find(role="Tab", label="Ecualizador"))
    a.settle(0.8)
    sw = a.find(role="Switch", label="Activar Ecualizador")
    was = sw.get("toggled")
    a.click(sw)
    a.settle(0.5)
    now = a.find(role="Switch", label="Activar Ecualizador").get("toggled")
    p.check(was != now, f"a pointer click on the switch toggles the equalizer ({was} -> {now})")
    a.click(a.find(role="RadioButton", label="20"))
    a.settle(0.8)
    p.check(len(bands(a)) == 21 and a.find(role="RadioButton", label="20").get("toggled") == "true", f"the 20-band choice shows the preamp and 20 bands ({len(bands(a))} sliders)")
    a.click(a.find(role="RadioButton", label="31"))
    a.settle(0.8)
    p.check(len(bands(a)) == 32, f"the 31-band choice shows the preamp and 31 bands ({len(bands(a))} sliders)")
    p.note(MOCK_NOTE)
    band = a.find(role="Slider", label="Banda de 125 Hz")
    cx, cy = a.center(band)
    p.check(band["numeric_value"] == 0.0, "a band starts at 0 dB")
    a.drag(cx, cy, cx, cy - 50, steps=5)
    a.settle(0.4)
    v = a.find(role="Slider", label="Banda de 125 Hz")["numeric_value"]
    p.check(v > 1.0, f"dragging a band up raises it ({v} dB)")
    a.click_at(cx, cy, button="secondary")
    a.settle(0.4)
    p.check(a.find(role="Slider", label="Banda de 125 Hz")["numeric_value"] == 0.0, "a secondary click on the band resets it to 0 dB")
    a.drag(cx, cy, cx, cy + 50, steps=5)
    a.settle(0.4)
    v = a.find(role="Slider", label="Banda de 125 Hz")["numeric_value"]
    p.check(v < -1.0, f"dragging it down lowers it ({v} dB)")
    keep = band_values(a)
    # the load dialog: a preview, then Cancelar restores
    a.click(a.find(role="Button", label="Cargar preset"))
    a.settle(0.8)
    dlg = a.find(role="Dialog", label="Cargar Preset")
    p.check(dlg is not None, "'Cargar preset' opens the load dialog")
    presets = preset_buttons(a)
    p.check(len(presets) >= 10 and "Default" in presets, f"...listing the presets ({len(presets)}: {presets[:4]}...)")
    a.click(a.find(role="ListBoxOption", label="Hard Rock", pred=lambda n: n["bounds"]["x"] < 700))
    a.settle(0.6)
    preview = band_values(a)
    p.check(preview != keep, "picking a preset previews it on the sliders")
    a.shot("preview")
    a.click(a.find(role="Button", label="Cancelar"))
    a.settle(0.6)
    p.check(a.find(role="Dialog", label="Cargar Preset") is None and band_values(a) == keep, "Cancelar closes the dialog and restores the sliders")
    # Escape closes only the dialog
    a.click(a.find(role="Button", label="Cargar preset"))
    a.settle(0.6)
    a.click(a.find(role="ListBoxOption", label="Hard Rock", pred=lambda n: n["bounds"]["x"] < 700))
    a.settle(0.4)
    a.key("Escape")
    a.settle(0.6)
    p.check(a.find(role="Dialog", label="Cargar Preset") is None and ac_dialog(a) is not None and band_values(a) == keep,
            "Escape closes the load dialog (restoring the sliders) and leaves the Audio Center open")
    # Aceptar keeps it
    a.click(a.find(role="Button", label="Cargar preset"))
    a.settle(0.6)
    a.click(a.find(role="ListBoxOption", label="Hard Rock", pred=lambda n: n["bounds"]["x"] < 700))
    a.settle(0.4)
    a.click(a.find(role="Button", label="Aceptar"))
    a.settle(0.6)
    p.check(a.find(role="Dialog", label="Cargar Preset") is None and band_values(a) == preview, "Aceptar keeps the previewed preset")
    # the save dialog
    a.click(a.find(role="Button", label="Guardar preset"))
    a.settle(0.7)
    sd = [d for d in dialogs(a) if d["label"] != "Centro de Audio Avanzado"]
    p.check(sd, f"'Guardar preset' opens the save dialog {[d['label'] for d in sd]}")
    a.shot("save_dialog")
    inp = [n for n in a.nodes() if n["role"] == "TextInput" and n.get("focused")]
    p.check(inp, "...with its name field focused")
    if inp:
        a.type(inp[0], "Mi preset de prueba")
        a.key("Enter")
        a.settle(0.8)
    p.check(not [d for d in dialogs(a) if d["label"] != "Centro de Audio Avanzado"], "Enter in the name field saves and closes the dialog")
    a.click(a.find(role="Button", label="Cargar preset"))
    a.settle(0.7)
    p.check("Mi preset de prueba" in preset_buttons(a), f"the saved preset is in the load list ({preset_buttons(a)[-3:]})")
    a.key("Escape")
    a.settle(0.5)

    # ---- the effects
    a.click(a.find(role="Tab", label="Efectos de Audio"))
    a.settle(0.8)
    sw = a.find(role="Switch", label="Compresor")
    was = sw.get("toggled")
    a.click(sw)
    a.settle(0.5)
    p.check(a.find(role="Switch", label="Compresor").get("toggled") != was, "a pointer click on an effect's switch toggles it")
    sl = a.find(role="Slider", label="Compresor: Umbral dB")
    v0 = sl["numeric_value"]
    b = sl["bounds"]
    a.drag(b["x"] + b["width"] * 0.9, b["y"] + b["height"] / 2, b["x"] + b["width"] * 0.3, b["y"] + b["height"] / 2, steps=4)
    a.settle(0.4)
    v1 = a.find(role="Slider", label="Compresor: Umbral dB")["numeric_value"]
    p.check(v1 != v0, f"dragging an effect's slider changes its value ({v0} -> {v1})")

    # ---- volume and mix: steppers by click and by typing
    a.click(a.find(role="Tab", label="Volumen y Mezcla"))
    a.settle(0.8)

    def spin():
        return a.find(role="SpinButton", label="Silencio más corto eliminado")
    s = spin()
    b = s["bounds"]
    y = b["y"] + b["height"] / 2
    v0 = s["numeric_value"]
    a.click_at(b["x"] + b["width"] - 6, y)
    a.settle(0.4)
    v1 = spin()["numeric_value"]
    a.click_at(b["x"] + 6, y)
    a.settle(0.4)
    a.click_at(b["x"] + 6, y)
    a.settle(0.4)
    v2 = spin()["numeric_value"]
    p.check(v1 > v0 and v2 < v0, f"the stepper's + and - buttons answer a pointer click ({v0} -> {v1} -> {v2})")
    a.click_at(b["x"] + b["width"] / 2, y)
    a.settle(0.4)
    field = [n for n in a.nodes() if n["role"] == "TextInput" and n.get("focused")]
    p.check(field, "a click on the value turns it into a text field")
    if field:
        a.key("a", text="a", command=True)
        a.type(field[0], "2500")
        a.key("Enter")
        a.settle(0.5)
    p.check(spin()["numeric_value"] == 2500.0, f"typing a value and pressing Enter sets it ({spin()['numeric_value']})")
    cbx = a.find(role="CheckBox", label="Desvanecimiento de Volumen")
    sub = a.find(role="CheckBox", label="Suavizar el cambio de volumen")
    p.check(sub.get("disabled"), "a dependent option is disabled while its parent is off")
    a.click(cbx)
    a.settle(0.5)
    p.check(a.find(role="CheckBox", label="Desvanecimiento de Volumen").get("toggled") == "true" and not a.find(role="CheckBox", label="Suavizar el cambio de volumen").get("disabled"),
            "a pointer click on the check box turns it on and enables its dependent options")

    # ---- Escape closes the innermost layer first
    a.click(a.find(role="Tab", label="Ecualizador"))
    a.settle(0.6)
    a.click(a.find(role="Button", label="Cargar preset"))
    a.settle(0.6)
    a.key("Escape")
    a.settle(0.5)
    p.check(ac_dialog(a) is not None and a.find(role="Dialog", label="Cargar Preset") is None, "Escape closes the load dialog first")
    a.key("Escape")
    a.settle(0.6)
    p.check(ac_dialog(a) is None, "a second Escape closes the Audio Center")
    no_hazards(p, a)
    clean_log(p, a)


run("p07_audio_center", body)
