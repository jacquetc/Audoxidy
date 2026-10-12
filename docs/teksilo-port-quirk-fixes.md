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
| 43 | The VU meter's peak hold decays and no longer pins the bars at 0 dBFS |
| 44 | The VU meter's RMS bar shows the RMS |
| 45 | The VU meter fades to silence on pause |
| 46 | The VU meter's correlation and M/S come from the samples |
| 47 | The list modes follow the sort |
| 48 | The rubber band selects what it covers |
| 49 | Delete takes only what is picked |
| 50 | The Grid reveal finds a compilation track's album |
| 51 | A reveal opens the song's collapsed album in every list mode |
| 52 | Down with nothing focused selects the first row |
| 53 | The open grid album follows the search whatever its case |
| 54 | The open grid album's database fallback finds compilations |
| 55 | Renaming the active playlist leaves the playing track alone |
| 56 | The playlist selection follows its songs through a search |
| 57 | A song without a track number is numbered within its group |
| 58 | "Enviar a otra lista" leaves out the active playlist |
| 59 | The equalizer's preset dialogs stay inside the window |
| 60 | Cancelling the preset dialog after an import saves the restored equalizer |
| 61 | A click on empty space commits an open value field |
| 62 | A memory purge never cuts the library |
| 63 | The cursor shows what can be clicked or resized |
| 64 | A sideways wheel over the player leaves the volume alone |
| 65 | The grid's keys step between cards and enter the open album only from its row |
| 66 | A letter brings the filter tree to its top |
| 67 | The window reopens where it was |
| 68 | The equalizer tab waits for the restored equalizer |
| 69 | A preload the playlist no longer wants is dropped |
| 70 | A library refresh keeps the search and the filters |
| 71 | A sort reorders the albums a filter shows |
| 72 | The title marquee starts over on a new title and rests while paused |
| 73 | Only the player's own events wake its controls |

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
  mouse hover and clicks change; every action in the filter column also sets it to the library
  (`src/gui/app.rs:4246`, `:4286`, `:4327`, `:4379`), where in the Teksilo UI the keys then go to
  the focused tree. `PlayLibrarySelection` plays the playlist's focused row first,
  even when the library has focus (`src/gui/app.rs:3711`).
- **Teksilo:**
  - The keys are handled by the playlist and library panels themselves, so they act on the
    panel that holds keyboard focus.
  - Ctrl+A (⌘A on macOS) is the app's one global shortcut (`teksilo_ui::app::keys`): it selects
    all in the playlist or the library when the focus is inside it, in the library when nothing
    has the focus (Iced's `AppFocus` starts there), and nowhere else; a text field keeps it.
  - The library's Play button plays the library selection.
  - Text fields, menus and dialogs keep these keys.
- **Why:** a key that acts on a panel the user is not in is surprising.
- **Pinned by:** the library by `teksilo_ui::library::library_vm::tests::quirk_2_play_appends_the_library_selection_to_the_active_playlist` and `teksilo_ui::library::panel::tests::{quirk_2_enter_on_the_list_appends_the_selection_to_the_active_playlist, quirk_2_the_arrows_move_the_library_selection, quirk_2_the_search_field_keeps_enter_delete_and_the_arrows}`; the filter column by `teksilo_ui::filters::panel::tests::{the_keyboard_walks_the_tree_and_enter_selects_a_node, the_arrows_move_the_focus_between_the_cells_and_enter_chooses_one}`; the playlist by `teksilo_ui::playlist::list::tests::{the_keys_act_on_the_focused_list, the_keys_are_left_to_an_open_menu}` and `teksilo_ui::playlist::playlist_vm::tests::select_all_takes_every_row`; Ctrl+A by `teksilo_ui::app::tests::{ctrl_a_with_nothing_focused_selects_the_whole_library, ctrl_a_in_the_playlist_selects_every_row, ctrl_a_in_the_library_selects_the_library_and_follows_the_focus, ctrl_a_in_a_text_field_is_the_fields_own, ctrl_a_outside_the_two_panels_selects_nothing}` and `teksilo_ui::app::keys::tests::the_focused_panel_takes_ctrl_a_and_the_library_takes_it_from_nowhere`; plus the "space and Enter in every text field" probe.

## 3. Closing the window saves state and stops cleanly

- **Iced:** the close button calls `std::process::exit(0)` (`src/gui/app.rs:4669`), so nothing is
  saved at exit.
- **Teksilo:** the window's close hook does two things, then lets the app exit normally:
  - saves the playlist position, shuffle and repeat state;
  - stops the background threads and the audio engine. A startup restore still waiting in the
    engine's queue then loads, seeks and plays nothing, and the player is stopped again once the
    threads are joined, so a close during the restore never starts playback.
- **Why:** the playback position and open settings were lost on every close.
- **Pinned by:** the whole close of a restored session by `teksilo_ui::shell::main_window::tests::quirk_3_closing_a_restored_session_stores_the_position_the_player_reached` (the stored song, position and play state after `before_close` and `after_close`); a close during the restore by `frontend::session::tests::a_close_while_the_restore_waits_in_the_queue_starts_no_playback`; the persistence part by `frontend::session::tests::quirk_3_persist_now_then_stop_stores_the_position_and_the_session` (the position, play state, shuffle and session saved by `session::persist_now` are written before `session::stop` ends the threads); the close path by `teksilo_ui::shell::main_window::tests::closing_runs_the_hooks_and_stops_the_backend` (`AppHandles::before_close` runs the close hooks and queues the save from the window's close guard; `AppHandles::after_close` stops the backend from the window's removal hook) and `teksilo_ui::app::session::tests::persisting_after_the_restore_writes_the_position`. Iced also saves the equalizer and the effects when the Audio Center closes; the Teksilo facade stores every equalizer, effect and mix change as it is made, so nothing is left to save at close (`teksilo_ui::audio_center::shell::audio_center_vm::tests::the_equalizer_and_the_effects_are_stored_without_a_close`).

## 4. Escape closes overlays and dialogs

- **Iced:** no overlay or dialog reacts to Escape.
- **Teksilo:** Escape closes the topmost of these:
  - context menu;
  - popup;
  - dialog. In the EQ preset Load dialog, Escape restores the previewed state, as Cancel does;
  - the Audio Center panel.
- **Why:** standard desktop behaviour, and needed for keyboard users.
- **Pinned by:** the player menu by `teksilo_ui::player::panel::tests::{escape_closes_the_menu_without_acting, a_click_outside_closes_the_menu}`, the filter menu by `teksilo_ui::filters::panel::tests::escape_closes_the_menu_without_choosing`, the search fields by fix 35's tests; the playlist menus by `teksilo_ui::playlist::list::tests::escape_closes_the_row_menu` and `teksilo_ui::playlist::tabs::tests::{the_chevron_opens_every_playlist_under_itself, a_right_click_on_a_tab_opens_its_menu_and_on_the_strip_the_bar_menu}`; the Audio Center by `teksilo_ui::audio_center::shell::panel::tests::{escape_closes_the_panel, escape_from_a_control_inside_a_tab_closes_the_panel_too, escape_does_nothing_while_the_panel_is_closed, the_close_button_takes_the_focus_when_the_panel_opens}`, `teksilo_ui::audio_center::config::config_tab::tests::escape_closes_an_open_pick_list_before_it_closes_the_panel` and `teksilo_ui::audio_center::volume_mix::volume_mix_tab::tests::escape_cancels_an_open_field_before_it_closes_the_panel` (the innermost overlay closes first), the equalizer dialogs by `teksilo_ui::audio_center::equalizer::load_dialog::tests::escape_and_a_press_outside_restore_the_backup` and `teksilo_ui::audio_center::equalizer::save_dialog::tests::cancelar_and_a_press_outside_close_without_saving`; the meter popup by `teksilo_ui::meter::tests::{escape_closes_the_popup, a_click_outside_closes_the_popup}`; the playlist dialogs by `teksilo_ui::playlist::dialogs::tests::{quirk_4_escape_closes_the_create_dialog_without_creating, quirk_4_escape_closes_the_rename_dialog_without_renaming, quirk_4_escape_closes_the_delete_confirmation_without_deleting, quirk_4_escape_closes_the_export_dialog_without_asking_where}`.

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
- **Pinned by:** `frontend::db_worker::tests::recv_blocking_on_the_marked_ui_thread_panics` (any blocking wait on the marked UI thread panics in debug builds) and `frontend::reply::tests::try_recv_and_async_recv_are_allowed_on_the_ui_thread`; ordering and the purge re-check are pinned by `frontend::engine_worker::tests::{a_purge_followed_by_play_before_it_runs_is_skipped, engine_jobs_run_in_submission_order}`; the Audio Center's sound-server status (Iced runs `pw-metadata` on the UI thread every 3 s, `audio_center.rs:298`) is one engine-worker request at a time, every 3 s, only while the config tab is shown: `teksilo_ui::audio_center::config::config_vm::tests::{a_poll_asks_the_engine_worker_and_shows_the_answer, polls_are_three_seconds_apart_whatever_the_frame_rate, a_slow_sound_server_is_never_asked_twice_at_once}` and `teksilo_ui::audio_center::config::config_tab::tests::{the_poll_subscription_lives_only_while_the_tab_is_shown, inside_the_panel_polling_stops_when_the_panel_closes_or_changes_tab, a_hidden_tab_asks_the_sound_server_nothing}`.

## 6. The session is restored once at startup

- **Iced:** `AudoxidyApp::new` restores the last playlist and track, then `InitStartup`
  (`src/gui/app.rs:4835`) switches to the same playlist again, reloading and re-seeking the file.
  Volume and mix settings are also read twice.
- **Teksilo:** a single restore runs off the UI thread and announces itself with a
  `Session::Restored` event. The UI sends no playback command before that event.
- **Why:** the double restore reloaded the file and re-seeked it.
- **Pinned by:** `frontend::session::tests::quirk_6_the_restore_runs_the_settings_then_the_track_once_and_publishes_restored` (one restore, settings before the track, one `Session::Restored`, later calls refused), `frontend::session::tests::a_settings_restore_first_blocks_the_session_restore`, `frontend::commands::engine_settings::tests::the_restore_applies_each_setting_once_and_runs_once_per_context` and `frontend::app_context::tests::the_startup_restore_is_claimed_once_across_clones`; on the UI side, `teksilo_ui::app::session::tests::the_restore_runs_once` and `teksilo_ui::app::tests::building_the_root_restores_the_session_once` (the root starts the restore once and `SessionState::status` gates playback until it settles); what the restore delays is applied once it settles (a search typed meanwhile, a flagged end of track) by `teksilo_ui::playlist::playlist_vm::tests::{a_search_typed_before_the_restore_applies_once_it_settles, an_end_of_track_before_the_restore_settles_is_handled_once_it_does}`.

## 7. Popups open at their trigger

- **Iced:**
  - Context menus and dialogs open at the last mouse position.
  - The VU-meter popup always opens at the window's top-left, because its emitter passes
    `Point::ORIGIN` (`src/gui/library.rs:1910`).
  - The tab overflow menu opens at a hard-coded `(220, 440)` (`src/gui/app.rs:2741`).
- **Teksilo:** context menus open at the pointer, and popups are anchored to the control that
  opened them. Modal dialogs are centred, the framework default. The EQ preset dialogs stay
  positioned relative to the Audio Center.
- **Why:** hard-coded and origin positions put popups away from what opened them.
- **Pinned by:** the player menu by `teksilo_ui::player::panel::tests::{the_menu_button_opens_the_menu_below_it, the_menu_follows_the_button_when_the_panel_moves_or_has_an_inset}` (Iced opens it at a fixed position; the popover API has no offset, so it opens at the button's leading edge, about 7 px left of Iced's x = 15); the filter menu by `teksilo_ui::filters::panel::tests::the_selector_opens_a_172_pixel_menu_of_five_types_anchored_at_it` (Iced: a fixed 70 px from the window top, `library_filters.rs:345`); the playlist row menu by `teksilo_ui::playlist::list::tests::the_row_menu_opens_at_the_pointer` and the tab overflow list by `teksilo_ui::playlist::tabs::tests::the_chevron_opens_every_playlist_under_itself` (Iced: a fixed (220, 440)); the Audio Center's pick lists by `teksilo_ui::audio_center::config::config_tab::tests::a_pick_list_opens_under_its_box`; the meter popup by `teksilo_ui::meter::tests::{a_right_click_on_the_meter_opens_the_popup_above_the_slot, a_right_click_on_the_empty_slot_opens_the_popup_too, the_popup_follows_the_slot_wherever_it_is}` (it opens just above the slot, aligned with its leading edge, and below it when there is no room above; Iced opens it at the window's top-left).

## 8. Failed user actions show an error toast

- **Iced:** failures of actions the user started are only logged: playlist export, M3U import,
  external file import, applying audio settings, EQ preset import and export.
- **Teksilo:** the same failures are logged and also shown as an error toast.
- **Why:** the user saw nothing happen and had no hint why.
- **Also (Teksilo only, the user's decision of 2026-10-11):** the playlist tells the user when an export
  has written its list (a success toast, or a warning saying how many song copies failed), and shows
  an error toast when creating, renaming or deleting a playlist, or opening a song's file location,
  fails; Iced only logs these.
- **Pinned by:** the command side (the failure comes back as an error and a `LongOperation::Failed` event, not only a log line) by `frontend::commands::playlist::tests::quirk_8_an_export_that_cannot_write_its_list_fails`, `frontend::commands::playlist::tests::quirk_25_an_entry_outside_latin_1_fails_the_m3u_export_without_writing` and `frontend::commands::playlist::tests::quirk_8_an_import_that_finds_nothing_or_no_file_fails` (export and import), and `frontend::commands::audio_settings::tests::a_stream_error_comes_back_as_err_and_the_keys_are_still_persisted` (Apply); the UI side by `teksilo_ui::library::library_vm::tests::{quirk_8_a_failed_delete_shows_an_error_toast_and_a_cancelled_one_does_not, quirk_8_a_folder_that_cannot_be_scanned_shows_an_error_toast}` and `teksilo_ui::player::panel::tests::{a_scan_that_cannot_start_shows_an_error_toast, a_folder_dialog_that_cannot_open_shows_an_error_toast, a_track_that_cannot_be_played_shows_an_error_toast}`; the playlist by `teksilo_ui::playlist::playlist_vm::tests::{a_failed_export_is_an_error_notice, a_failed_import_is_an_error_notice, a_cancelled_operation_and_someone_elses_say_nothing}` and `teksilo_ui::playlist::notice::tests::every_notice_is_shown_as_a_toast_in_the_window` (the playlist also toasts a failed create, rename, delete or file location, and a successful export); the Audio Center's Apply by `teksilo_ui::audio_center::config::config_vm::tests::a_failed_apply_shows_an_error_toast_and_still_disables_apply` and its preset operations by `teksilo_ui::audio_center::equalizer::equalizer_vm::tests::a_failed_preset_operation_shows_an_error_toast`.

## 9. Downmix settings apply at startup

- **Iced:** the saved downmix settings (`audio_downmix_*`) are applied to the engine only when the
  Audio Center is first opened (`sync_from_engine`, `src/gui/audio_center.rs:1049-1071`). Until
  then, multichannel files play with the default mix.
- **Teksilo:** the startup restore applies them with the other DSP settings.
- **Why:** the saved mix should apply to everything played.
- **Pinned by:** `frontend::commands::engine_settings::tests::every_persisted_setting_reaches_the_engine` (the startup restore applies the downmix) and `frontend::commands::audio_settings::tests::the_audio_center_state_reads_everything_and_changes_nothing` (opening the configuration tab pushes nothing into the engine).

## 10. The VU meter can be enabled on a fresh install; its popup stays open

- **Iced:** `meter_enabled` defaults to false. The only way to open the meter settings is a
  right-click on the meter (`src/gui/library.rs:1910`), and the meter is hidden when disabled
  (`src/gui/library.rs:1913-1943`), so a fresh install can never enable it. The popup also closes
  after each stepper change (`src/gui/app.rs:4925`, `4944`).
- **Teksilo:**
  - The meter's slot in the library bar is always there and always takes the right-click
    that opens the popup. It is the free space between the figures and the actions, never
    narrower than the meter wants (400 px when shown, as in Iced). With the meter disabled it
    is the gap Iced leaves there, so the bar keeps Iced's layout.
  - When the bar is too narrow for a shown meter (the library of the default 1360 px window),
    the figures give way first (ellipsis), then the meter shows the left part of its 400 px,
    so the actions stay in the bar; Iced lets them fall off its end. As in Iced, the 48 px
    meter overflows the 40 px bar by 4 px above and below.
  - The popup stays open while values change.
- **Why:** the setting was unreachable, and the popup closed under the user's hand.
- **Pinned by:** the slot by `teksilo_ui::library::panel::tests::{quirk_10_without_a_meter_the_actions_end_the_bar_after_a_free_slot, quirk_10_a_meter_fills_its_slot_and_a_shown_one_gets_its_400_px, quirk_10_in_a_narrow_bar_the_figures_give_way_and_the_actions_stay}` and, with the real meter slot, `teksilo_ui::meter::tests::{the_library_bar_hosts_the_slot_between_the_figures_and_the_actions, the_disabled_slot_keeps_its_400_by_40_and_shows_no_meter, the_library_bar_draws_the_whole_meter_over_its_edges, a_squeezed_meter_shows_its_left_part_and_keeps_its_height}`; the right-click on an empty slot by `teksilo_ui::meter::tests::a_right_click_on_the_empty_slot_opens_the_popup_too`; enabling the meter from the popup by `teksilo_ui::meter::tests::the_checkboxes_show_the_options_and_flip_them_persistently` and `teksilo_ui::meter::meter_vm::tests::showing_the_meter_sets_the_engine_and_persists_it`; the popup staying open by `teksilo_ui::meter::tests::the_popup_stays_open_while_its_values_change`.

## 11. The playing row is tracked by song id

- **Iced:** the playing song is a linear index (`playing_song_idx`). It is not remapped when a
  playlist search filters the groups (`src/gui/playlist.rs:215`, `src/gui/app.rs:3001-3009`), so
  the playing marker can land on the wrong row.
- **Teksilo:** the playing song is tracked by its song id, and rows are matched by id.
- **Repeat One** replays the playing song found by its id, even when a search hides it, and preloads it; Iced replays the row of its stale index (`src/gui/playlist.rs:1252-1265`). A song removed from the playlist stops playback: there is nothing to replay.
- **Why:** with an index, the marker drifted to the wrong row whenever a search filtered the list.
- **Pinned by:** the cursor side by `frontend::commands::playback::tests::quirk_11_the_playing_song_is_tracked_by_id_under_a_search` (`PlaybackCursor` keeps the playing song by id and finds its row in the search result); the playing marker by `teksilo_ui::playlist::playlist_vm::tests::the_playing_row_is_found_by_song_id_under_a_search`; the view side by `teksilo_ui::playlist::playlist_vm::tests::the_playing_row_is_found_by_song_id_under_a_search`; Repeat One by `frontend::commands::playback::tests::quirk_11_repeat_one_replays_the_playing_song_a_search_hides`.

## 12. Changing a playlist's content does not reload the playing track

- **Iced:** adding, importing or sending songs to a playlist goes through `SwitchPlaylist`, which
  reloads the playlist and then reloads, seeks and re-applies play state on the current track
  (`src/gui/app.rs:2151`, `2176`, `3890`, `2853-2861`).
- **Teksilo:** a content change reloads only the playlist's rows. The current track keeps playing
  untouched.
- **Why:** adding songs caused an audible reload and seek of the playing track.
- **Pinned by:** the command side by `frontend::commands::playlist::tests::quirk_12_reloading_the_content_refreshes_the_rows_without_touching_the_player` (`playlist::reload_content` refreshes the rows with no load, seek or play-state call); the view-model wiring by `teksilo_ui::playlist::playlist_vm::tests::added_songs_appear_without_reloading_the_playing_track`; the view side by `teksilo_ui::playlist::playlist_vm::tests::added_songs_appear_without_reloading_the_playing_track`.

## 13. The playlist position is saved on a real 20 s interval

- **Iced:** the "every 20 s" autosave runs on a `Tick` (every 500 ms while playing) when
  `tick_count % 40 == 0` (`src/gui/app.rs:1330`, `:1472-1475`). `tick_count` only advances in
  `UITick` (`:1666`), which runs only while the player's controls are awake or a title scrolls
  (`:5911-5917`). The cadence therefore depends on where the count stopped: frozen on a multiple
  of 40 (from startup, for instance), the position is saved on every tick, twice a second;
  frozen anywhere else, it is never saved until the controls wake.
- **Teksilo:** while playing, the position is saved every `behavior.auto_save_interval_secs`
  (20 s by default, from `config.ron`), plus on every event where Iced saves.
- **Why:** after a crash, the restored position could be far behind.
- **Pinned by:** `frontend::monitor::tests::autosave_is_due_every_interval_while_playing` and `frontend::monitor::tests::no_autosave_is_due_while_paused_and_resuming_restarts_the_interval` (the monitor side); the command side by `frontend::commands::playlist::tests::quirk_13_persisting_stores_the_player_position_state_and_modes` and `frontend::session::tests::quirk_13_persist_now_on_autosave_resolves_once_the_state_is_stored`; the view-model side by `teksilo_ui::playlist::playlist_vm::tests::the_autosave_persists_the_position` (through the `AutosaveDue` event alone).

## 14. Hidden built-in EQ presets stay hidden across restarts

- **Iced:** deleting a built-in preset only hides it for the session (`hidden_builtins`,
  `src/gui/audio_center.rs:1648`). It comes back after a restart.
- **Teksilo:** the hidden list is saved under the new settings key `eq_hidden_builtin_presets`.
  The Iced UI ignores that key, so its "Predeterminado" cannot clear it: built-ins hidden in the
  Teksilo UI stay hidden after a reset made in the Iced UI.
- **Why:** a preset the user deleted should stay deleted.
- **Pinned by:** `frontend::commands::eq::tests::deleting_a_builtin_hides_it_across_restarts` and `frontend::commands::eq::tests::restoring_the_default_presets_clears_customs_and_the_hidden_list`; the JSON encoding (names may contain commas) by `frontend::settings::tests::hidden_preset_names_with_commas_survive_the_json_encoding`.

## 15. Spanish plurals and typos are corrected

- **Iced:**
  - Counts are formatted with a fixed plural, for example `"{} Canciones"` gives "1 Canciones"
    (`src/gui/library.rs:1846`, `:1855`).
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
- **Language:** the UI follows the OS locale (`teksilo_ui/startup.rs`, `auto_detect_os_locale`),
  with es-ES, the Iced UI's language, as the fallback.
- **Pinned by:** `tools/check_locales.py` (in CI): every es-ES message without a placeable is an
  Iced literal or differs from one only by a correction above, and the 28 messages with placeables
  or plural selectors are listed for a hand check (done in Wave E); the plural selectors by
  `teksilo_ui::library::panel::tests::the_stats_read_the_figures_with_plurals`.

## 16. "Eliminar lista" is disabled on system playlists

- **Iced:** "Eliminar lista" on a system playlist (Audoxidy, Oxidy Drift) opens the confirmation
  dialog. After confirmation, `delete_playlist` refuses silently (`src/db/database.rs:1352`) and
  nothing happens.
- **Teksilo:** the entry is disabled on system playlists.
- **Why:** the user confirmed a deletion that never happened.
- **Pinned by:** the command side by `frontend::commands::playlist::tests::quirk_16_deleting_a_system_playlist_is_refused` (`playlist::delete_playlist` refuses a system playlist with an error instead of doing nothing); the disabled entry by `teksilo_ui::playlist::menus::tests::the_tab_menu_disables_eliminar_lista_on_a_system_playlist` and `teksilo_ui::playlist::playlist_vm::tests::a_system_playlist_cannot_be_deleted`; the view side by `teksilo_ui::playlist::menus::tests::the_tab_menu_disables_eliminar_lista_on_a_system_playlist` and `teksilo_ui::playlist::playlist_vm::tests::a_system_playlist_cannot_be_deleted`.

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
- **Pinned by:** `frontend::commands::mix::tests::after_a_fresh_install_restore_the_tab_shows_the_fade_lengths_in_effect`; the tab by `teksilo_ui::audio_center::volume_mix::volume_mix_vm::tests::the_tab_shows_the_values_the_engine_uses`.

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
  - the M3U format chosen in the export dialog is written in Latin-1 (ISO-8859-1) and the M3U8
    format in UTF-8 (the encoding follows the chosen format, not the file name's extension);
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
- When the engine skips the load because the same track plays on (Next under Repeat One, a single enabled song under Repeat All), the track's own position is saved, not 0.
- **Why:** after a crash or a restart before the next save, the restored song started at a
  position taken from another song.
- **Pinned by:** `frontend::commands::playback::tests::quirk_29_a_track_change_saves_position_zero_before_the_engine_applies_the_load`; also `frontend::commands::playback::tests::{quirk_29_playing_a_song_saves_position_zero_before_the_engine_applies_the_load, quirk_29_previous_saves_position_zero_before_the_engine_applies_the_load, quirk_29_the_end_of_a_track_saves_position_zero_before_the_engine_applies_the_load, quirk_29_next_under_repeat_one_keeps_the_position_of_the_track_that_plays_on}`.

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

## 43. The VU meter's peak hold decays and no longer pins the bars at 0 dBFS

- **Iced:** `update_meter` (`src/gui/app.rs:6056`), on a 30 ms `MeterTick` (`src/gui/app.rs:5945`),
  runs `logic::meter::MeterState::update` (`src/logic/meter.rs:207`). Its `peak_hold_step`
  (`src/logic/meter.rs:31-68`) decays only when `decay > 0.0` (`:48`); readings are negative dBFS, so a
  held peak never falls. The hold starts at 0.0, and the bars move towards `max(peak, hold)`
  (`src/logic/meter.rs:265-266`), so they sit at 0 dBFS until "reset peak", which turns the hold into
  a max-hold. Each decay step also assumes a 30 ms tick (defect D5).
- **Teksilo:** `logic::meter::LiveMeter` and `PeakHold` hold each peak for 1.5 s, then lower it by
  1.8 dB per second (Iced's constants) down to the −60 dB floor. A louder peak raises the hold again,
  the hold time counts only while playing, and infinite hold never ages. The bars follow the RMS, not
  the hold. Everything is computed from the time elapsed, whatever the tick rate.
- **Why:** the hold marker never moved and the bars stayed at full scale.
- **Pinned by:** `logic::meter::live_tests::{a_held_peak_stays_for_the_hold_time_then_falls_in_db_per_second, the_iced_hold_never_falls_where_the_correct_one_does, the_hold_marker_does_not_fall_while_paused, the_result_does_not_depend_on_the_tick_rate, an_infinite_hold_never_ages_and_never_falls}` and `teksilo_ui::meter::meter_vm::tests::{the_hold_marker_sits_at_the_peak_then_falls_with_time, infinite_hold_keeps_the_marker_for_ever}`.

## 44. The VU meter's RMS bar shows the RMS

- **Iced:** `MeterState::smooth_peak_*` and `smooth_rms_*` are never updated, yet `smooth_peak_*` is
  what the widget draws as the RMS bar (`src/gui/library.rs:1889`, `:1892`), so that bar is always
  empty (defect D6).
- **Teksilo:** the bar shows the RMS with Iced's ballistics (fast attack, slow release), the "Pk"
  readout the smoothed peak, and the crest factor comes from the instantaneous peak and RMS.
- **Why:** the RMS bar showed nothing.
- **Pinned by:** `logic::meter::live_tests::the_bar_level_is_the_rms_and_the_readout_the_peak` and `teksilo_ui::meter::meter_vm::tests::the_bars_show_the_rms_and_the_readout_the_peak`.

## 45. The VU meter fades to silence on pause

- **Iced:** on pause the meter multiplies the last dB levels by a factor falling to 0
  (`src/logic/meter.rs:289-309`). Levels are negative dB, so the bars rise to 0 dBFS, full scale
  (defect D7).
- **Teksilo:** the bars fall linearly in dB to the −60 dB floor over 0.4 s (Iced's fade time); then the
  meter goes idle, its `active` signal turns false and it draws no more frames. The hold markers stay
  where they were.
- **Why:** the meter jumped to full scale when the music stopped.
- **Pinned by:** `logic::meter::live_tests::{the_fade_falls_to_the_floor_where_the_iced_fade_rises_to_full_scale, pausing_fades_the_bars_down_to_silence_and_never_up}` and `teksilo_ui::meter::meter_vm::tests::pausing_fades_the_bars_to_silence_then_the_meter_goes_idle`.

## 46. The VU meter's correlation and M/S come from the samples

- **Iced:** the phase correlation is the sign product of the two channels' dB RMS values
  (`src/logic/meter.rs:270-275`), so it is +1 whenever both play. The M/S bars are the half sum and
  half difference of dB peaks (`src/logic/meter.rs:222-224`), and an M/S RMS is computed and never
  used (defect D8). The clip LED is never lit: Iced passes `false` (`src/gui/app.rs:5147`).
- **Teksilo:** the decoder accumulates Σl², Σr², Σl·r and the mid and side peaks over each batch,
  allocation-free (`audio::meter::StereoStats`), and publishes the correlation, the mid and side RMS and
  their peaks beside the existing readings (the Iced UI ignores them). The meter draws them, and its clip
  LED follows the peaks, held for 1.5 s.
- **Why:** the correlation gauge and the M/S bars showed meaningless values.
- **Pinned by:** `audio::meter::tests::{a_mono_signal_correlates_fully_and_is_all_mid, an_out_of_phase_signal_correlates_at_minus_one_and_is_all_side, a_quarter_period_offset_is_uncorrelated_and_splits_evenly, accumulating_and_reading_allocate_nothing}`, `logic::meter::live_tests::{a_hard_panned_signal_is_half_mid_and_half_side, identical_channels_correlate_at_plus_one_and_opposite_ones_at_minus_one, mid_side_mode_draws_the_linear_mid_and_side_levels}` and `teksilo_ui::meter::meter_vm::tests::{the_correlation_comes_from_the_linear_statistics, mid_side_mode_draws_the_linear_mid_and_side_levels, the_clip_led_follows_the_peaks_and_reset_peak_clears_it}`.

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
- **Iced, in Grid mode:** the open album's row is hit as `252 + 60 + 32 × all its songs` px tall
  (`src/gui/library.rs:730`), where the band draws `32 + 32 × the songs the search keeps + 37`
  (`library.rs:1653-1719`), and its songs from 32 px under the cards at the full library width
  (`:721-726`): the cards under an open album are hit at least 9 px off where they are drawn.
- **Teksilo:** the band and its hit rectangles use the list's own coordinates and width; in the
  grid, where the cards and the band's songs are drawn (`library/grid.rs` `spot_rect`).
- **Why:** the band selected rows it did not cover and missed rows it did.
- **Pinned by:** `teksilo_ui::library::list_rows::tests::a_drag_over_the_rows_selects_them_with_the_marquee` and `teksilo_ui::library::library_vm::tests::the_marquee_selects_the_rows_it_crosses`; in the grid `teksilo_ui::library::grid::tests::g3_the_rubber_bands_rectangles_are_where_the_grid_draws_its_cards_and_songs` and `teksilo_ui::library::panel::tests::g3_a_drag_from_a_card_down_over_the_band_selects_the_card_and_the_songs`.

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

## 55. Renaming the active playlist leaves the playing track alone

- **Iced:** after a rename, `InternalPlaylistsRefreshed` sends `SwitchPlaylist(id, false)`
  (`src/gui/app.rs:2404-2420`). For the active playlist this persists its state, reloads it, then
  loads, seeks and replays the playing track.
- **Teksilo:** the active playlist only takes its new name: nothing is sent to the player, and the
  selection stays. Renaming another playlist, or a system playlist (which makes a renamed copy), still
  opens it, as in Iced.
- **Why:** the user's decision (2026-10-11). A rename caused an audible reload and seek of the track,
  as content changes did before fix 12.
- **Pinned by:** `teksilo_ui::playlist::playlist_vm::tests::renaming_the_active_playlist_keeps_the_playing_track`; the switches that are kept by `teksilo_ui::playlist::playlist_vm::tests::{renaming_another_playlist_opens_it, renaming_a_system_playlist_switches_to_a_renamed_copy}`.

## 56. The playlist selection follows its songs through a search

- **Iced:** `PlaylistSearchChanged` (`src/gui/app.rs:2746`) filters the groups again but keeps the
  selection, the focus and the pivot as linear indices, which then point at other rows.
- **Teksilo:** the selection is carried by the identity of its rows (a song by its playlist item, a
  separator by its group's anchor). Selected rows the search still shows stay selected under their new
  indices. Rows it hides drop out and do not come back when the search is cleared. The focus and the
  pivot follow their row, or are dropped with it.
- **Why:** the user's decision. The highlighted rows, and the rows Delete, Space and "Enviar a otra
  lista" act on, changed under the user's hand.
- **Pinned by:** `teksilo_ui::playlist::playlist_vm::tests::the_selection_follows_its_songs_through_a_search`, `teksilo_ui::playlist::rows::tests::{a_selection_follows_its_rows_through_a_search, a_row_is_found_again_by_its_key}`.

## 57. A song without a track number is numbered within its group

- **Iced:** `build_song_row` (`src/gui/playlist.rs:931-935`) shows `{:02}.` of the row's linear index,
  which counts the separators. The first group reads 01, 02, …; a later group continues from its
  offset (the first song of the mock "CD 02" reads 12).
- **Teksilo:** the song shows its position among the songs of its group, counted over the whole
  playlist before any search, from 1. That is what Iced shows for the first group, and it reads like
  the track number it replaces. A song keeps its number when a search hides others.
- **Why:** the user's decision. The numbering was off by the number of groups above.
- **Pinned by:** `teksilo_ui::playlist::rows::tests::a_song_is_numbered_within_its_group_in_the_whole_playlist`, `teksilo_ui::playlist::list::tests::a_song_shows_its_track_number_or_its_position_in_its_group`.

## 58. "Enviar a otra lista" leaves out the active playlist

- **Iced:** the submenu (`src/gui/app.rs:3999`, built at `:4024-4041`) lists every playlist, the active
  one included. Sending songs to it adds nothing, since a song already in a playlist is skipped.
- **Teksilo:** it lists "Nueva lista", the system playlists, then the user playlists sorted
  case-insensitively, without the active playlist.
- **Why:** the user's decision. An entry that silently did nothing.
- **Pinned by:** `teksilo_ui::playlist::playlist_vm::tests::{the_send_to_lists_leave_out_the_active_playlist, the_send_to_lists_put_the_system_lists_first_then_sort_the_rest}`, `teksilo_ui::playlist::menus::tests::the_send_to_submenu_lists_new_then_the_system_lists_then_the_rest_sorted`.

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
  and the tab shows. The import is applied when its reply reaches the tab, not on the database
  worker: a Cancel before the reply wins, the file's preset joins the list and nothing of it is
  applied or saved, and a row previewed meanwhile is never split between the engine and the tab.
- **Why:** the equalizer that came back after a restart was not the one the user had cancelled back
  to.
- **Pinned by:** `teksilo_ui::audio_center::equalizer::equalizer_vm::tests::{quirk_60_cancelling_after_an_import_saves_the_restored_state, quirk_60_a_cancel_before_the_import_finishes_still_restores_the_backup, a_preview_made_while_an_import_reply_is_on_its_way_never_splits_the_engine_and_the_tab}` (each fails without its part of the fix).

## 61. A click on empty space commits an open value field

- **Iced:** any click outside an open stepper or effect-slider value field commits it and deselects
  the slider (the field loses focus on every outside click).
- **Teksilo:** a click into non-focusable space keeps the keyboard focus where it was (review item 25),
  which would leave the field open. So the Audio Center's frame takes the focus on a primary press,
  and the open field blurs and commits as in Iced. This restores Iced's behaviour rather than changing
  it; it is listed because the mechanism differs.
- **Why:** an edited value left uncommitted until Enter or Tab.
- **Pinned by:** `teksilo_ui::audio_center::volume_mix::volume_mix_tab::tests::a_press_on_the_background_commits_an_open_value_field` (it fails without the focusable frame).

## 62. A memory purge never cuts the library

- **Iced:** the memory purge (`GlobalMemoryPurge`, `src/gui/app.rs:1580-1631`) calls
  `LibraryManager::unload` (`src/gui/library.rs:252-297`). When the library is not focused and
  holds at least 150 albums and 700 songs, it is cut to its first 100 albums and 500 songs, and it
  shows only those until the user acts in it (focus, search, keys, view or filter change:
  `app.rs:2779`, `:2800`, `:3143`, `:3372`, `:4248`), which reloads it. The purge runs every 60 s
  and under memory pressure, so a large library left unfocused for a minute shrinks.
- **Teksilo:** the purge drops the oldest covers (16, after 32 more when a scan has just finished,
  as Iced's `purge_old_covers` does) and the backend frees its memory as in Iced (the string
  interner, the preload, the stream buffers when nothing plays, memory back to the OS). The library
  is never cut.
- **Why:** the user's decision (2026-10-12). A library that loses most of its rows while the user
  looks elsewhere looks broken.
- **Pinned by:** `teksilo_ui::app::tests::a_memory_purge_drops_iceds_16_oldest_covers` (the covers
  go, every song stays) and `teksilo_ui::shared::cover_store::tests::a_purge_drops_iceds_16_and_32_more_after_a_scan`.

## 63. The cursor shows what can be clicked or resized

- **Iced:** the whole view is wrapped in `CursorOff` (`src/gui/app.rs:30-175`, used at `:5887`),
  whose `mouse_interaction` is always `Interaction::Idle`: the cursor is the plain arrow
  everywhere, over buttons, rows and the column resize grips alike (menus and other overlays
  excepted).
- **Teksilo:** clickable controls show the pointing hand (`widgets/pressable.rs`, the rows, the
  filter cells, the stock controls' looks in `widgets/stock_styles.rs`), a disabled control the
  arrow, and the column header's grips the column-resize arrows (`widgets/column_header.rs`).
- **Why:** the user's decision (2026-10-12). The arrow everywhere hides which parts of the window
  act on a click and where a column can be resized.
- **Pinned by:** probe (Wave F): hovering a button shows the hand, a disabled entry the arrow, a
  column grip the resize cursor. Teksilo offers no headless query for the cursor (see the review
  file, item 49).

## 64. A sideways wheel over the player leaves the volume alone

- **Iced:** `PlayerScroll` (`src/gui/app.rs:4606-4609`) turns every wheel event over the player
  into a 5 % step, up when the vertical turn is positive and down otherwise, so a zero vertical
  turn (a sideways wheel, the end of a touchpad gesture) lowers the volume.
- **Teksilo:** a turn of zero changes nothing (`player/panel.rs`, the band's wheel handler); up and
  down turns step the volume as in Iced.
- **Why:** the user's decision (2026-10-12). Scrolling sideways should not turn the music down.
- **Pinned by:** `teksilo_ui::player::panel::tests::a_wheel_with_no_turn_is_not_a_step` (it fails without the fix) and `teksilo_ui::player::panel::tests::the_wheel_over_the_band_moves_the_volume_five_percent_and_shows_the_bubble`.

## 65. The grid's keys step between cards and enter the open album only from its row

- **Iced:** the Grid branch of `handle_key_nav` (`src/gui/library.rs:803-872`) walks the flat list
  of `get_visible_items` (`:663-737`), where the open album's songs sit between the rows of cards,
  with the arithmetic of a grid of cards: Up from a card under the open album (`current − per_row`)
  lands on one of its songs, or on a card shifted by the number of songs; Right from the last card
  of the open row enters the songs; Left from the first card under them lands on the last song;
  Down from a card above a short last row that holds the open album enters its songs.
- **Teksilo:** steps between cards are the grid's own and step over the open album; only Down
  from a card of the open album's row enters its songs, Up from the first song returns to the open
  card, Down stops at the last song, Left and Right do nothing in the songs
  (`logic::library_selection::grid_nav_target`). Shift extends the selection in Iced's order.
- **Why:** the user's decision (2026-10-12). The arithmetic counted songs as cards.
- **Pinned by:** `logic::library_selection::tests::the_cards_step_by_one_and_by_a_row_and_never_sideways_into_the_band` and `teksilo_ui::library::library_vm::tests::g2_down_from_any_card_of_the_open_row_enters_the_band_and_other_steps_are_the_grids`.

## 66. A letter brings the filter tree to its top

- **Iced:** `SelectSubfilter` resets `filters_manager.scroll_offset` (`src/gui/app.rs:4293`) but,
  unlike a type change (`:4279-4282`) or a search (`:4383-4386`), does not scroll the tree. The
  window of rows it draws (`tree_window`, `src/gui/library_filters.rs:121`) is computed for the
  top while the scrollable stays where it was, so a scrolled tree can show blank rows until the
  next scroll.
- **Teksilo:** a letter or the restore icon scrolls the tree to its top, as a type change and a
  search do.
- **Why:** the user's decision (2026-10-12): what Iced's reset evidently intends, without the
  blank rows.
- **Pinned by:** `teksilo_ui::filters::filters_vm::tests::g1_a_type_a_letter_and_the_restore_icon_bring_the_tree_and_the_library_to_the_top` and `teksilo_ui::filters::panel::tests::g1_a_letter_and_the_search_bring_the_scrolled_tree_back_to_its_top`.

## 67. The window reopens where it was

- **Iced:** the window opens at the profile's size from `config.ron` every time (`src/main.rs:50-56`),
  wherever and however large it was when it closed.
- **Teksilo:** the size, the position and the maximized state are saved when the window closes and
  restored when it opens (`teksilo_ui/mod.rs`, `SettingsBundle::with_window_state`, into
  `window_state.toml` in the app's settings directory); the profile's size applies on the first run.
- **Why:** the approved plan ("window geometry goes through `WindowStateService`").
- **Pinned by:** `teksilo_ui::shell::main_window::tests::the_initial_size_is_the_profile_size_within_the_minimum` (the first run) and a live check (Wave E): an edited `window_state.toml` (1500 × 900) came back on the next launch, and closing wrote it again.

## 68. The equalizer tab waits for the restored equalizer

- **Iced:** the equalizer is restored synchronously before the window exists (`src/gui/app.rs:839-875`),
  and the tab reads the engine when it opens (`src/gui/audio_center.rs:1143-1152`).
- **Teksilo:** the startup restore runs on the facade's workers while the window is already up.
  The equalizer's view-model reads the engine once the restore has settled; until that read
  answers, its controls, "Guardar" and the preset dialogs do nothing (a toggle or a radio that moved
  goes back).
- **Why:** reading the engine before the restore showed its defaults, and the first change then
  stored them over the saved equalizer, for both UIs (found by the Wave F review).
- **Pinned by:** `teksilo_ui::audio_center::equalizer::equalizer_vm::tests::{the_equalizer_built_during_the_restore_shows_and_keeps_the_restored_settings, the_controls_do_nothing_before_the_restored_equalizer_is_read, the_load_dialogs_backup_is_the_restored_equalizer}`.

## 69. A preload the playlist no longer wants is dropped

- **Iced:** the `Tick` path preloads the next song once per track in its last 15 s and never drops
  the preload (`src/gui/app.rs:1371-1448`); the decoder promotes any pending preload at the end of
  the file (`src/audio/decoder.rs:2460-2500`). When the next song changes or disappears after the
  preload (Repeat switched off on the last song, shuffle toggled, the next song disabled or hidden
  by a search), the engine plays the preloaded song while the playlist stays on the ended one: a
  wrong playing marker, and Next does nothing (Iced defect D28).
- **Teksilo:** `frontend::commands::playback::drop_stale_preload` purges a pending preload that is no
  longer the cursor's next song, from `maybe_preload`, `toggle_repeat`, `toggle_shuffle` and every
  change of the playback cursor; the next position event preloads the new next song. (A preload the
  engine already promoted before the change is not undone.)
- **Why:** the user's decision (2026-10-12): the end of a track must play what the playlist chose.
- **Pinned by:** `frontend::commands::playback::tests::{d28_turning_repeat_off_on_the_last_song_drops_the_preload_of_the_first, d28_a_disabled_next_song_gives_its_preload_to_the_new_next_one}` and `teksilo_ui::playlist::playlist_vm::tests::d28_a_preload_the_next_song_no_longer_is_is_dropped_at_once`.

## 70. A library refresh keeps the search and the filters

- **Iced:** when a scan has changed the library, the refresh loads every song
  (`get_all_songs`, `src/gui/app.rs:1345-1362`) and `InternalLibraryRefreshed` (`:3921`) shows them
  all, while the search field keeps its text and the filter column its highlight.
- **Teksilo:** the reload runs the active search and filters again
  (`teksilo_ui::models::LibrarySongsModel::reload`).
- **Why:** the user's decision (2026-10-12, a silent fix the review found): what the library shows
  must match the search and the filters it shows.
- **Pinned by:** `teksilo_ui::models::library_songs_model::tests::a_library_change_reloads_under_the_active_search`.

## 71. A sort reorders the albums a filter shows

- **Iced:** `LibrarySortChanged` (`src/gui/app.rs:2819-2876`) sorts `cached_albums`, the unfiltered
  list; with a filter or a search active, the grid shows `filtered_albums` in its old order.
- **Teksilo:** the grid's albums follow the sort whatever filter or search is active.
- **Why:** the user's decision (2026-10-12, a silent fix the review found).
- **Pinned by:** `teksilo_ui::library::library_vm::tests::a_sort_reorders_the_albums_a_filter_shows`.

## 72. The title marquee starts over on a new title and rests while paused

- **Iced:** the marquee's window comes from the global `tick_count` (`src/gui/app.rs:1665-1686`), so a
  new title starts wherever the count is, and `UITick` also runs while paused when the controls are
  awake (`:5911-5917`), so the title scrolls under a moving mouse while nothing plays.
- **Teksilo:** a new title starts from its beginning, and the marquee scrolls only while the track
  plays; pausing keeps its place (`player/title_marquee.rs`).
- **Why:** the user's decision (2026-10-12): keep the Teksilo behaviour.
- **Pinned by:** `teksilo_ui::player::player_vm::tests::{a_new_title_starts_the_marquee_from_the_beginning, the_marquee_stops_when_the_track_pauses_and_keeps_its_place}`.

## 73. Only the player's own events wake its controls

- **Iced:** any mouse movement anywhere in the window wakes the player's controls (`PlayerMouseMoved`
  from the window's subscription, `src/gui/app.rs:5973`), and so do the media keys.
- **Teksilo:** a movement over the player panel and the player's own actions (its zones, the seek
  bar, the wheel) wake them; a movement elsewhere in the window and the media keys do not.
- **Why:** the user's decision (2026-10-12): keep the Teksilo behaviour.
- **Pinned by:** `teksilo_ui::player::player_vm::tests::{the_controls_are_active_for_1500_ms_after_a_mouse_move, a_transport_action_hides_the_bubble_and_wakes_the_controls}`.
