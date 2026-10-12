"""Playlist: tabs and their overflow list, the row menu and its send-to submenu, the create / rename /
delete dialogs, search, Ctrl+A in the list."""
from lib import *

STRIP = (0, 400, 400, 440)


def tab_names(a):
    return [n["label"] for n in a.find_all(role="Tab", pred=lambda n: 395 <= n["bounds"]["y"] < 445 and n["bounds"]["x"] < 420)]


def selected_tab(a):
    t = a.find(role="Tab", pred=lambda n: 395 <= n["bounds"]["y"] < 445 and n["bounds"]["x"] < 420 + 2000 and n.get("selected"))
    return t and t["label"]


def rows(a):
    return [n for n in a.find_all(role="ListItem") if n["bounds"]["x"] < 400 and n["bounds"]["y"] >= 440]


def song_rows(a):
    return [n for n in rows(a) if n["label"][:1].isdigit()]


def menu_labels(a):
    return [n["label"] for n in a.find_all(role="MenuItem")]


def focused_input(a):
    r = [n for n in a.nodes() if n["role"] == "TextInput" and n.get("focused")]
    return r[0] if r else None


def reveal(a, name):
    """Make a tab reachable: pick it in the chevron's list (which switches to it and scrolls it into view)."""
    a.click(a.find(role="Button", label="Todas las listas"))
    a.settle(0.5)
    a.click(a.find(role="MenuItem", label=name))
    a.settle(1.0)


def in_strip(n):
    return 12 <= n["bounds"]["x"] and n["bounds"]["x"] + n["bounds"]["width"] <= 364


SCROLL_FINDING = ("creating a list activates it but leaves its tab out of the strip's view; Iced scrolls the new active tab "
                  "into view (app.rs:2636-2660, RefreshPlaylists -> SwitchPlaylist)")
SUBMENU_FINDING = ("a pointer click (down/up, click, hover path first, or node-local) on an item of the 'Enviar a otra lista' submenu does nothing: "
                   "the submenu closes, the parent menu stays open and no song is sent; invoke_action click on the same item works")
STALE_FINDING = ("after a pick in the overflow list that scrolls the strip, the newly active Tab keeps selected=false in the "
                 "accessibility tree (and the old one selected=true) although the list switched on screen; it corrects itself at the next tab click")


def create_list(a, name):
    a.click_at(340, 420, button="secondary")
    a.settle(0.4)
    a.click(a.find(role="MenuItem", label="Nueva lista"))
    a.settle(0.5)
    inp = focused_input(a)
    a.type(inp, name)
    return inp


def body(p):
    a = start(p)
    names = tab_names(a)
    p.check(names == ["Audoxidy", "Oxidy Drift", "David Bowie"] and selected_tab(a) == "David Bowie",
            f"three seeded lists, David Bowie active: {names}")
    # ---- switching tabs with the pointer
    n0 = len(song_rows(a))
    a.click(a.find(role="Tab", label="Oxidy Drift"))
    a.settle(1.0)
    p.check(selected_tab(a) == "Oxidy Drift", "a pointer click on a tab selects it")
    p.check(len(song_rows(a)) == 0, f"...and the list shows that list's songs (Oxidy Drift is empty, {len(song_rows(a))} rows)")
    a.click(a.find(role="Tab", label="David Bowie"))
    a.settle(1.0)
    p.check(selected_tab(a) == "David Bowie" and len(song_rows(a)) == n0, "back to David Bowie")

    # ---- the create dialog: type a name, press Enter
    inp = create_list(a, "Mi lista")
    d = dialogs(a)
    p.check(d and d[0]["label"] == "Nueva lista de reproducción", f"'Nueva lista' opens the create dialog {[x['label'] for x in d]}")
    p.check(field_text(focused_input(a)) == "Mi lista", "the dialog's field has the keyboard focus and holds the typed name")
    a.key("Enter")
    a.settle(1.0)
    p.check(not dialogs(a), "Enter closes the dialog")
    p.check("Mi lista" in tab_names(a), f"Enter created the list: {tab_names(a)}")
    p.check(selected_tab(a) == "Mi lista", "...and it is the active list")
    # an empty name created by the Crear button? (Cancelar leaves nothing)
    create_list(a, "Cancelada")
    a.click(a.find(role="Button", label="Cancelar"))
    a.settle(0.6)
    p.check(not dialogs(a) and "Cancelada" not in tab_names(a), "Cancelar closes the dialog and creates nothing")
    create_list(a, "Con boton")
    a.click(a.find(role="Button", label="Crear"))
    a.settle(1.0)
    p.check("Con boton" in tab_names(a), "the Crear button creates the list")
    create_list(a, "Escapada")
    a.key("Escape")
    a.settle(0.6)
    p.check(not dialogs(a) and "Escapada" not in tab_names(a), "Escape closes the create dialog and creates nothing")

    # ---- the overflow chevron
    for i in range(3):
        create_list(a, f"Lista de relleno {i}")
        a.key("Enter")
        a.settle(0.8)
    act = a.find(role="Tab", pred=lambda n: n.get("selected") and 395 <= n["bounds"]["y"] < 445)
    p.check(act and in_strip(act), f"after creating lists the active tab is in view ({act and act['label']} at x={act and act['bounds']['x']:.0f})",
            known_failure=SCROLL_FINDING)
    chev = a.find(role="Button", label="Todas las listas")
    p.check(chev is not None, "a chevron 'Todas las listas' appears once the tabs overflow")
    if chev:
        a.click(chev)
        a.settle(0.6)
        items = menu_labels(a)
        p.check(set(tab_names(a)) <= set(items) and len(items) >= 7, f"the chevron lists every list: {items}")
        a.shot("overflow")
        target = a.find(role="MenuItem", label="Oxidy Drift")
        a.click(target)
        a.settle(1.0)
        p.check(not a.find(role="Menu") and a.find(role="Label", pred=lambda n: "No hay canciones" in field_text(n) or n.get("value") == "No hay canciones en esta lista"),
                "picking one from the list switches to it (the empty Oxidy Drift shows its placeholder) and closes the menu")
        p.check(selected_tab(a) == "Oxidy Drift", "...and its tab is the selected one in the accessibility tree", known_failure=STALE_FINDING)
        a.click(a.find(role="Tab", label="Oxidy Drift"))
        a.settle(0.6)
        p.check(selected_tab(a) == "Oxidy Drift", "...(a direct click on the tab does select it)")
        a.click(chev)
        a.settle(0.4)
        a.key("Escape")
        a.settle(0.3)
        p.check(not a.find(role="Menu"), "Escape closes the overflow list")

    # ---- rename
    reveal(a, "Mi lista")
    tab = a.find(role="Tab", label="Mi lista")
    a.click(tab, button="secondary")
    a.settle(0.5)
    a.click(a.find(role="MenuItem", label="Renombrar lista"))
    a.settle(0.6)
    d = dialogs(a)
    p.check(d, f"'Renombrar lista' opens a dialog {[x['label'] for x in d]}")
    inp = focused_input(a)
    p.check(inp and field_text(inp) == "Mi lista", f"the rename field holds the current name {field_text(inp) if inp else None!r}")
    a.key("a", text="a", command=True)
    a.type(inp, "Renombrada")
    a.key("Enter")
    a.settle(1.0)
    p.check("Renombrada" in tab_names(a) and "Mi lista" not in tab_names(a), f"Enter renames the list: {tab_names(a)}")

    # ---- delete with a confirmation
    reveal(a, "Renombrada")
    tab = a.find(role="Tab", label="Renombrada")
    a.click(tab, button="secondary")
    a.settle(0.5)
    a.click(a.find(role="MenuItem", label="Eliminar lista"))
    a.settle(0.6)
    d = dialogs(a)
    p.check(d, f"'Eliminar lista' asks for a confirmation {[(x['role'], x['label']) for x in d]}")
    a.shot("delete_confirm")
    a.key("Escape")
    a.settle(0.6)
    p.check(not dialogs(a) and "Renombrada" in tab_names(a), "Escape cancels the deletion")
    a.click(a.find(role="Tab", label="Renombrada"), button="secondary")
    a.settle(0.5)
    a.click(a.find(role="MenuItem", label="Eliminar lista"))
    a.settle(0.6)
    yes = a.find(role="Button", label="Eliminar") or a.find(role="Button", label="Aceptar") or a.find(role="Button", label="Sí")
    if yes:
        a.click(yes)
        a.settle(1.0)
    p.check(yes and "Renombrada" not in tab_names(a), f"confirming deletes the list: {tab_names(a)}")

    # ---- the row menu and the send-to submenu
    reveal(a, "David Bowie")
    a.settle(1.0)
    row = song_rows(a)[0]
    a.click(row, button="secondary")
    a.settle(0.6)
    items = menu_labels(a)
    p.check("Enviar a otra lista" in items, f"the row menu opens on a right click: {items}")
    for stub in ("Información", "Editor de Etiquetas", "Convertidor de audio"):
        it = a.find(role="MenuItem", label=stub)
        p.check(it and it.get("disabled"), f"row menu entry {stub!r} is disabled (stub)")
    a.click(a.find(role="MenuItem", label="Enviar a otra lista"))
    a.settle(0.6)
    sub = a.find(role="Menu", label="Enviar a otra lista")
    p.check(sub is not None, "'Enviar a otra lista' opens a submenu")
    if sub:
        sb = sub["bounds"]
        subitems = [n["label"] for n in a.find_all(role="MenuItem") if n["bounds"]["x"] >= sb["x"] and n["bounds"]["x"] < sb["x"] + sb["width"]]
        p.check("David Bowie" not in subitems, f"the active list is left out of the submenu: {subitems}")
        p.check("Nueva lista" in subitems and "Audoxidy" in subitems and "Oxidy Drift" in subitems, "...the others are there")
    a.key("Escape")
    a.settle(0.4)
    a.key("Escape")
    a.settle(0.4)
    p.check(not a.find(role="Menu"), "Escape closes the menus")
    # send a song to Oxidy Drift (empty): the pointer route first, then the assistive route
    reveal(a, "Oxidy Drift")
    before = len(rows(a))
    p.check(before == 0, "Oxidy Drift is empty")
    reveal(a, "David Bowie")

    def open_submenu():
        a.click(song_rows(a)[0], button="secondary")
        a.settle(0.5)
        a.click(a.find(role="MenuItem", label="Enviar a otra lista"))
        a.settle(0.5)
        sub = a.find(role="Menu", label="Enviar a otra lista")
        sb = sub["bounds"]
        return a.find(role="MenuItem", label="Oxidy Drift", pred=lambda n: n["bounds"]["x"] >= sb["x"])

    def drift_rows():
        reveal(a, "Oxidy Drift")
        n = len(song_rows(a))
        reveal(a, "David Bowie")
        return n

    it = open_submenu()
    a.click(it)
    a.settle(1.0)
    n_ptr = drift_rows()
    p.check(n_ptr == 1, f"a pointer click on a submenu item sends the song ({n_ptr} song rows in Oxidy Drift)", known_failure=SUBMENU_FINDING)
    if a.find(role="Menu"):
        a.key("Escape")
        a.key("Escape")
        a.settle(0.4)
    it = open_submenu()
    a.call("invoke_action", node=it["id"], action="click", settle=SETTLE)
    a.settle(1.0)
    n_at = drift_rows()
    p.check(n_at >= 1 and not a.find(role="Menu"), f"the assistive click on the same item sends the song and closes the menus ({n_at} song rows)")

    # ---- search
    reveal(a, "David Bowie")
    total = len(song_rows(a))
    box = a.find(role="TextInput", label="Buscar...", pred=lambda n: n["bounds"]["x"] < 100)
    a.click(box)
    a.type(box, "Panic")
    a.settle(1.0)
    found = song_rows(a)
    p.check(0 < len(found) < total and all("Panic" in r["label"] for r in found), f"typing in the search filters the list ({total} -> {len(found)})")
    a.key("Escape")
    a.settle(1.0)
    p.check(len(song_rows(a)) == total, "Escape clears the search and restores the list")

    # ---- Ctrl+A in the list
    a.click(song_rows(a)[1])
    a.settle(0.4)
    sel = [r for r in song_rows(a) if r.get("selected")]
    p.check(len(sel) == 1, "a click selects one row")
    a.key("a", text="a", command=True)
    a.settle(0.6)
    sel = [r for r in song_rows(a) if r.get("selected")]
    p.check(len(sel) == len(song_rows(a)) > 1, f"Ctrl+A selects every row of the list ({len(sel)}/{len(song_rows(a))})")

    no_hazards(p, a)
    clean_log(p, a)


run("p04_playlist", body)
