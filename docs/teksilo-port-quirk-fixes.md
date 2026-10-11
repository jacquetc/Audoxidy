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
| 20 | Cancelling an EQ preset preview restores both band sets |
| 21 | The volume-and-mix tab shows the fade lengths in effect |
| 22 | Previous wraps to the last song under Repeat All |
| 23 | Shuffle Previous skips songs that left the playlist |
| 24 | Enabling shuffle with no enabled song drops the old session |
| 25 | M3U exports are Latin-1, M3U8 exports UTF-8, and durations are rounded |
| 26 | The RAM hard-cap purge has a 10 s cooldown and the next track is preloaded again |
| 27 | The restored compressor intensity and reverb room size apply at startup |
| 28 | Next or Previous right after a track ends moves once, from the track that plays |
| 29 | A track change saves position 0 for the new track |
| 30 | The restore opens the first playlist when the saved one is gone |
| 31 | Low-resource mode is detected before the audio engine starts |
| 32 | The window can be moved by its title bar and resized from its edges |
| 33 | The sliders, steppers and value fields work from the keyboard |
| 34 | The VU meter's labels are drawn whole, and the compact meter stays in its bounds |
| 35 | Escape clears a search field |
| 36 | The library's column header scrolls sideways with the rows |
| 37 | Bulk Left/Right keeps every toggled header selected |
| 38 | An album shared by two cached entries resolves to one id everywhere |
| 39 | A song's artist is the header that groups it |
| 40 | An album of a collapsed artist has no position |
| 41 | An album forms one run within its artist |
| 42 | Clearing the sort keeps the open grid album |
| 47 | The list modes follow the sort |
| 48 | The rubber band selects what it covers |
| 49 | Delete takes only what is picked |
| 50 | The Grid reveal finds a compilation track's album |
| 51 | A reveal opens the song's collapsed album in every list mode |
| 52 | Down with nothing focused selects the first row |
| 53 | The open grid album follows the search whatever its case |
| 54 | The open grid album's database fallback finds compilations |
| 59 | The equalizer's preset dialogs stay inside the window |
| 60 | Cancelling the preset dialog after an import saves the restored equalizer |

---

## 1. Deleting from the library asks for confirmation

- **Iced:** `LibraryDeleteSelection` (`src/gui/app.rs:3972`) removes the selected songs from the
  database immediately, from the Delete key or the action, with no confirmation.
- **Teksilo:** a `MessageBox` names what will be removed and asks for confirmation. Only
  "Eliminar" runs the deletion; Escape and "Cancelar" leave the library untouched.
- **Why:** it is destructive and cannot be undone. Removing playlist entries (not songs) keeps
  Iced's behaviour.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::quirk_1_delete_asks_first_and_deletes_only_after_the_confirmation`, `…::quirk_1_nothing_selected_asks_nothing` and `teksilo_ui::library::panel::tests::quirk_1_delete_on_the_list_asks_for_confirmation`.

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
- **Pinned by:** the library by `teksilo_ui::library::library_vm::tests::quirk_2_play_appends_the_library_selection_to_the_active_playlist` and `teksilo_ui::library::panel::tests::{quirk_2_enter_on_the_list_appends_the_selection_to_the_active_playlist, quirk_2_the_arrows_move_the_library_selection, quirk_2_the_search_field_keeps_enter_delete_and_the_arrows}`; the filter column by `teksilo_ui::filters::panel::tests::{the_keyboard_walks_the_tree_and_enter_selects_a_node, the_arrows_move_the_focus_between_the_cells_and_enter_chooses_one}`; the playlist by `teksilo_ui::playlist::list::tests::{the_keys_act_on_the_focused_list, the_keys_are_left_to_an_open_menu}` and `teksilo_ui::playlist::playlist_vm::tests::select_all_takes_every_row`, Ctrl+A by the integration wave, plus the "space and Enter in every text field" probe.

## 3. Closing the window saves state and stops cleanly

- **Iced:** the close button calls `std::process::exit(0)` (`src/gui/app.rs:5075`), so nothing is
  saved at exit.
- **Teksilo:** the window's close hook does three things, then lets the app exit normally:
  - saves the playlist position, shuffle and repeat state;
  - saves the EQ and DSP settings when the Audio Center is open;
  - stops the background threads and the audio engine.
- **Why:** the playback position and open settings were lost on every close.
- **Pinned by:** the persistence part by `frontend::session::tests::quirk_3_persist_now_then_stop_stores_the_position_and_the_session` (the position, play state, shuffle and session saved by `session::persist_now` are written before `session::stop` ends the threads); the close path by `teksilo_ui::shell::main_window::tests::closing_runs_the_hooks_and_stops_the_backend` (`AppHandles::before_close` runs the close hooks and queues the save from the window's close guard; `AppHandles::after_close` stops the backend from the window's removal hook) and `teksilo_ui::app::session::tests::persisting_after_the_restore_writes_the_position`. The Audio Center registers its EQ and DSP save as a close hook in its wave.

## 4. Escape closes overlays and dialogs

- **Iced:** no overlay or dialog reacts to Escape.
- **Teksilo:** Escape closes the topmost of these:
  - context menu;
  - popup;
  - dialog. In the EQ preset Load dialog, Escape restores the previewed state, as Cancel does;
  - the Audio Center panel.
- **Why:** standard desktop behaviour, and needed for keyboard users.
- **Pinned by:** the player menu by `teksilo_ui::player::panel::tests::{escape_closes_the_menu_without_acting, a_click_outside_closes_the_menu}`, the filter menu by `teksilo_ui::filters::panel::tests::escape_closes_the_menu_without_choosing`, the search fields by fix 35's tests; the playlist menus by `teksilo_ui::playlist::list::tests::escape_closes_the_row_menu` and `teksilo_ui::playlist::tabs::tests::{the_chevron_opens_every_playlist_under_itself, a_right_click_on_a_tab_opens_its_menu_and_on_the_strip_the_bar_menu}`; the Audio Center and the meter popup by their waves.

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
- **Pinned by:** `frontend::session::tests::quirk_6_the_restore_runs_the_settings_then_the_track_once_and_publishes_restored` (one restore, settings before the track, one `Session::Restored`, later calls refused), `frontend::session::tests::a_settings_restore_first_blocks_the_session_restore`, `frontend::commands::engine_settings::tests::the_restore_applies_each_setting_once_and_runs_once_per_context` and `frontend::app_context::tests::the_startup_restore_is_claimed_once_across_clones`; on the UI side, `teksilo_ui::app::session::tests::the_restore_runs_once` and `teksilo_ui::app::tests::building_the_root_restores_the_session_once` (the root starts the restore once and `SessionState::status` gates playback until it settles).

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
- **Pinned by:** the player menu by `teksilo_ui::player::panel::tests::{the_menu_button_opens_the_menu_below_it, the_menu_follows_the_button_when_the_panel_moves_or_has_an_inset}` (Iced opens it at a fixed position; the popover API has no offset, so it opens at the button's leading edge, about 7 px left of Iced's x = 15); the filter menu by `teksilo_ui::filters::panel::tests::the_selector_opens_a_172_pixel_menu_of_five_types_anchored_at_it` (Iced: a fixed 70 px from the window top, `library_filters.rs:345`); the playlist row menu by `teksilo_ui::playlist::list::tests::the_row_menu_opens_at_the_pointer` and the tab overflow list by `teksilo_ui::playlist::tabs::tests::the_chevron_opens_every_playlist_under_itself` (Iced: a fixed (220, 440)); the meter popup by its wave.

## 8. Failed user actions show an error toast

- **Iced:** failures of actions the user started are only logged: playlist export, M3U import,
  external file import, applying audio settings, EQ preset import and export.
- **Teksilo:** the same failures are logged and also shown as an error toast.
- **Why:** the user saw nothing happen and had no hint why.
- **Pinned by:** the command side (the failure comes back as an error and a `LongOperation::Failed` event, not only a log line) by `frontend::commands::playlist::tests::quirk_8_an_export_that_cannot_write_its_list_fails`, `frontend::commands::playlist::tests::quirk_25_an_entry_outside_latin_1_fails_the_m3u_export_without_writing` and `frontend::commands::playlist::tests::quirk_8_an_import_that_finds_nothing_or_no_file_fails` (export and import), and `frontend::commands::audio_settings::tests::a_stream_error_comes_back_as_err_and_the_keys_are_still_persisted` (Apply); the toasts are to be filled by the playlist and Audio Center waves; the UI side by `teksilo_ui::library::library_vm::tests::{quirk_8_a_failed_delete_shows_an_error_toast_and_a_cancelled_one_does_not, quirk_8_a_folder_that_cannot_be_scanned_shows_an_error_toast}` and `teksilo_ui::player::panel::tests::{a_scan_that_cannot_start_shows_an_error_toast, a_folder_dialog_that_cannot_open_shows_an_error_toast, a_track_that_cannot_be_played_shows_an_error_toast}`; the playlist by `teksilo_ui::playlist::playlist_vm::tests::{a_failed_export_is_an_error_notice, a_failed_import_is_an_error_notice, a_cancelled_operation_and_someone_elses_say_nothing}` and `teksilo_ui::playlist::notice::tests::every_notice_is_shown_as_a_toast_in_the_window` (the playlist also toasts a failed create, rename, delete or file location, and a successful export); the Audio Center by its wave.

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
- **Pinned by:** the slot by `teksilo_ui::library::panel::tests::{quirk_10_the_meter_slot_keeps_400_px_without_a_meter, quirk_10_the_meter_fills_its_slot}`; the meter and its popup by the meter wave.

## 11. The playing row is tracked by song id

- **Iced:** the playing song is a linear index (`playing_song_idx`). It is not remapped when a
  playlist search filters the groups (`src/gui/playlist.rs:215`, `src/gui/app.rs:3001-3009`), so
  the playing marker can land on the wrong row.
- **Teksilo:** the playing song is tracked by its song id, and rows are matched by id.
- **Why:** with an index, the marker drifted to the wrong row whenever a search filtered the list.
- **Pinned by:** the cursor side by `frontend::commands::playback::tests::quirk_11_the_playing_song_is_tracked_by_id_under_a_search` (`PlaybackCursor` keeps the playing song by id and finds its row in the search result); the playing marker is to be filled by the playlist wave; the view side by `teksilo_ui::playlist::playlist_vm::tests::the_playing_row_is_found_by_song_id_under_a_search`.

## 12. Changing a playlist's content does not reload the playing track

- **Iced:** adding, importing or sending songs to a playlist goes through `SwitchPlaylist`, which
  reloads the playlist and then reloads, seeks and re-applies play state on the current track
  (`src/gui/app.rs:2151`, `2176`, `3890`, `2853-2861`).
- **Teksilo:** a content change reloads only the playlist's rows. The current track keeps playing
  untouched.
- **Why:** adding songs caused an audible reload and seek of the playing track.
- **Pinned by:** the command side by `frontend::commands::playlist::tests::quirk_12_reloading_the_content_refreshes_the_rows_without_touching_the_player` (`playlist::reload_content` refreshes the rows with no load, seek or play-state call); the view-model wiring is to be filled by the playlist wave; the view side by `teksilo_ui::playlist::playlist_vm::tests::added_songs_appear_without_reloading_the_playing_track`.

## 13. The playlist position is saved on a real 20 s interval

- **Iced:** the "every 20 s" autosave runs when `tick_count % 40 == 0` (`src/gui/app.rs:1657`).
  `tick_count` only advances in `UITick`, which runs only while the player overlay is active or a
  title scrolls, so the autosave rarely runs during normal playback.
- **Teksilo:** while playing, the position is saved every `behavior.auto_save_interval_secs`
  (20 s by default, from `config.ron`), plus on every event where Iced saves.
- **Why:** after a crash, the restored position could be far behind.
- **Pinned by:** `frontend::monitor::tests::autosave_is_due_every_interval_while_playing` and `frontend::monitor::tests::no_autosave_is_due_while_paused_and_resuming_restarts_the_interval` (the monitor side); the command side by `frontend::commands::playlist::tests::quirk_13_persisting_stores_the_player_position_state_and_modes` and `frontend::session::tests::quirk_13_persist_now_on_autosave_resolves_once_the_state_is_stored`; the playlist view-model's persist-on-`AutosaveDue` test is added by the playlist wave; the view side by `teksilo_ui::playlist::playlist_vm::tests::the_autosave_persists_the_position`.

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
- **Pinned by:** the command side by `frontend::commands::playlist::tests::quirk_16_deleting_a_system_playlist_is_refused` (`playlist::delete_playlist` refuses a system playlist with an error instead of doing nothing); the disabled entry is to be filled by the playlist wave; the view side by `teksilo_ui::playlist::menus::tests::the_tab_menu_disables_eliminar_lista_on_a_system_playlist` and `teksilo_ui::playlist::playlist_vm::tests::a_system_playlist_cannot_be_deleted`.

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

## 20. Cancelling an EQ preset preview restores both band sets

- **Iced:** a click in the Load dialog previews the preset with `apply_eq_preset`, which sets the
  preamp and **both** band sets (20 and 31). Cancel (`CloseDialog`, `src/gui/app.rs:2163-2176`)
  restores the preamp and the gains of the **active** set only, from `EqStateBackup`; the other
  set keeps the previewed preset's gains, which show up after a switch to the other mode.
- **Teksilo:** the snapshot taken when the dialog opens (`frontend::commands::eq::capture`) holds
  the preamp, the selected preset and both band sets read from the engine, and Cancel
  (`frontend::commands::eq::restore`) puts all of them back.
- **Why:** a cancelled preview should leave the equalizer exactly as it was.
- **Pinned by:** `frontend::commands::eq::tests::a_cancelled_preview_restores_both_band_sets_the_preamp_and_the_preset`.

## 21. The volume-and-mix tab shows the fade lengths in effect

- **Iced:** on a fresh install (no `vol_fade_in_ms` or `vol_fade_out_ms` key), the startup puts
  2000 ms and 3000 ms in the engine (`src/gui/app.rs:730-737`), but the Audio Center's own fields
  start at 1000 ms and 2000 ms (`src/gui/audio_center.rs:270-271`) and are only overwritten from
  saved keys. The tab shows 1000 and 2000 ms while the engine fades over 2000 and 3000 ms, until a
  restart after the first change.
- **Teksilo:** the tab's values come from the engine state (`frontend::commands::mix::current`),
  so it shows the lengths in effect. The secondary-click reset values stay Iced's (1000 and
  2000 ms).
- **Why:** the tab showed values that were not the ones applied.
- **Pinned by:** `frontend::commands::mix::tests::after_a_fresh_install_restore_the_tab_shows_the_fade_lengths_in_effect`.

## 22. Previous wraps to the last song under Repeat All

- **Iced:** `PreviousTrack` (`src/gui/app.rs:1781`) goes through `PlaylistManager::play_prev`
  (`src/gui/playlist.rs:1268`) and `logic::playlist_nav::step_prev`
  (`src/logic/playlist_nav.rs:1056`), which wraps to the last enabled song only when nothing plays
  (defect D14). From the first song with Repeat All, Previous does nothing.
- **Teksilo:** `frontend::commands::playback::previous` uses `logic::playlist_nav::step_prev_fixed`:
  when there is no previous enabled song, Repeat All wraps to the last enabled song (the playing
  song itself when it is the only one enabled, as Next does).
- **Why:** Repeat All wraps forward at the end of the list; going back from its start should wrap
  the same way.
- **Pinned by:** `logic::playlist_nav::tests::d14_sequential_prev_from_the_first_song_wraps_to_the_last_with_repeat_all`, `logic::playlist_nav::tests::d14_sequential_prev_wraps_when_only_disabled_songs_come_before`, `logic::playlist_nav::tests::d14_sequential_prev_with_a_single_enabled_song_wraps_to_itself`, `logic::playlist_nav::tests::d14_the_fixed_prev_matches_the_original_elsewhere` and `frontend::commands::playback::tests::quirk_22_previous_from_the_first_song_wraps_to_the_last_with_repeat_all`.

## 23. Shuffle Previous skips songs that left the playlist

- **Iced:** in shuffle, `step_prev` (`src/logic/playlist_nav.rs:1056`) pops the current song from
  the history and moves the position back even when the previous history entry is a song that
  was removed from the playlist; it then falls back to the previous song in playlist order, and
  the removed song stays the last history entry (defect D15).
- **Teksilo:** `step_prev_fixed` plays the most recent earlier history entry whose song is still in
  the list. The current song and the skipped entries are popped, and the position moves back one
  per popped entry. When no earlier entry is in the list, the session is left alone and the
  sequential rule applies.
- **Why:** Previous should go back to the song played before, not to an unrelated neighbour, and
  should not leave a removed song at the top of the history.
- **Pinned by:** `logic::playlist_nav::tests::d15_shuffle_prev_skips_history_entries_that_left_the_list`, `logic::playlist_nav::tests::d15_without_an_earlier_song_in_the_list_the_session_is_kept_and_sequential_applies`, `logic::playlist_nav::tests::d15_shuffle_prev_without_removed_songs_pops_one_entry_like_the_original` and `frontend::commands::playback::tests::quirk_23_shuffle_previous_skips_a_song_that_left_the_playlist`.

## 24. Enabling shuffle with no enabled song drops the old session

- **Iced:** `ToggleShuffle` (`src/gui/app.rs:1875`) calls `PlaylistManager::activate_shuffle`
  (`src/gui/playlist.rs:383`, `logic::playlist_nav::activate_shuffle`). With no enabled song,
  shuffle stays off but the previous session is kept (defect D16), and the handler saves that stale
  session again.
- **Teksilo:** `frontend::commands::playback::toggle_shuffle` uses
  `logic::playlist_nav::activate_shuffle_fixed`: shuffle stays off, the session is dropped, and the
  saved session is cleared.
- **Why:** a session over songs that cannot play is stale; keeping it means a later shuffle
  restores an old order.
- **Pinned by:** `logic::playlist_nav::tests::d16_activating_shuffle_without_enabled_songs_drops_the_old_session`, `logic::playlist_nav::tests::d16_activating_shuffle_with_enabled_songs_matches_the_original` and `frontend::commands::playback::tests::quirk_24_enabling_shuffle_with_no_enabled_song_drops_the_old_session`.

## 25. M3U exports are Latin-1, M3U8 exports UTF-8, and durations are rounded

- **Iced:** `ExportPlaylist` (`src/gui/app.rs:2431`) writes the same UTF-8 text for `.m3u` and
  `.m3u8`, and `logic::playlist_export::m3u_entry` (`src/logic/playlist_export.rs:53`) truncates
  each duration with `as i32` (defect D19): a 4:59.9 song is listed as 299 s.
- **Teksilo:** `frontend::commands::playlist::export`:
  - durations are rounded to the nearest second (never below 0);
  - an `.m3u` file is written in Latin-1 (ISO-8859-1) and an `.m3u8` file in UTF-8;
  - when an entry has a character Latin-1 cannot store, the M3U export fails before anything is
    written or copied, with an error naming the entry and the character and suggesting M3U8. A
    lossy file is never written.
- **Why:** M3U is read as Latin-1 by the players that tell the two formats apart; UTF-8 bytes in an
  `.m3u` show up as mojibake there. M3U8 exists for UTF-8. Truncating durations under-reports
  every track by up to a second.
- **Round trip:** the M3U import (`Scanner::import_m3u`, shared with Iced) used to read every line
  as UTF-8 and silently skip a line that was not, so an `.m3u` with non-ASCII paths exported here
  did not import back. That is defect D27, fixed for both UIs: the import now decodes the file as
  UTF-8 and falls back to Latin-1 (`db::scanner::decode_playlist_text`).
- **Pinned by:** `frontend::commands::playlist::tests::quirk_25_an_m3u_export_is_latin_1_with_rounded_durations`, `frontend::commands::playlist::tests::quirk_25_an_entry_outside_latin_1_fails_the_m3u_export_without_writing`, `frontend::commands::playlist::tests::quirk_25_an_m3u8_export_is_utf_8` and `frontend::commands::playlist::tests::rounded_durations_never_go_negative`.

## 26. The RAM hard-cap purge has a 10 s cooldown and the next track is preloaded again

- **Iced:** while system memory use stays above 75 %, `Tick` (`src/gui/app.rs:1507`) runs the full
  memory purge on every tick, every 500 ms while playing (defect D23). The purge drops the
  preloaded next track. `GlobalMemoryPurge` keeps `preloaded_next_path` on purpose
  (`src/gui/app.rs:1615-1619`): re-creating the preload would only be dropped again by the next
  tick's purge, a loop. So the track is never preloaded again and the transition is not gapless.
- **Teksilo:** the playback monitor (`frontend::monitor`) gives the hard cap the 10 s cooldown of
  the player RAM-limit purge; the two RAM purges share it. With the cooldown, the loop Iced avoids
  is bounded to one purge every 10 s, so after a backend purge
  `frontend::monitor::queue_backend_purge` forgets the dropped preload, and the playback commands
  preload the next track again before the transition.
- **Why:** memory pressure from other applications made the player rebuild idle streams and lose
  gapless transitions continuously.
- **Pinned by:** `frontend::monitor::tests::d23_the_ram_hard_cap_purges_at_most_every_ten_seconds`, `frontend::monitor::tests::the_ram_hard_cap_and_the_player_ram_limit_share_one_cooldown` and `frontend::monitor::tests::the_backend_purge_forgets_the_preload_it_dropped`.

## 27. The restored compressor intensity and reverb room size apply at startup

- **Iced:** `AudoxidyApp::new` (`src/gui/app.rs:973`, `988`) assigns `compressor.intensity` and
  `reverb.room_size` without `Compressor::update_intensity_params` or `Reverb::set_room_size`
  (defect D25), so the compressor ratio, attack and release and the reverb's damping, pre-delay and
  feedback keep their defaults until the next stream rebuild or control change.
- **Teksilo:** the startup restore (`frontend::commands::engine_settings::restore_persisted`)
  computes them right after applying the saved effects.
- **Why:** the saved effect should sound the same from the first second.
- **Pinned by:** `frontend::commands::engine_settings::tests::d25_the_restored_compressor_intensity_and_reverb_room_size_take_effect_at_once`.

## 28. Next or Previous right after a track ends moves once, from the track that plays

- **Iced:** the end of a track is handled by the next `Tick` (`src/gui/app.rs:1412`). `NextTrack`
  (`src/gui/app.rs:1707`) and `PreviousTrack` (`src/gui/app.rs:1781`) ignore a pending end of
  track: when the engine has just promoted the preloaded song and that `Tick` has not run, Next
  loads the song after the one that ended, which is the song already playing (the engine reloads it
  from the start, since the end-of-track flag is still set), and the `Tick` then advances once more.
  The user hears the new song restart, then skip.
- **Teksilo:** `frontend::commands::playback::next` and `previous` take the end-of-track flag first.
  When the engine promoted the preloaded song, the cursor catches up with it, so Next moves on from
  the song that plays and Previous goes back from it; the end-of-track event that follows finds
  nothing to do. When playback stopped at the end instead, Next simply plays the next song.
- **Why:** one press of Next should move one song from what the user hears, and an end of track
  should be handled exactly once.
- **Pinned by:** `frontend::commands::playback::tests::quirk_28_a_manual_next_during_a_pending_end_of_track_moves_once_from_the_promoted_track`, `frontend::commands::playback::tests::quirk_28_previous_during_a_pending_end_of_track_goes_back_from_the_promoted_track`, `frontend::commands::playback::tests::a_manual_next_after_playback_stopped_at_the_end_moves_to_the_next_song_once` and `frontend::commands::playback::tests::the_end_of_a_track_advances_once_and_a_repeated_event_is_a_no_op`.

## 29. A track change saves position 0 for the new track

- **Iced:** `persist_playlist_state` (`src/gui/app.rs:5092`) saves the engine's current position.
  After a track change it runs right after `load_file`, which the engine's decoder applies later,
  so it can save the new song with the previous song's position; a restart then seeks the new
  song there.
- **Teksilo:** after a load (playing a song, Next, Previous, the end of a track), the playback
  commands save position 0 for the new song. Every other save still takes the engine's position.
- **Why:** after a crash or a restart before the next save, the restored song started at a
  position taken from another song.
- **Pinned by:** `frontend::commands::playback::tests::quirk_29_a_track_change_saves_position_zero_before_the_engine_applies_the_load`.

## 30. The restore opens the first playlist when the saved one is gone

- **Iced:** `AudoxidyApp::new` (`src/gui/app.rs:636`) restores `last_active_playlist_id` as it is;
  when no playlist has that id any more, the playlist panel is empty and nothing is restored.
- **Teksilo:** `frontend::session::restore` opens the first playlist (in tab order) instead and logs
  a warning. Without a saved id it opens playlist 1, as Iced does.
- **Why:** an empty panel bound to a playlist that does not exist leaves the user nowhere to go.
- **Pinned by:** `frontend::session::tests::quirk_30_the_restore_falls_back_to_the_first_playlist_when_the_saved_one_is_gone` and `frontend::session::tests::without_a_saved_playlist_the_restore_opens_playlist_one`.

## 31. Low-resource mode is detected before the audio engine starts

- **Iced:** `main.rs` creates the `AudioManager` first. `AudoxidyApp::new` then measures RAM and cores
  and stores `LOW_RESOURCE_MODE` (`src/gui/app.rs:512-521`). So the first stream sizes its ring
  buffer (`src/audio/engine.rs:409`) from the configuration profile alone, and later streams use
  the measured value.
- **Teksilo:** `teksilo_ui::startup::init_process` measures and stores the mode before
  `AppContext::production` creates the engine. Every stream uses the measured value. The rule is
  unchanged (less than 8 GiB of RAM or fewer than 4 cores). Only the memory figures and the CPU
  list are read; Iced refreshes every process as well.
- **Why:** with Iced, a low-resource machine got a full-size first stream, and a capable machine
  under a low-resource profile got a reduced first stream, until the first stream rebuild.
- **Pinned by:** `teksilo_ui::startup::tests::the_low_resource_rule_matches_the_iced_ui` (the rule).
  The ordering is the structure of `teksilo_ui::run`: `init_process` runs before `build_context`.

## 32. The window can be moved by its title bar and resized from its edges

- **Iced:** the window has no decorations (`decorations: false`, `src/main.rs`). The Audoxidy title
  strip has no drag region and the window has no resize border, so the window cannot be moved or
  resized with the mouse, only with the window manager's keyboard commands.
- **Teksilo:** the strip above the filter column and the library is a Teksilo `TitleBar`. Dragging
  its free area moves the window, and a double click toggles maximize. On Wayland and X11, where
  the client draws the frame, a `WindowFrame` adds resize strips along the edges. On Windows and
  macOS the OS resizes the borderless window itself. The Audoxidy window buttons are kept in place
  of the framework's. Where custom chrome is unavailable, the window falls back to native
  decorations and the strip keeps only the logo and the pills.
- **Why:** a window that cannot be moved or resized with the mouse.
- **Pinned by:** `teksilo_ui::shell::main_window::tests::with_custom_chrome_the_strip_is_a_title_bar_inside_a_resize_frame`
  and `teksilo_ui::shell::main_window::tests::without_resize_handles_the_root_is_the_app_itself`.
  These use a fake platform host: the strip is a `TitleBar`, and it sits in a `WindowFrame` exactly
  when the platform asks for resize strips. `teksilo_ui::shell::title_bar::tests::maximize_toggles_between_maximized_and_floating`
  covers the maximize button.

## 33. The sliders, steppers and value fields work from the keyboard

- **Iced:** an EQ or effect slider answers the arrow keys only after it has been clicked. Any other
  key clears its keyboard focus, and the arrows stop working until the next click, while the handle
  keeps its selected colour (`src/gui/widgets.rs:3952-3955`, `:4028-4033`). A stepper has no keyboard
  access at all. A value field opens only with a click.
- **Teksilo:** each control is a Tab stop:
  - The EQ slider steps by 0.1 dB with Up/Down, by 1 dB with Page Up/Down, and Home/End reach the
    limits. Left/Right do nothing, as in Iced.
  - The effect slider steps with Left/Right, and Enter opens its value field.
  - The stepper steps with Up/Down, and Enter opens its value field.
  - The keys keep working whatever other key was pressed, until focus leaves the control.
  - The secondary-click reset is also offered as an accessibility action.
- **Why:** the controls were unreachable without a mouse, and the arrow keys stopped working after an
  unrelated key press.
- **Pinned by:**
  - `teksilo_ui::widgets::eq_slider::tests::up_and_down_step_by_a_tenth_and_left_and_right_do_nothing`
  - `teksilo_ui::widgets::eq_slider::tests::page_keys_step_by_one_db_and_home_and_end_reach_the_limits`
  - `teksilo_ui::widgets::eq_slider::tests::the_reset_action_does_what_the_secondary_click_does`
  - `teksilo_ui::widgets::effect_slider::tests::left_and_right_step_and_up_and_down_do_nothing`
  - `teksilo_ui::widgets::effect_slider::tests::enter_on_the_slider_opens_the_field_and_a_secondary_press_on_the_box_resets`
  - `teksilo_ui::widgets::stepper::tests::up_and_down_step_and_enter_edits_when_focused`
  - `teksilo_ui::widgets::stepper::tests::the_stepper_is_a_tab_stop`

## 34. The VU meter's labels are drawn whole, and the compact meter stays in its bounds

- **Iced:** the meter's texts are anchored at the centre of their box and clipped to a box that
  starts at that anchor (`iced_wgpu` 0.14 `Text::Cached`), so only part of each label shows:
  - the "L"/"R" ("M"/"S") bar labels (`src/gui/widgets.rs:4897-4934`) show their lower-right quarter;
  - the numeric readout (`:4950-4995`) and the correlation value (`:5064-5082`) show their lower half;
  - each scale label (`:5131-5170`) shows its right half.
  
  The compact 24 px meter also draws its crest row (y = 24) and its scale (y = 33) outside its own
  bounds (`:4784-4793`).
- **Teksilo:** every label is drawn whole, at Iced's size and colour:
  - the bar labels are centred in the 10 × 6 box Iced gives them;
  - the readout keeps its left edge (x = 289), centred on its bar row;
  - the correlation value keeps its left edge (x = 21), centred on its row;
  - each scale label is centred on its tick.
  
  The compact meter draws only what fits in its 24 px: the crest row and the scale appear only in
  the 48 px meter. This is part of the user's decision to give the Teksilo UI a correct meter (Iced
  defects D5–D8).
- **Why:** the labels were unreadable fragments, and the compact meter painted over whatever lay
  below it.
- **Pinned by:**
  - `teksilo_ui::widgets::vu_meter::paint::tests::every_label_is_centred_in_the_box_iced_gives_it`
  - `teksilo_ui::widgets::vu_meter::paint::tests::every_label_and_shape_of_the_tall_meter_lies_inside_it`
  - `teksilo_ui::widgets::vu_meter::paint::tests::the_compact_meter_paints_nothing_outside_its_twenty_four_pixels`
  - `teksilo_ui::widgets::vu_meter::tests::every_label_of_the_tall_meter_is_drawn_whole_and_unclipped`
  - `teksilo_ui::widgets::vu_meter::tests::the_compact_meter_draws_nothing_below_its_twenty_four_pixels`

## 35. Escape clears a search field

- **Iced:** in the library, playlist and filter search fields (`standard_search_input`,
  `src/gui/widgets.rs:1852`), Escape only drops the keyboard focus (`text_input.rs` in iced_widget
  0.14). Emptying the search takes the "x" button or deleting the text.
- **Teksilo:** Escape in a search field that holds text empties it. In an empty field, Escape is
  passed on to whatever surrounds the field, so an overlay or a dialog still closes on it (fix 4).
- **Why:** the user's decision (2026-10-11). Escape is the usual "clear search" key.
- **Pinned by:** `teksilo_ui::widgets::search_field::tests::escape_clears_the_query_without_reaching_the_overlay`
  and `teksilo_ui::widgets::search_field::tests::escape_on_an_empty_field_reaches_the_overlay`.

## 36. The library's column header scrolls sideways with the rows

- **Iced:** in the list modes, only the column header scrolls horizontally (`src/gui/widgets.rs:1911`).
  Once it has scrolled, the header's columns no longer line up with the rows below them.
- **Teksilo:** the header and the rows share one horizontal offset, so every column stays over
  its values.
- **Why:** the user's decision (2026-10-11). A misaligned header labels the wrong values.
- **Pinned by:** `teksilo_ui::library::panel::tests::quirk_36_the_column_header_and_the_rows_share_one_offset`, `teksilo_ui::library::list_rows::tests::{quirk_36_the_rows_follow_the_shared_offset_and_the_headers_stay, a_sideways_wheel_over_the_list_moves_the_shared_offset}` and `teksilo_ui::library::library_vm::tests::quirk_36_the_shared_offset_stays_within_the_overflowing_content`. The header and the rows with columns sit in one `HShift`, laid out `max(width + offset, natural)` wide and shifted by the offset; the artist headers (and the Simple/Thumbnail album headers), which have no columns, stay put.

## 37. Bulk Left/Right keeps every toggled header selected

- **Iced:** `LibraryKeyNav` (`src/gui/app.rs:3149-3168`) takes its targets from `get_bulk_toggles`
  (`src/gui/library.rs:1082`), two lists in hash order. It applies them one by one with
  `set_album_collapsed` / `set_artist_collapsed` (`src/gui/library.rs:339-376`), and each call
  replaces the selection with its own header (defect D9). One arbitrary header stays selected, and
  the next key acts on it alone.
- **Teksilo:** `logic::library_selection::bulk_toggle_targets_fixed` returns the targets in display
  order. `apply_bulk_toggle_fixed` then collapses them all (Left) or expands them all (Right). It
  keeps every one of them selected and focuses the first. With one target this is Iced's behaviour,
  and Left then Right restores both the layout and the selection.
- **Why:** a multi-selection collapsed with Left must be expandable again with Right.
- **Pinned by:** `logic::library_selection::tests::{d9_a_bulk_collapse_keeps_every_toggled_header_selected, d9_collapse_then_expand_restores_the_layout_and_the_selection, d9_without_targets_nothing_changes_and_one_target_behaves_like_the_original}` and `teksilo_ui::library::library_vm::tests::quirk_37_left_collapses_every_selected_album_and_keeps_them_all_selected`.

## 38. An album shared by two cached entries resolves to one id everywhere

- **Iced:** when two cached albums share an artist and a title (defect D10), the list structure
  keeps the last one (`src/logic/library_geometry.rs:325-331`), while the filtered albums, the album
  of a song, the Detailed covers (`src/gui/widgets.rs:2882-2886`) and the delete map
  (`src/gui/app.rs:3754-3757`) keep the first.
- **Teksilo:** `logic::library_geometry::build_view_structure_fixed` keeps the first, like the rest
  of the library. The rows, the selection, the delete targets and the covers all resolve through it.
- **Why:** with two ids for one album, selecting, collapsing and deleting it acted on different
  albums.
- **Pinned by:** `logic::library_geometry::tests::d10_duplicate_albums_resolve_to_the_first_cached_hash_like_the_rest_of_the_library`.

## 39. A song's artist is the header that groups it

- **Iced:** `artist_to_toggle` (`src/logic/library_selection.rs:390`) takes the `artist` tag first,
  while the grouping uses the effective artist (album artist first) under the group's spelling
  (defect D11). The reveal opens `get_effective_artist(song)` verbatim
  (`src/gui/app.rs:6269-6271`), which misses a group spelled with different case.
- **Teksilo:** `artist_to_toggle_fixed` resolves the group by effective artist, ignoring case, and
  returns the group's own name. The reveal (by path and "show playing") uses it to open the song's
  collapsed artist.
- **Why:** the header to open must be one that exists.
- **Pinned by:** `logic::library_selection::tests::{d11_the_artist_of_a_song_is_the_header_that_groups_it, d11_the_header_keeps_the_spelling_of_its_group_and_follows_the_collapse_state}` and `teksilo_ui::library::library_vm::tests::a_reveal_by_path_opens_the_collapsed_artist_and_selects_the_song`.

## 40. An album of a collapsed artist has no position

- **Iced:** `get_item_y_range(Album)` (`src/gui/library.rs:581`; `item_y_range`,
  `src/logic/library_geometry.rs:746-759`) returns the y its header would have, inside a collapsed
  artist (defect D12), and reads the first two parts of a malformed id.
- **Teksilo:** `item_y_range_fixed` returns nothing for a hidden album or a malformed id. Every
  scroll the library view-model asks for goes through it.
- **Why:** a reveal scrolled to a row that was not shown.
- **Pinned by:** `logic::library_geometry::tests::{d12_an_album_of_a_collapsed_artist_has_no_span, d12_the_fixed_span_matches_every_visible_rectangle_and_rejects_malformed_ids}`.

## 41. An album forms one run within its artist

- **Iced:** `build_view_structure` (`src/logic/library_geometry.rs:342-362`) opens a new run whenever
  the album tag changes. Sorted by another column, one album forms several runs: duplicate
  `Album(id)` items sharing one collapsed state (defect D13).
- **Teksilo:** `build_view_structure_fixed` gives each album one run, at its first song, with its
  songs in list order. Without duplicates the result is identical to Iced's.
- **Why:** duplicate items with one id broke ranges, collapsing and keyboard navigation.
- **Pinned by:** `logic::library_geometry::tests::{d13_an_album_whose_songs_are_not_consecutive_forms_one_run, d13_collapsing_the_album_hides_all_its_songs, the_fixed_structure_matches_the_original_without_duplicates}`.

## 42. Clearing the sort keeps the open grid album

- **Iced:** `LibrarySortChanged` (`src/gui/app.rs:2849-2853`) reloads the expanded album with
  `get_songs_by_album(composite id)`, which matches nothing, so the open album empties (defect D22).
- **Teksilo:** the grid's open album (the inline band under its row of cards) resolves its album from
  the parsed `(artist, hash)`. It looks first in memory, in the library results, as Iced's
  `ToggleAlbumExpansion` does (`src/gui/app.rs:2924-2955`), and otherwise in the database
  (`commands::library::album_songs_by_album_artist`, quirk fix 54). A sort change only re-sorts the loaded songs,
  back to their canonical order when the sort is cleared.
- **Why:** clearing the sort emptied the open album.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::{quirk_42_clearing_the_sort_keeps_the_expanded_album_and_restores_its_order, quirk_42_an_album_hidden_by_the_search_loads_from_its_parsed_artist_and_hash}`.

## 47. The list modes follow the sort

- **Iced:** a click on a column header in the Simple, Thumbnail and Detailed modes changes the sort
  arrow but not the rows. `LibrarySortChanged` (`src/gui/app.rs:2819-2876`) sorts the grid's
  albums, the expanded album and the cached songs, but never `filtered_songs` / `artist_groups`,
  which the lists are built from (`src/gui/library.rs:1189-1221`).
- **Teksilo:** the list rows follow the sort through the three-state cycle, as the grid does, and
  return to the database order when the sort is cleared.
- **Why:** a sort control that sorts nothing in three of the four modes. The approved plan
  specified the working sort.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::a_sort_reorders_the_rows_and_clearing_it_restores_the_database_order`.

## 48. The rubber band selects what it covers

- **Iced:** the marquee is drawn 2 px off the pointer (`pos.y − 72` for a list starting at y = 70,
  `src/gui/app.rs:3333`, `:4704`). Its hit rectangles use `window width − 722`
  (`src/gui/app.rs:3429`), so the last 120 px of the list can never be selected by it.
- **Teksilo:** the band and its hit rectangles use the list's own coordinates and width.
- **Why:** the band selected rows it did not cover and missed rows it did.
- **Pinned by:** `teksilo_ui::library::list_rows::tests::a_drag_over_the_rows_selects_them_with_the_marquee` and `teksilo_ui::library::library_vm::tests::the_marquee_selects_the_rows_it_crosses`.

## 59. The equalizer's preset dialogs stay inside the window

- **Iced:** the Load and Save preset dialogs open at a fixed offset from the Audio Center
  (`src/gui/app.rs:5730-5740`). A dragged panel can push them partly off-screen.
- **Teksilo:** they open at the same offset from the panel, clamped so the whole dialog stays inside
  the window.
- **Why:** a dialog whose buttons are off-screen cannot be answered.
- **Pinned by:** `teksilo_ui::audio_center::equalizer::placement::tests` (the offset formulas and the clamp), `teksilo_ui::audio_center::equalizer::load_dialog::tests::a_dragged_panel_moves_the_dialog`.

## 60. Cancelling the preset dialog after an import saves the restored equalizer

- **Iced:** "Importar" in the Load dialog applies the file's preset and saves it at once. "Cancelar"
  then restores the backup in the engine and on screen, without saving it
  (`src/gui/app.rs:2163-2178`). The database keeps the imported preset, so the next start shows it.
- **Teksilo:** Cancel saves the restored state too, so the database matches what the engine plays
  and the tab shows.
- **Why:** the equalizer that came back after a restart was not the one the user had cancelled back
  to.
- **Pinned by:** `teksilo_ui::audio_center::equalizer::equalizer_vm::tests::quirk_60_cancelling_after_an_import_saves_the_restored_state` (it fails without the fix).

## 49. Delete takes only what is picked

- **Iced:** `LibraryDeleteSelection` (`src/gui/app.rs:3717-3790`) deletes the selected songs plus every
  song of each selected album and artist (`:3741-3785`), even when songs are picked inside that album
  or artist. Play (`get_selected_songs`, `src/gui/library.rs:991`) narrows such an album or artist to
  the picked songs.
- **Teksilo:** Delete takes the songs Play takes (`logic::library_selection::selected_song_ids`): a
  selected album or artist adds its songs only when none of them is picked. The confirmation of fix 1
  states the count.
- **Why:** the user's decision (2026-10-11). After selecting an album and Ctrl-picking two of its songs,
  Delete removed the whole album where Play acted on the two songs.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::{quirk_49_delete_takes_only_the_songs_picked_inside_a_selected_album_as_play_does, quirk_49_the_confirmation_states_how_many_songs_go}`.

## 50. The Grid reveal finds a compilation track's album

- **Iced:** in Grid mode the reveal looks for the card whose title is the song's album and whose artist
  is the song's `artist` tag (`src/gui/app.rs:6125-6140`). A card bears the album artist, so a
  compilation track finds no card, and the reveal opens and selects nothing.
- **Teksilo:** the card is found by the artist the grid groups the album under: the album artist,
  else the artist (`utils::get_effective_artist`). The album opens, and the song is selected and
  brought to the top.
- **Why:** the user's decision. "Show playing" and the playlist's "Mostrar en la biblioteca" did
  nothing for a compilation's tracks in Grid mode.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::quirk_50_a_grid_reveal_finds_a_compilation_track_s_album`.

## 51. A reveal opens the song's collapsed album in every list mode

- **Iced:** the list-mode reveal (`src/gui/app.rs:6266-6300`) expands the song's artist, but opens its
  collapsed album only in the Detailed mode (`:6273-6274`). In the Simple and Thumbnail modes the song
  is selected inside a collapsed album and stays hidden.
- **Teksilo:** the reveal also opens the album run that holds the song, in every list mode.
- **Why:** the user's decision. The revealed song was not shown.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::quirk_51_a_reveal_in_the_simple_and_thumbnail_modes_opens_the_collapsed_album`.

## 52. Down with nothing focused selects the first row

- **Iced:** with nothing focused, key navigation starts from row 0 as if it were focused
  (`src/gui/library.rs:768-800`), so the first Down selects the second row.
- **Teksilo:** with nothing focused (or the focused row hidden by a collapse), Up and Down select the
  first row; from there they move as before.
- **Why:** the user's decision. The first row was skipped.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::quirk_52_with_nothing_focused_down_and_up_select_the_first_row`.

## 53. The open grid album follows the search whatever its case

- **Iced:** the open grid album shows the songs `song_matches_search(song, query)` keeps, with the
  search trimmed but not lowercased (`src/gui/library.rs:481-489`, `:548-551`), while
  `song_matches_search` expects a lowercase query (`src/utils/mod.rs:13-16`). Any capital letter hides
  every song of the open album, while the rest of the search ignores case.
- **Teksilo:** the open album's songs are matched against the lowercased search text.
- **Why:** the user's decision. Typing "Starman" kept the album's card but showed none of its songs.
- **Pinned by:** `teksilo_ui::library::library_vm::tests::quirk_53_the_open_album_s_songs_match_the_search_whatever_its_case` and `teksilo_ui::library::grid_detail::tests::quirk_53_the_rows_follow_the_search_whatever_its_case`.

## 54. The open grid album's database fallback finds compilations

- **Iced:** when the open album's songs are not in memory, they are read with
  `get_songs_by_album_and_artist(hash, artist)` (`src/gui/app.rs:2957-2977`, `:6150-6160`), whose
  artist is the **track** artist (`src/db/database.rs:1088-1104`), while the album id carries the album
  artist. A compilation's songs match none, and the album opens empty.
- **Teksilo:** the fallback goes through `commands::library::album_songs_by_album_artist`, a facade query
  for the Teksilo UI: the album's songs by hash, kept when their album artist is the id's artist. The
  Iced UI keeps its query.
- **Why:** the user's decision. A compilation opened empty whenever the current results did not hold it.
- **Pinned by:** `frontend::commands::library::tests::{quirk_54_a_compilation_is_found_by_its_album_artist, the_album_artist_query_finds_an_ordinary_album_and_a_pipe_in_the_artist}` and `teksilo_ui::library::library_vm::tests::quirk_54_a_compilation_hidden_by_the_search_loads_from_its_album_artist`.

