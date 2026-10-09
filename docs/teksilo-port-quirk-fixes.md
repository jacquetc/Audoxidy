# Teksilo UI: behaviour that differs from the Iced UI on purpose

The Teksilo UI (`teksilo-ui` feature, binary `audoxidy-teksilo`) is a port of the Iced UI with
strict feature parity. This file lists the places where it deliberately behaves differently:
each one is an Iced behaviour that looked accidental, fixed in the Teksilo UI only. The Iced UI
is unchanged.

Every entry gives the Iced behaviour with its source location, the Teksilo behaviour, the
reason, and the test or probe that pins the new behaviour. A fix does not land without its
entry, and an entry is not complete until its **Pinned by** line names a test that exists.

Unfinished Iced features (stubs) are not listed here; they are in
[`teksilo-port-stubs.md`](teksilo-port-stubs.md).

| # | Fix |
| --- | --- |
| 1 | Deleting from the library asks for confirmation |
| 2 | Enter, Delete, Space and Ctrl+A act on the focused panel |
| 3 | Closing the window saves state and stops cleanly |
| 4 | Escape closes overlays and dialogs |
| 5 | No blocking work on the UI thread |
| 6 | The session is restored once at startup |
| 7 | Popups open at their trigger |
| 8 | Failed user actions show an error toast |
| 9 | Downmix settings apply at startup |
| 10 | The VU meter can be enabled on a fresh install; its popup stays open |
| 11 | The playing row is tracked by song id |
| 12 | Changing a playlist's content does not reload the playing track |
| 13 | The playlist position is saved on a real 20 s interval |
| 14 | Hidden built-in EQ presets stay hidden across restarts |
| 15 | Spanish plurals and typos are corrected |
| 16 | "Eliminar lista" is disabled on system playlists |

---

## 1. Deleting from the library asks for confirmation

- **Iced:** `LibraryDeleteSelection` (`src/gui/app.rs:3972`) removes the selected songs from the
  database immediately, from the Delete key or the action, with no confirmation.
- **Teksilo:** a `MessageBox` names what will be removed and asks for confirmation. Only
  "Eliminar" runs the deletion; Escape and "Cancelar" leave the library untouched.
- **Why:** it is destructive and cannot be undone. Removing playlist entries (not songs) keeps
  Iced's behaviour.
- **Pinned by:** to be filled by the library wave.

## 2. Enter, Delete, Space and Ctrl+A act on the focused panel

- **Iced:** these keys arrive through `GlobalKeyDown` and are routed by an `AppFocus` value that
  mouse hover and clicks change. `PlayLibrarySelection` plays the playlist's focused row first,
  even when the library has focus (`src/gui/app.rs:3711`).
- **Teksilo:**
  - The keys are handled by the playlist and library panels themselves, so they act on the
    panel that holds keyboard focus.
  - The library's Play button plays the library selection.
  - Text fields, menus and dialogs keep these keys.
- **Why:** a key that acts on a panel the user is not in is surprising.
- **Pinned by:** to be filled by the playlist and library waves, plus the "space and Enter in
  every text field" probe.

## 3. Closing the window saves state and stops cleanly

- **Iced:** the close button calls `std::process::exit(0)` (`src/gui/app.rs:5075`), so nothing is
  saved at exit.
- **Teksilo:** the window's close hook does three things, then lets the app exit normally:
  - saves the playlist position, shuffle and repeat state;
  - saves the EQ and DSP settings when the Audio Center is open;
  - stops the background threads and the audio engine.
- **Why:** the playback position and open settings were lost on every close.
- **Pinned by:** to be filled by the integration wave.

## 4. Escape closes overlays and dialogs

- **Iced:** no overlay or dialog reacts to Escape.
- **Teksilo:** Escape closes the topmost of these:
  - context menu;
  - popup;
  - dialog. In the EQ preset Load dialog, Escape restores the previewed state, as Cancel does;
  - the Audio Center panel.
- **Why:** standard desktop behaviour, and needed for keyboard users.
- **Pinned by:** to be filled by the integration wave.

## 5. No blocking work on the UI thread

- **Iced:** many calls block the UI thread inside `update`:
  - database queries (sort reset, album expansion, view-mode switch, reveal,
    `build_filter_index`);
  - stream restarts with sleeps (pause, stop, settings apply);
  - `systemctl` and `pw-metadata` subprocesses every 3 s while the Audio Center is open;
  - a `sysinfo` refresh on every tick.
- **Teksilo:**
  - All database access goes through one ordered worker thread.
  - Stream-touching engine calls and subprocesses go through a second worker.
  - Debug builds assert that the database is never touched from the UI thread.
- **Why:** the UI froze during these calls.
- **Pinned by:** to be filled by the frontend wave (the DbWorker thread assertion test).

## 6. The session is restored once at startup

- **Iced:** `AudoxidyApp::new` restores the last playlist and track, then `InitStartup`
  (`src/gui/app.rs:5256`) switches to the same playlist again, reloading and re-seeking the file.
  Volume and mix settings are also read twice.
- **Teksilo:** a single restore runs off the UI thread and announces itself with a
  `Session::Restored` event. The UI sends no playback command before that event.
- **Why:** the double restore reloaded the file and re-seeked it.
- **Pinned by:** to be filled by the frontend wave.

## 7. Popups open at their trigger

- **Iced:**
  - Context menus and dialogs open at the last mouse position.
  - The VU-meter popup always opens at the window's top-left, because its emitter passes
    `Point::ORIGIN` (`src/gui/library.rs:3040`).
  - The tab overflow menu opens at a hard-coded `(220, 440)` (`src/gui/app.rs:2996`).
- **Teksilo:** context menus open at the pointer, and popups are anchored to the control that
  opened them. Modal dialogs are centred, the framework default. The EQ preset dialogs stay
  positioned relative to the Audio Center.
- **Why:** hard-coded and origin positions put popups away from what opened them.
- **Pinned by:** to be filled by the meter and playlist waves.

## 8. Failed user actions show an error toast

- **Iced:** failures of actions the user started are only logged: playlist export, M3U import,
  external file import, applying audio settings, EQ preset import and export.
- **Teksilo:** the same failures are logged and also shown as an error toast.
- **Why:** the user saw nothing happen and had no hint why.
- **Pinned by:** to be filled by the playlist and Audio Center waves.

## 9. Downmix settings apply at startup

- **Iced:** the saved downmix settings (`audio_downmix_*`) are applied to the engine only when the
  Audio Center is first opened (`sync_from_engine`, `src/gui/audio_center.rs:1049-1071`). Until
  then, multichannel files play with the default mix.
- **Teksilo:** the startup restore applies them with the other DSP settings.
- **Why:** the saved mix should apply to everything played.
- **Pinned by:** to be filled by the frontend wave (session restore test).

## 10. The VU meter can be enabled on a fresh install; its popup stays open

- **Iced:** `meter_enabled` defaults to false. The only way to open the meter settings is a
  right-click on the meter (`src/gui/library.rs:3040`), and the meter is hidden when disabled
  (`src/gui/library.rs:3059-3072`), so a fresh install can never enable it. The popup also closes
  after each stepper change (`src/gui/app.rs:5346`, `5365`).
- **Teksilo:**
  - When the meter is disabled, its 400 px slot in the library bar stays, empty, and a
    right-click on it still opens the popup.
  - The popup stays open while values change.
- **Why:** the setting was unreachable, and the popup closed under the user's hand.
- **Pinned by:** to be filled by the meter wave.

## 11. The playing row is tracked by song id

- **Iced:** the playing song is a linear index (`playing_song_idx`). It is not remapped when a
  playlist search filters the groups (`src/gui/playlist.rs:215`, `src/gui/app.rs:3001-3009`), so
  the playing marker can land on the wrong row.
- **Teksilo:** the playing song is tracked by its song id, and rows are matched by id.
- **Why:** with an index, the marker drifted to the wrong row whenever a search filtered the list.
- **Pinned by:** to be filled by the playlist wave.

## 12. Changing a playlist's content does not reload the playing track

- **Iced:** adding, importing or sending songs to a playlist goes through `SwitchPlaylist`, which
  reloads the playlist and then reloads, seeks and re-applies play state on the current track
  (`src/gui/app.rs:2151`, `2176`, `3890`, `2853-2861`).
- **Teksilo:** a content change reloads only the playlist's rows. The current track keeps playing
  untouched.
- **Why:** adding songs caused an audible reload and seek of the playing track.
- **Pinned by:** to be filled by the playlist wave.

## 13. The playlist position is saved on a real 20 s interval

- **Iced:** the "every 20 s" autosave runs when `tick_count % 40 == 0` (`src/gui/app.rs:1657`).
  `tick_count` only advances in `UITick`, which runs only while the player overlay is active or a
  title scrolls, so the autosave rarely runs during normal playback.
- **Teksilo:** while playing, the position is saved every `behavior.auto_save_interval_secs`
  (20 s by default, from `config.ron`), plus on every event where Iced saves.
- **Why:** after a crash, the restored position could be far behind.
- **Pinned by:** to be filled by the playlist wave.

## 14. Hidden built-in EQ presets stay hidden across restarts

- **Iced:** deleting a built-in preset only hides it for the session (`hidden_builtins`,
  `src/gui/audio_center.rs:1648`). It comes back after a restart.
- **Teksilo:** the hidden list is saved under the new settings key `eq_hidden_builtin_presets`.
  The Iced UI ignores that key.
- **Why:** a preset the user deleted should stay deleted.
- **Pinned by:** to be filled by the EQ wave.

## 15. Spanish plurals and typos are corrected

- **Iced:**
  - Counts are formatted with a fixed plural, for example `"{} Canciones"` gives "1 Canciones"
    (`src/gui/library.rs:2972`).
  - Several labels have typos.
- **Teksilo:**
  - The es-ES catalogue uses Fluent plural selectors ("1 Canción", "2 Canciones").
  - These typos are fixed:

| Iced | Teksilo |
| --- | --- |
| Automatico | Automático |
| cancion | canción |
| Cancion | Canción |
| Reproduccion | Reproducción |
| Personalizacion | Personalización |
| Lirycs | Lyrics |
| elimarán | eliminarán |

  - Every other string matches the Iced literal exactly.
- **Why:** spelling and grammar errors in the interface.
- **Pinned by:** to be filled by the locale parity check (wave E).

## 16. "Eliminar lista" is disabled on system playlists

- **Iced:** "Eliminar lista" on a system playlist (Audoxidy, Oxidy Drift) opens the confirmation
  dialog. After confirmation, `delete_playlist` refuses silently (`src/db/database.rs:1352`) and
  nothing happens.
- **Teksilo:** the entry is disabled on system playlists.
- **Why:** the user confirmed a deletion that never happened.
- **Pinned by:** to be filled by the playlist wave.
