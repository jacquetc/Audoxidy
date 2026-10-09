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
| 17 | Apply and startup resolve "Automatic" sound-server rate and quantum the same way |
| 18 | Both EQ band sets are saved, the inactive one converted |
| 19 | The startup restore sets the PipeWire clock before rebuilding the stream |

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
- **Pinned by:** `frontend::db_worker::tests::recv_blocking_on_the_marked_ui_thread_panics` (any blocking wait on the marked UI thread panics in debug builds) and `frontend::reply::tests::try_recv_and_async_recv_are_allowed_on_the_ui_thread`; ordering and the purge re-check are pinned by `frontend::engine_worker::tests::{a_purge_followed_by_play_before_it_runs_is_skipped, engine_jobs_run_in_submission_order}`.

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
- **Pinned by:** `frontend::commands::engine_settings::tests::every_persisted_setting_reaches_the_engine` (the startup restore applies the downmix) and `frontend::commands::audio_settings::tests::the_audio_center_state_reads_everything_and_changes_nothing` (opening the configuration tab pushes nothing into the engine).

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
- **Pinned by:** `frontend::monitor::tests::autosave_is_due_every_interval_while_playing` and `frontend::monitor::tests::no_autosave_is_due_while_paused_and_resuming_restarts_the_interval` (the monitor side); the playlist view-model's persist-on-`AutosaveDue` test is added by the playlist wave.

## 14. Hidden built-in EQ presets stay hidden across restarts

- **Iced:** deleting a built-in preset only hides it for the session (`hidden_builtins`,
  `src/gui/audio_center.rs:1648`). It comes back after a restart.
- **Teksilo:** the hidden list is saved under the new settings key `eq_hidden_builtin_presets`.
  The Iced UI ignores that key.
- **Why:** a preset the user deleted should stay deleted.
- **Pinned by:** `frontend::commands::eq::tests::deleting_a_builtin_hides_it_across_restarts` and `frontend::commands::eq::tests::restoring_the_default_presets_clears_customs_and_the_hidden_list`; the JSON encoding (names may contain commas) by `frontend::settings::tests::hidden_preset_names_with_commas_survive_the_json_encoding`.

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

## 17. Apply and startup resolve "Automatic" sound-server rate and quantum the same way

- **Iced:** the sound-server clock is resolved by two different rules (defect D18):
  - Apply (`src/gui/audio_center.rs:1260-1270`): an Automatic rate is the player's selected rate,
    or none (the server's own) when the player rate is automatic; an Automatic quantum is estimated
    from the player rate.
  - Startup (`src/gui/app.rs:609-623`): an Automatic rate is the saved player rate, or 48 kHz when
    it is automatic; an Automatic quantum is estimated from the resolved system rate, or from
    48 kHz when the rate is left to the server.
  - With a fixed system rate of 192 kHz, an Automatic quantum and a 44.1 kHz player, Apply forces
    a 512-frame quantum and the next startup a 2048-frame one.
- **Teksilo:** Apply and the startup restore share one rule
  (`frontend::commands::audio_settings::resolve_system_clock`), the Iced startup rule:
  - Automatic rate: the rate the player stream opens at, that is the configured player rate, or
    48 kHz when it is automatic (the rate the engine picks for an automatic stream).
  - Automatic quantum: 10 ms of graph frames (`logic::audio_settings::estimate_quantum`) at the
    graph rate, that is the rate forced above, or 48 kHz (the server default) when the rate is
    left to the server.
  - The defect's rounding remark (exactly 256 frames rounds up to 512) is kept: no rate the UI
    offers gives a frame count on a boundary.
- **Why:** the quantum is counted in frames of the graph, which runs at the system rate, not at the
  player rate; and "follow the player" should mean the rate the player stream really uses. Using
  the startup rule also keeps what Iced does on every start, so a database written by either UI
  gives the same clock at startup.
- **Pinned by:** `frontend::commands::engine_settings::tests::startup_and_apply_resolve_automatic_with_the_same_rule`, `frontend::commands::audio_settings::tests::automatic_follows_the_player_rate_and_estimates_the_quantum_from_the_graph_rate` and `frontend::commands::audio_settings::tests::no_selectable_rate_lands_on_a_quantum_boundary`.

## 18. Both EQ band sets are saved, the inactive one converted

- **Iced:** `save_eq_settings_to_db` (`src/gui/audio_center.rs:376-388`, through
  `logic::eq::encode_band_gain_settings`) saves the active gains badly (defect D17):
  - in 31-band mode, `eq_band_gains_20` gets the first 20 of the 31 gains, which belong to other
    frequencies;
  - in 20-band mode, `eq_band_gains_31` is not written at all, so it keeps an older set.
- **Teksilo:** every EQ save writes both keys. The active set is stored as it is; the other set is
  converted from it with the log-frequency interpolation of `audio::preset::EqPreset`
  (`convert_31_to_20` or `convert_20_to_31`), the rule Iced already uses to save and export a
  preset. At the next startup both band sets of the engine are restored from those keys. The
  string format (`{:.1}` per band, comma-separated) is unchanged, so the Iced UI reads them.
- **Why:** after a restart in the other mode, the equalizer should sound like the curve the user
  set, not like a shifted or stale one.
- **Pinned by:** `frontend::commands::eq::tests::both_band_sets_are_persisted_with_a_converted_inactive_set`.

## 19. The startup restore sets the PipeWire clock before rebuilding the stream

- **Iced:** `AudoxidyApp::new` rebuilds the stream with the saved device settings
  (`src/gui/app.rs:557-594`), then forces the PipeWire clock and persists the server configuration
  (`src/gui/app.rs:596-633`) (defect D24). `system_audio::apply_pipewire_clock` documents that it
  must run before the stream is built, and Apply follows that order.
- **Teksilo:** the startup restore (`frontend::commands::engine_settings::restore_persisted`) runs
  Apply's order: force the clock, wait 250 ms, persist the PipeWire configuration (or persist the
  PulseAudio one), then rebuild the stream.
- **Why:** with the clock forced after the stream exists, PipeWire reconfigures the graph under a
  live stream, which can leave it at the old rate or disconnected from the sink.
- **Pinned by:** `frontend::commands::engine_settings::tests::the_clock_is_applied_before_the_stream_is_rebuilt`.
