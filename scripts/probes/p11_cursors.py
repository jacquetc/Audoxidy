"""Cursors (quirk fix 63): hover a button (hand), a disabled entry (arrow), a column grip (resize).

The automation bridge of teksilo 0.16.0 has 36 tools and none of them reports the mouse cursor: `query_pointers`
gives id, kind, position, down and touch_action; `inspect_node` and `read_node` give no cursor either, and a
screenshot is rendered off-screen without one.  The probe checks that this is still so and skips; the day a tool
reports the cursor, the three checks go here."""
import json

from lib import *


def has_cursor(obj) -> bool:
    return "cursor" in json.dumps(obj).lower()


def body(p):
    a = start(p)
    button = a.find(role="Button", label="Aleatorio")
    a.move(*a.center(button))
    a.settle(0.3)
    pointers = a.call("query_pointers")
    inspected = a.call("inspect_node", node=button["id"])
    read = a.call("read_node", node=button["id"])
    r = a.session._send("tools/list", {})
    tools = a.session._recv(r, 10)["result"]["tools"]
    named = [t["name"] for t in tools if "cursor" in (t["name"] + t.get("description", "")).lower() and t["name"] not in ("cancel_pointer",)]
    if not (has_cursor(pointers) or has_cursor(inspected) or has_cursor(read) or named):
        p.skip(f"no automation tool reports the mouse cursor ({len(tools)} tools; query_pointers keys: {sorted(pointers[0])}); "
               "hand over a button, arrow over a disabled entry and resize over a column grip cannot be checked")
        return
    p.check(False, f"a cursor report appeared ({named}); write the three hover checks in this probe")


run("p11_cursors", body)
