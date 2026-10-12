# Live probes of the Teksilo UI

Python probes that start the real app, drive it through teksilo's automation bridge (`scripts/teksilo_probe`,
written by `cargo teksilo probe install`), and assert on the accessibility tree, the layout tree, the overlays and
screenshots.  They need a graphical session (the app opens a real window).

## Run them

```sh
# 1. the mocks binary: a deterministic mock library, no database file, no audio device
cargo build --no-default-features --features teksilo-mocks --bin audoxidy-teksilo --target-dir target/mocks

# 2. the 0.16.0 automation client (the one on $PATH is 0.15.0 and cannot speak to the app)
cargo install teksilo-automation-mcp --version 0.16.0 --locked --root <a dir of yours>   # never ~/.cargo/bin
export TEKSILO_MCP_BIN=<that dir>/bin/teksilo-automation-mcp

# 3. every probe, one fresh app and a fresh isolated home each; prints a table
scripts/probes/run_all.py            # --strict also fails on an XPASS; names filter: run_all.py p05 p07
scripts/probes/p05_library.py        # or one probe alone
```

`run_all.py` builds nothing.  Environment: `AUDOXIDY_PROBE_APP` (the binary, default `target/mocks/debug/audoxidy-teksilo`),
`TEKSILO_MCP_BIN`, `AUDOXIDY_PROBE_OUT` (screenshots and app logs, default `$TMPDIR/audoxidy-probes/<probe>/`),
`AUDOXIDY_PROBE_LANG` (default `es_ES.UTF-8`: the strings the probes look up are Spanish).

Each app gets `HOME`, `XDG_CONFIG_HOME`, `XDG_DATA_HOME` and `XDG_STATE_HOME` in a fresh temp directory (deleted
afterwards), so it never reads or writes the user's settings.  A probe stops its app by pid on every exit path.

## Results

A probe exits 0 (pass, possibly with known failures), 2 (a check failed), 1 (it could not run), 3 (XPASS under
`--strict`).  `run_all.py` prints `PASS*` when a probe passes with known failures.

A **known failure** is a check written `p.check(cond, msg, known_failure="<the finding in one line>")`: the app is
wrong, the probe says so and does not fail; it prints `XFAIL`.  When the app is fixed the check passes and prints
`XPASS`: delete the `known_failure=` argument.  `docs/` does not list them; the one-line description is the record.

## The probes

| probe | covers |
|---|---|
| `p01_title_bar` | source pills (streaming ones disabled), maximize/restore and the control's name, minimize, close exits 0 |
| `p02_window_geometry` | `window_state.toml`: floating and maximized placement across close and reopen, narrow windows |
| `p03_player` | menu (pointer open, disabled stubs, Escape), the three zones, wheel volume bubble, seek bar |
| `p04_playlist` | tabs, chevron overflow list, create / rename / delete dialogs, row menu and send-to submenu, search, Ctrl+A |
| `p05_library` | the four view modes, the grid's open album and its keys, sort, column resize and grip hover, rubber bands, delete confirmation, Ctrl+A, "Mostrar lo que suena" |
| `p06_filters` | the five filter types, letters, tree nodes, restore icon, filter search; library and tree back at the top |
| `p07_audio_center` | opening on each tab, header drag, pick lists, equalizer (switch, 20/31 bands, drag, secondary-click reset, load preview + Cancel, save), effects, steppers by click and typing, Escape order |
| `p08_vu_meter` | right-click popup anchored above the slot, enabling the meter, popup survives redraws, Escape and outside click |
| `p09_text_fields` | in every text field, space / Enter / Delete / arrows edit the text and never act on a panel |
| `p10_every_button` | a pointer press on every visible button, tab, header, menu entry, AC control; the screen must change |
| `p11_cursors` | skipped: no automation tool reports the mouse cursor |

Every probe also checks that no screenshot shows the debug inspector's overflow overlay (yellow hazard stripes) and
that the app's log has no warning.

## Gaps in the automation tools (what the probes cannot see)

* No tool reports the mouse cursor (`p11_cursors`).
* No tool reports whether the window is minimized, nor its position (Wayland: a client cannot be moved).
* The filter tree's `TreeItem` rows are named by their labels (Wave F fix), so a probe can pick a row by its label.
* Screenshots are physical pixels: on a scaled display (1.2 here) the probes convert (`App.scale`).
* The mocks' fake engine never writes meter levels and gives bands beyond the 20th the name "Banda de 0 Hz".
* The display the machine has when the probes run decides the maximized size; the probes read it, never assume it.
