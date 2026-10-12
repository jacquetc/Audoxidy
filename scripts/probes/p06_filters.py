"""Filters: the type selector, a letter, a tree node filtering the library, the restore icon, the filter search;
the library and the tree are back at their top after each."""
from lib import *

NAMELESS_FINDING = ("the filter tree's rows (role TreeItem) have no accessible name and no children in the accessibility tree: "
                    "the text drawn in a row is not exposed (a screen reader reads an unnamed tree item)")
TYPES = ["Carpeta", "Artista", "Álbum", "Género", "Año"]


def titems(a):
    return [n for n in a.nodes() if n["role"] == "TreeItem"]


def tree_top(a):
    """Is the first realised row of the tree at the tree's own top edge (offset 0)?"""
    t = a.find(role="Tree", label="Filtros")
    items = titems(a)
    return bool(t and items and abs(items[0]["bounds"]["y"] - t["bounds"]["y"]) < 1.0)


def grid_top(a):
    g = a.find(role="Grid", label="Álbumes")
    cs = a.find_all(role="GridCell")
    return bool(g and cs and abs(cs[0]["bounds"]["y"] - g["bounds"]["y"]) < 1.0)


def status(a):
    t = a.label_texts((800, 960, 1450, 1000))
    return t[0] if t else ""


def scroll_both(a):
    t = a.find(role="Tree", label="Filtros")
    g = a.find(role="Grid", label="Álbumes")
    a.call("scroll", node=t["id"], dy=40.0, lines=True, settle=SETTLE)
    a.call("scroll", node=g["id"], dy=40.0, lines=True, settle=SETTLE)
    a.settle(0.4)


def pick_type(a, name):
    a.click(a.find(role="Button", label="Tipo de filtro"))
    a.settle(0.4)
    a.click(a.find(role="MenuItem", label=name))
    a.settle(1.0)


def restore(a):
    a.click(a.find(role="Button", label="Mostrar todo"))
    a.settle(1.0)


def body(p):
    a = start(p)
    ALL = "194 Canciones | 49 Álbumes | 30 Artistas | 13:47:47 | 5.74 GB"
    p.check(tree_top(a) and grid_top(a) and status(a) == ALL, "at start the tree and the library are at their top and unfiltered")
    p.check(all(not n.get("label") for n in titems(a)), "the tree's rows are unnamed in the accessibility tree", known_failure=NAMELESS_FINDING) \
        if False else p.check(all(n.get("label") for n in titems(a)), "every tree row has an accessible name", known_failure=NAMELESS_FINDING)

    # ---- the selector
    a.click(a.find(role="Button", label="Tipo de filtro"))
    a.settle(0.5)
    items = [n["label"] for n in a.find_all(role="MenuItem")]
    p.check(items == TYPES, f"the selector opens a menu of the five types: {items}")
    menu = a.find(role="Menu")
    p.check(menu and abs(menu["bounds"]["x"] - 400) < 30 and menu["bounds"]["y"] < 120, "...anchored under the selector")
    a.key("Escape")
    a.settle(0.4)
    p.check(not a.find(role="Menu"), "Escape closes it")
    sigs = {}
    for t in TYPES + ["Género"]:
        scroll_both(a)
        pick_type(a, t)
        p.check(tree_top(a) and grid_top(a), f"type {t}: the tree and the library are back at their top")
        a.park()
        a.settle(0.3)
        path, _ = a.shot(f"type_{t}")
        from PIL import Image
        sigs[t] = path
        p.check(a.find_all(role="TreeItem"), f"type {t}: the tree has rows ({len(titems(a))})")
    names = ["Carpeta", "Artista", "Álbum", "Género", "Año"]
    diffs = [a.crop_diff(sigs[x], sigs[y], (400, 126, 600, 960)) for x, y in zip(names, names[1:])]
    p.check(all(d > 500 for d in diffs), f"every type draws a different tree (pixel differences between neighbours: {diffs})")

    # ---- a letter
    restore(a)
    n_all = len(titems(a))
    scroll_both(a)
    a.click(a.find(role="Button", label="B"))
    a.settle(1.0)
    p.check(len(titems(a)) < n_all and tree_top(a) and grid_top(a),
            f"a letter narrows the tree ({n_all} -> {len(titems(a))}) and both are at their top")
    p.check(a.find(role="Button", label="B").get("selected"), "...the letter shows as selected")
    a.click(a.find(role="Button", label="Mostrar todo"))
    a.settle(1.0)
    p.check(len(titems(a)) == n_all and not a.find(role="Button", label="B").get("selected"), "the restore icon brings the whole tree back")

    # ---- a tree node filters the library
    scroll_both(a)
    node = [n for n in titems(a) if 300 < n["bounds"]["y"] < 800][0]
    a.click(node)
    a.settle(1.0)
    p.check(status(a) != ALL and grid_top(a), f"a tree node filters the library ({status(a)}) and the library is at its top (the tree stays where the user clicked)")
    p.check(any(n.get("selected") for n in titems(a)), "...the node shows as selected")
    a.shot("node_filtered")
    a.click(a.find(role="Button", label="Mostrar todo"))
    a.settle(1.0)
    p.check(status(a) == ALL and not any(n.get("selected") for n in titems(a)), "the restore icon clears the node filter: the library is whole again")
    p.check(grid_top(a) and tree_top(a), "...and both are at their top")

    # ---- the filter search
    box = a.find(role="TextInput", label="Buscar...", pred=lambda n: 400 < n["bounds"]["x"] < 600)
    scroll_both(a)
    a.click(box)
    a.type(box, "Rock")
    a.settle(1.2)
    n_found = len(titems(a))
    p.check(0 < n_found < n_all and tree_top(a), f"typing in the filter search narrows the tree ({n_all} -> {n_found}) and it is at its top")
    a.key("Escape")
    a.settle(1.0)
    p.check(len(titems(a)) == n_all, "Escape clears the filter search and the tree is whole again")
    no_hazards(p, a)


run("p06_filters", body)
