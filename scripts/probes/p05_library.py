"""Library: view modes, the grid's open album and its keys, sort, column resize, the rubber band, the delete
confirmation, Ctrl+A, "Mostrar lo que suena"."""
from lib import *

HAZARD_FINDING = ("the debug overflow overlay (yellow hazard stripes) is painted over the cover thumbnails of the song rows "
                  "(and over the whole song area of the first list mode): their container is laid out narrower than its content")
RESIZE_FINDING = ("dragging a column grip with the pointer moving in small steps resizes at half the pointer's speed "
                  "(+50 px of width for 100 px of travel in ten 10 px moves; a single 100 px jump is exact)")


def cells(a):
    return a.find_all(role="GridCell")


def sel_cells(a):
    return [n for n in cells(a) if n.get("selected")]


def song_rows(a):
    return [n for n in a.find_all(role="Row") if n["bounds"]["x"] >= 600 and n["bounds"]["y"] >= 60 and n["label"] != "Encabezados de columna"]


def status(a):
    t = a.label_texts((800, 960, 1450, 1000))
    return t[0] if t else ""


def mode(a):
    if a.find(role="Grid", label="Álbumes"):
        return "grid"
    if a.find(role="ListBox", label="Canciones"):
        return "list"
    return None


def album_names(a):
    return [n["label"].split(" — ", 1)[-1] for n in cells(a)]


def body(p):
    a = start(p)
    # ---- the view modes
    seen = []
    for i in range(5):
        a.settle(0.5)
        a.shot(f"mode{i}")
        seen.append(mode(a))
        if i < 4:
            a.click(a.find(role="Button", label="Cambiar de vista"))
            a.settle(1.0)
    p.check(seen == ["grid", "list", "list", "list", "grid"], f"the view button cycles grid -> three list modes -> grid: {seen}")
    p.check(a.find(role="Grid", label="Álbumes") and len(cells(a)) > 5, "back in the grid, the album cards are there")
    p.check(a.hazards["mode0"] < 50, "the grid mode shows no overflow overlay")
    p.check(all(a.hazards[f"mode{i}"] < 50 for i in (1, 2, 3)), f"the three list modes show no overflow overlay "
            f"({[a.hazards[f'mode{i}'] for i in (1, 2, 3)]} stripe pixels)", known_failure=HAZARD_FINDING)

    # ---- the grid's open album: the chevron, a double click
    chev = a.find(role="Button", label="Abrir o cerrar Piano Sonatas")
    a.click(chev)
    a.settle(1.0)
    anna = a.find(role="GridCell", label_contains="Anna Keller")
    p.check(anna.get("expanded") and anna.get("selected"), "a pointer click on a card's chevron opens the album and selects the card")
    rows = song_rows(a)
    p.check([r["label"] for r in rows] == ["Sonata in C, I. Allegro", "Sonata in C, II. Adagio"], f"the open album shows its songs: {[r['label'] for r in rows]}")
    a.shot("album_open")
    a.click(a.find(role="Button", label="Abrir o cerrar Piano Sonatas"))
    a.settle(1.0)
    anna = a.find(role="GridCell", label_contains="Anna Keller")
    p.check(not anna.get("expanded") and not song_rows(a), "a second click on the chevron closes the album")
    a.click(a.find(role="GridCell", label_contains="Bleak Harbour"), double=True)
    a.settle(1.2)
    bleak = a.find(role="GridCell", label_contains="Bleak Harbour")
    p.check(bleak.get("expanded"), "a double click on a card opens its album")
    a.click(a.find(role="GridCell", label_contains="Bleak Harbour"), double=True)
    a.settle(1.2)
    # ---- keys: Down enters the open album's songs, Up leaves them
    a.click(a.find(role="GridCell", label_contains="Anna Keller"))
    a.settle(0.4)
    a.key("ArrowDown", alt=True)
    a.settle(0.8)
    p.check(a.find(role="GridCell", label_contains="Anna Keller").get("expanded"), "Alt+Down opens the selected album")
    a.key("ArrowDown")
    a.settle(0.5)
    r = [x["label"] for x in song_rows(a) if x.get("selected")]
    p.check(r == ["Sonata in C, I. Allegro"], f"Down from the open album's card enters its first song: {r}")
    a.key("ArrowDown")
    a.settle(0.5)
    r = [x["label"] for x in song_rows(a) if x.get("selected")]
    p.check(r == ["Sonata in C, II. Adagio"], f"Down walks to the next song: {r}")
    a.key("ArrowDown")
    a.settle(0.5)
    r = [x["label"] for x in song_rows(a) if x.get("selected")]
    p.check(r == ["Sonata in C, II. Adagio"], "Down stops at the last song")
    a.key("ArrowUp")
    a.key("ArrowUp")
    a.settle(0.5)
    p.check(a.find(role="GridCell", label_contains="Anna Keller").get("selected") and not [x for x in song_rows(a) if x.get("selected")],
            "Up from the first song returns to the album's card")
    a.key("ArrowUp")
    a.settle(0.6)
    p.check(not a.find(role="GridCell", label_contains="Anna Keller").get("expanded"), "Up on the open album's card closes it")

    # ---- sort by a column header: three-state cycle
    orig = album_names(a)
    keyf = lambda s: s.casefold()
    a.click(a.find(role="ColumnHeader", label="Álbum"))
    a.settle(1.0)
    asc = album_names(a)
    p.check(asc == sorted(asc, key=keyf) and asc != orig, f"a first click on a header sorts ascending: {asc[:3]}")
    a.click(a.find(role="ColumnHeader", label="Álbum"))
    a.settle(1.0)
    desc = album_names(a)
    p.check(desc == sorted(desc, key=keyf, reverse=True) and desc != asc, f"a second click sorts descending: {desc[:3]}")
    a.click(a.find(role="ColumnHeader", label="Álbum"))
    a.settle(1.0)
    p.check(album_names(a) == orig, "a third click clears the sort (the database order is back)")

    # ---- column resize by dragging a grip, and the grip turning accent on hover
    def width():
        return a.find(role="ColumnHeader", label="Título")["bounds"]["width"]
    g = a.find(role="Splitter", label="Redimensionar Título")
    gx, gy = a.center(g)
    a.park()
    a.settle(0.3)
    pa, _ = a.shot("grip_idle")
    a.move(gx, gy)
    a.settle(0.3)
    pb, _ = a.shot("grip_hover")
    ca, cb = a.pixel(pa, gx, gy), a.pixel(pb, gx, gy)
    p.check(ca[0] - ca[2] < 10 and cb[0] - cb[1] > 30, f"hovering a grip turns it accent (grey {ca} -> {cb})")
    a.park()
    a.settle(0.3)
    pc, _ = a.shot("grip_away")
    p.check(a.pixel(pc, gx, gy) == ca, "moving away turns it back")
    w0 = width()
    a.move(gx, gy)
    a.call("inject_pointer", action="down", x=gx, y=gy, settle=SETTLE)
    for i in range(1, 11):
        a.move(gx + 10 * i, gy)
    a.call("inject_pointer", action="up", x=gx + 100, y=gy, settle=SETTLE)
    a.settle(0.4)
    grown = width() - w0
    p.check(grown > 0, f"dragging a grip to the right widens the column ({grown:+.0f} px)")
    p.check(abs(grown - 100) <= 3, f"...by the distance the pointer travelled ({grown:+.0f} px for +100)", known_failure=RESIZE_FINDING)
    g = a.find(role="Splitter", label="Redimensionar Título")
    gx, gy = a.center(g)
    w1 = width()
    a.drag(gx, gy, gx - 40, gy, steps=1)
    a.settle(0.4)
    p.check(abs((w1 - width()) - 40) <= 3, f"a single 40 px jump to the left narrows it by 40 ({w1 - width():.0f})")

    # ---- the rubber band in the grid
    a.drag(803, 330, 1300, 120, steps=8)
    a.settle(0.5)
    sel = [n["label"] for n in sel_cells(a)]
    p.check(len(sel) >= 5, f"a rubber band across the grid selects the cards it covers ({len(sel)})")
    p.check(not any("Anna Keller" in s for s in sel), "...and not the card it does not touch")
    a.shot("band_grid")

    # ---- Ctrl+A, then the delete confirmation
    a.click(a.find(role="GridCell", label_contains="Anna Keller"))
    a.settle(0.4)
    a.key("a", text="a", command=True)
    a.settle(0.6)
    p.check(len(sel_cells(a)) >= 20, f"Ctrl+A selects every card ({len(sel_cells(a))} visible ones)")
    a.key("Delete")
    a.settle(0.6)
    d = dialogs(a)
    p.check(d and d[0]["label"] == "¿Eliminar de la biblioteca?", f"Delete asks for a confirmation: {[x['label'] for x in d]}")
    a.shot("delete_confirm")
    a.key("Escape")
    a.settle(0.5)
    p.check(not dialogs(a) and len(sel_cells(a)) >= 20 and status(a).startswith("194 Canciones"),
            "Escape cancels it: nothing is deleted and the selection stays")
    a.click_at(1000, 330)
    a.settle(0.3)
    a.key("Delete")
    a.settle(0.5)
    a.click(a.find(role="Button", label="Cancelar"))
    a.settle(0.5)
    p.check(not dialogs(a) and a.find(role="GridCell", label_contains="Anna Keller"), "the Cancelar button cancels it too, the album is still there")

    # ---- Mostrar lo que suena
    row = a.find(role="ListItem", label_contains="1. Watch That Man")
    a.click(row, double=True)
    a.settle(1.5)
    a.click(a.find(role="Button", label="Mostrar lo que suena"))
    a.settle(1.2)
    opened = [n["label"] for n in cells(a) if n.get("expanded")]
    p.check(len(opened) == 1 and "Aladdin Sane" in opened[0], f"'Mostrar lo que suena' opens the playing song's album: {opened}")
    p.check(any("Watch That Man" in r["label"] for r in song_rows(a)), "...and its songs are listed")

    # ---- a rubber band in a list mode
    for _ in range(3):
        a.click(a.find(role="Button", label="Cambiar de vista"))
        a.settle(1.0)
    p.check(mode(a) == "list", "(third list mode)")
    rows = song_rows(a)
    r0, r1 = rows[3], rows[6]
    a.drag(1400, r0["bounds"]["y"] + 6, 1400, r1["bounds"]["y"] + 6, steps=6)
    a.settle(0.5)
    selr = [r["label"] for r in song_rows(a) if r.get("selected")]
    p.check(len(selr) >= 3, f"a rubber band in a list mode selects the rows it covers ({len(selr)})")
    a.shot("band_list")


run("p05_library", body)
