# Iced UI defects found while extracting the shared logic

Wave A of the Teksilo port moved the Iced UI's pure logic into `src/logic/`. The extraction was
behaviour-preserving: every function reproduces the Iced behaviour exactly, defects included.
This file lists the defects that the work exposed, so that each one gets an explicit decision:

- **keep**: the Teksilo UI reproduces it (strict parity);
- **fix (Teksilo)**: the Teksilo UI fixes it, the Iced UI keeps it, and the fix is recorded in
  [`teksilo-port-quirk-fixes.md`](teksilo-port-quirk-fixes.md);
- **fix (both)**: the shared logic is fixed, so both UIs change.

Line numbers refer to the code before Wave A (commit `288477a`). Decisions were taken on 2026-10-09.

| ID | Defect | Decision |
| --- | --- | --- |
| **Crash and data integrity** | | |
| D1 | Shuffle Repeat All can recurse until the stack overflows | **fix (both)**, in `src/logic` with a regression test |
| D2 | Send-to-list and Delete target the wrong song while a search is active | **fix (both)**, in `src/logic` with a regression test |
| D3 | Portable export overwrites files with the same name | **fix (both)**, in `src/logic` with a regression test |
| D4 | `\|` in a name breaks album and filter-node lookups | **fix (both)**, in `src/logic` with a regression test |
| **VU meter maths** | | |
| D5 | The peak hold never decays and pins the bars at 0 dBFS | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions (quirk fix 43) |
| D6 | The RMS bar is always 0 | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions (quirk fix 44) |
| D7 | The pause fade goes to full scale instead of silence | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions (quirk fix 45) |
| D8 | Phase correlation and M/S are computed on dB values | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions (quirk fix 46) |
| **Navigation and selection** | | |
| D9 | Bulk Left/Right ends with one arbitrary header selected | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D10 | Duplicate (artist, title) albums resolve to different hashes | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D11 | Collapse targets use the `artist` tag, not the grouping artist | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D12 | Revealing an album ignores its collapsed artist | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D13 | One album can form two runs in the same artist | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D14 | Sequential Previous does not wrap with Repeat All | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D15 | Shuffle Previous rewinds into a song that left the list | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| D16 | Enabling shuffle with no enabled song keeps the old session | **fix (Teksilo)**: fixed variants in `src/logic` used by the Teksilo view-models; Iced keeps the old ones |
| **Settings and export** | | |
| D17 | 20/31-band EQ gains are not converted when saved | **fix (Teksilo)** |
| D18 | Apply and startup resolve Automatic rate/quantum differently | **fix (Teksilo)** |
| D19 | M3U and M3U8 output is identical; durations are truncated | **fix (Teksilo)** |
| D22 | Clearing the sort empties the expanded grid album | **fix (Teksilo)**: the Teksilo grid's open album (inline band) loads an album's songs from its parsed (artist, hash) |
| D23 | The 75 % RAM hard cap purges on every tick, with no cooldown | **fix (Teksilo)**: the frontend monitor applies a cooldown |
| D24 | Startup applies the stream settings before the PipeWire clock | **fix (Teksilo)**: the Teksilo restore sets the clock first, like Apply |
| D25 | Restored compressor intensity and reverb room size wait for a stream rebuild | **fix (Teksilo)**: the Teksilo startup restore applies their derived parameters |
| D26 | Each shuffle save/load moves one pending song into the history | **fix (both)**, in `src/db` with a regression test |
| D27 | The M3U import silently drops lines that are not UTF-8 | **fix (both)**: UTF-8 with a Latin-1 fallback, with tests |
| **No decision needed** | | |
| D20 | Iced grid navigation y-positions use the unfiltered song count | not applicable |
| D21 | `widgets::apply_marquee` is never called | not applicable |

---

## Crash and data integrity

### D1. Shuffle Repeat All can recurse until the stack overflows

- **Where:** `play_next` (`src/gui/playlist.rs`), now mirrored by `logic::playlist_nav::advance_next`.
- **Defect:** with shuffle and Repeat All, at the end of the shuffle order, `play_next` re-shuffles and tries again. If every song id's first occurrence in the playlist is disabled (the same song added twice, with the first copy disabled), it re-shuffles forever and the stack overflows.

**Fixed:** in `logic::playlist_nav`, a song id from the shuffle order now resolves to its first *enabled* occurrence (`enabled_linear_index_of_song_id`). That is still the first occurrence whenever it is enabled. `peek_next`, the preload (`next_song_ref`) and `advance_next` therefore agree on the enabled copy. `advance_next` no longer recurses: at the end of the order under Repeat All it re-shuffles at most once per call, and if the fresh order still plays nothing the advance ends as "no next song". Both UIs change, because the Iced `PlaylistManager::play_next` and `get_next_song_ref` call these functions. The duplicate case cannot come from the database today, since `PLAYLIST_ITEMS` has `UNIQUE(playlist_id, song_id)`, so the overflow was latent and the fix is defensive. Regression tests (`src/logic/playlist_nav/tests.rs`): `the_enabled_copy_of_a_song_is_found_when_its_first_copy_is_disabled`, `d1_shuffle_plays_the_enabled_copy_of_a_song_whose_first_copy_is_disabled`, `d1_repeat_all_reshuffle_plays_the_enabled_copy_instead_of_recursing`, `d1_repeat_all_keeps_reshuffling_and_playing_enabled_copies` and `d1_nothing_enabled_returns_no_next_without_looping`. The existing `the_preload_song_is_the_song_play_next_starts` and `an_exhausted_shuffle_with_repeat_all_reshuffles_and_plays` pass unchanged.

### D2. Send-to-list and Delete target the wrong song while a search is active

- **Where:** the `PlaylistSendToNewList`, `PlaylistSendToList` and `PlaylistDeleteSelection` handlers (`src/gui/app.rs`).
- **Defect:**
  - Send-to indexes the **full** group with the song's position in the **filtered** group, so a different song is sent.
  - Deleting a separator removes the whole full group, while a selected separator expands to its filtered songs only.
- **Note:** these handlers were left in the Iced code, because their rules are inconsistent.

**Fixed:** the new `logic::playlist_nav::action_target_song_ids(displayed, selected, clicked)` resolves the targets of a playlist action from the displayed groups: the search result when a filter is active, else the full list. If the clicked item is part of the selection, the targets are the whole selection; otherwise they are the clicked item alone. A song contributes itself, and a separator contributes the songs of its *displayed* group. The ids come in playlist order without duplicates, as with `selected_song_ids`. Song ids are enough, because `add_songs_to_playlist` and `batch_remove_songs_from_playlist` key items by (playlist, song id). The Iced `PlaylistSendToNewList`, `PlaylistSendToList` and `PlaylistDeleteSelection` handlers call it through `PlaylistManager::action_target_song_ids`. `PlaylistRequestSubMenu` only forwards the clicked index, so it needed no change. Without a search the targets are unchanged. Regression tests: `d2_an_action_under_a_search_targets_the_visible_song`, `d2_a_separator_under_a_search_targets_only_its_visible_songs`, `d2_a_clicked_item_in_the_selection_targets_the_whole_visible_selection` and `d2_without_a_search_the_targets_are_unchanged` (`src/logic/playlist_nav/tests.rs`), plus the Iced `gui::playlist::tests::test_action_targets_follow_the_search_results`.

### D3. Portable export overwrites files with the same name

- **Where:** `ExportPlaylist` with `ExportMode::PortableFolder` (`src/gui/app.rs:2657`), now `logic::playlist_export::plan_export`.
- **Defect:** two tracks with the same file name in the same `<artist>/<album>/` folder overwrite each other, so one file is lost from the copy.

**Fixed:** `logic::playlist_export::plan_export` gives every portable copy its own destination through `claim_file_name`. When an `<artist>/<album>/<file>` destination matches an earlier one, compared case-insensitively so that the copy is also safe on case-insensitive file systems, the later file gets ` (2)`, ` (3)`, … before its extension. The M3U/M3U8 entries use the final names. A source path listed twice is copied once, and both entries point to that copy. Single-file exports and portable exports without collisions are unchanged. Both UIs use the plan; the Iced `ExportPlaylist` handler still runs it in order. Regression tests (`src/logic/playlist_export.rs`): `d3_tracks_with_the_same_destination_get_numbered_names`, `d3_destinations_that_differ_only_by_case_collide`, `d3_a_generated_name_never_takes_an_existing_one`, `d3_the_same_source_listed_twice_is_copied_once`, `d3_without_collisions_the_plan_is_unchanged` and `a_single_file_export_keeps_repeated_names_and_sources`.

### D4. `|` in a name breaks album and filter-node lookups

- **Where:** composite album ids (`"{artist}|{hash}"`) and filter-node ids (`"Type|L1|L2|L3"`), now in `logic::library_groups` and `logic::filter_index`.
- **Defect:** the separator is not escaped. An artist, album or label containing `|` makes `songs_for_album_id` fail, because it expects exactly two parts, and shifts the filter levels.

**Fixed:** the new `logic::id_codec` (`join_parts` and `split_parts`) joins parts with `|` and escapes `\` and `|` inside a part with a backslash. Ids of names without those characters stay the same, byte for byte. It is used by `library_groups::album_composite_id`, the new `parse_album_composite_id`, `songs_for_album_id`, `library_geometry::item_y_range`, `filter_index::build_tree` and `filters_for_node`. It is also used by every Iced site that built or split album ids by hand, in `gui/app.rs`, `gui/library.rs` and `gui/widgets.rs`, so both UIs change. These ids live in memory only: no database setting or configuration field stores them, so the new format needs no migration. Regression tests: the five `logic::id_codec` tests; `d4_songs_for_album_id_finds_the_album_of_an_artist_named_with_a_pipe`, `d4_album_ids_with_pipes_and_backslashes_round_trip` and `d4_album_ids_without_pipes_or_backslashes_are_unchanged` (`library_groups`); `d4_album_ids_with_pipes_in_the_artist_or_album_stay_distinct` (`library_geometry`); and `d4_a_label_containing_a_pipe_selects_the_right_filters`, `d4_three_level_labels_with_pipes_and_backslashes_round_trip` and `d4_node_ids_without_pipes_or_backslashes_are_unchanged` (`filter_index`). The first of these replaces the parity test `a_label_containing_a_pipe_shifts_the_levels_like_the_original_ui`.

## VU meter maths

These are all in `update_meter` (`src/gui/app.rs:6056`), whose logic is now `logic::meter::MeterState` (`src/logic/meter.rs`).

### D5. The peak hold never decays and pins the bars at 0 dBFS

- The decay only runs when `decay > 0.0`. Readings are negative dBFS, so a held peak never decays.
- The default hold is 0.0, so the hold stays at 0 dBFS until "reset peak". After a reset it acts as a max-hold.
- The bars follow `max(peak, hold)`, so they are driven to 0 dB.
- The decay step also assumes a fixed 30 ms tick.

### D6. The RMS bar is always 0

`smooth_peak_*` and `smooth_rms_*` are never updated, yet `smooth_peak_*` is passed to the widget as the RMS value. The RMS bar is therefore always 0.

### D7. The pause fade goes to full scale instead of silence

The fade multiplies dB values by a factor that goes to 0, so the bars rise towards 0 dBFS (full scale) instead of falling to silence.

### D8. Phase correlation and M/S are computed on dB values

- "Phase correlation" is the sign product of the two dB RMS values, so it is always +1.
- M/S is computed on dB values.
- The M/S RMS is computed and never used.

## Navigation and selection

### D9. Bulk Left/Right ends with one arbitrary header selected

- **Where:** `get_bulk_toggles` (`src/gui/library.rs:2106`) and the calls at `src/gui/app.rs:3410-3418`.
- **Defect:** the lists come back in `HashSet` order. Each `set_*_collapsed` call then replaces the selection with that header, so after a bulk toggle the multi-selection is lost and one arbitrary header stays selected.

### D10. Duplicate (artist, title) albums resolve to different hashes

- `get_view_structure` keeps the last cached album with a given (artist, title).
- `get_album_to_toggle`, the filtered grid and the Detailed cover lookup keep the first.
- If the cache holds duplicates, the composite ids disagree.

### D11. Collapse targets use the `artist` tag, not the grouping artist

`get_artist_to_toggle` and `update_selection_memory` prefer the `artist` tag over `album_artist`. Grouping uses the effective artist, so with compilation-style tags the toggle target is not a header name. Both toggle getters are dead code in Iced.

### D12. Revealing an album ignores its collapsed artist

`get_item_y_range(Album)` ignores a collapsed artist and returns the y of a hidden album.

### D13. One album can form two runs in the same artist

When songs are sorted by a column other than the album, non-consecutive songs of one album form two runs. That gives duplicate `Album(...)` items with the same id and a shared collapsed state.

### D14. Sequential Previous does not wrap with Repeat All

With Repeat All, Previous does not wrap from the first song. It wraps only when nothing is playing (current index 0).

### D15. Shuffle Previous rewinds into a song that left the list

Shuffle Previous pops the history and moves the position back even when the previous song has been removed from the playlist. It then falls back to sequential.

### D16. Enabling shuffle with no enabled song keeps the old session

`activate_shuffle` with no enabled song turns shuffle off but keeps the previous session.

## Settings and export

### D17. 20/31-band EQ gains are not converted when saved

- In 31-band mode, `eq_band_gains_20` stores the first 20 of the 31 gains instead of a converted 20-band set.
- In 20-band mode, `eq_band_gains_31` is not written at all.

### D18. Apply and startup resolve Automatic rate/quantum differently

- **Rate:** Apply uses the player's selected rate, which is None when the player is on auto. Startup falls back to 48000.
- **Quantum:** Apply estimates it from the player rate; startup from the resolved system rate.
- **Rounding:** exactly 256 frames rounds up to 512.

### D19. M3U and M3U8 output is identical; durations are truncated

M3U and M3U8 differ only by extension. Durations are truncated with `as i32`.

### D22. Clearing the sort empties the expanded grid album

- **Where:** `LibrarySortChanged` (`src/gui/app.rs`, the branch that clears the sort and reloads the database order; line 2849 after Wave A).
- **Defect:** it calls `db.get_songs_by_album(album_id)` with the grid's expanded album id. That id is a composite `"{artist}|{hash}"`, but the function expects a bare album hash. The query matches nothing, so the expanded album shows no songs after the sort is cleared.
- **Found by:** Wave A3. Decided with the same rule as the other non-crash defects: the Teksilo UI does not have this path, because its grid's open album (inline band) loads the album from its parsed (artist, hash).

### D23. The 75 % RAM hard cap purges on every tick, with no cooldown

- **Where:** the `Tick` handler (`src/gui/app.rs`, `is_ram_over_hard_cap` → `GlobalMemoryPurge`).
- **Defect:** while system memory use is above 75 % (often because of other applications), every tick runs the full purge. That means every 500 ms while playing, and it includes dropping the preloaded next track (which breaks gapless playback) and rebuilding the stream when idle. The 500 MB / 75 % checks below it have a 10 s cooldown; the hard cap has none.
- **Found by:** Wave B1, which ported it as is.

### D24. Startup applies the stream settings before the PipeWire clock

- **Where:** `AudoxidyApp::new` (`src/gui/app.rs`, roughly lines 680-821 before Wave A).
- **Defect:** startup rebuilds the stream with the saved device settings first, and applies the PipeWire clock (rate and quantum) afterwards. `system_audio::apply_pipewire_clock` documents that it must run before the stream is built, and the Audio Center's Apply follows that order. At startup the stream may therefore open at the old server rate.
- **Found by:** Wave B1.

### D25. Restored compressor intensity and reverb room size wait for a stream rebuild

- **Where:** the startup restore (`AudoxidyApp::new`, the `dsp_compressor_intensity` and `dsp_reverb_room_size` keys).
- **Defect:** the restore assigns the two fields directly. It does not call `Compressor::update_intensity_params` or `Reverb::set_room_size`, which compute the parameters the DSP actually uses (ratio, attack, release; the room size). The saved values therefore only take effect after the next stream rebuild or the next change of those controls.
- **Found by:** Wave B2a, which reproduced it in `frontend::commands::engine_settings::restore_persisted`.

### D26. Each shuffle save/load moves one pending song into the history

- **Where:** `Database::save_shuffle_session` / `load_shuffle_session` (`src/db/database.rs`).
- **Defect:** the pending shuffle order was stored with `play_order = -(i)`, so its first song got 0. The load reads `play_order >= 0` as history, so every save and reload moved one song from "to play" to "played", and it collided with the first history entry (also 0). After a restart, shuffle could skip a song.
- **Found by:** Wave B2b.
- **Fixed:** the pending order is stored from -1 down (`-(i + 1)`), for both UIs. A session saved the old way is ambiguous once and is rewritten correctly by the next save. Pinned by `db::database::shuffle_session_tests::{a_saved_shuffle_session_loads_back_unchanged, repeated_save_and_load_cycles_do_not_move_pending_songs_into_history, a_session_with_an_empty_pending_order_keeps_its_history}`; the first two fail without the fix.

### D27. The M3U import silently drops lines that are not UTF-8

- **Where:** `Scanner::import_m3u` (`src/db/scanner.rs`).
- **Defect:** the file was read line by line as UTF-8, and `filter_map(|l| l.ok())` dropped every line that was not valid UTF-8 without a word. A Latin-1 `.m3u` (the traditional encoding, and what the Teksilo UI writes since quirk fix #25) lost every entry with a non-ASCII path.
- **Found by:** Wave B2b, as the import side of D19.
- **Fixed:** for both UIs, the file is decoded by `decode_playlist_text`: UTF-8 (a BOM is skipped), else Latin-1, which cannot fail. Pinned by `db::scanner::playlist_text_tests::*`, including `import_m3u_reads_a_latin1_playlist_with_accented_paths`.

### D29. A slider shorter than its handle panics (latent)

- **Where:** the EQ slider's body, `src/gui/widgets.rs:4199`: `handle_y.clamp(bounds.y, bounds.y + bounds.height - handle_height)`.
- **Defect:** `f32::clamp` panics when its minimum exceeds its maximum, which happens for a slider shorter than its 16 px handle. Latent: every Iced slider has a fixed size.
- **Found by:** the Wave F fix pass, in the Teksilo port of the same code.
- **Fixed:** in the Teksilo UI only (`teksilo_ui::widgets::eq_slider`, `.min(hi).max(lo)`; pinned by `a_slider_smaller_than_its_handle_lays_the_handle_at_the_start_without_panicking`). The Iced UI is unchanged: none of its layouts reaches the case.

## No decision needed

| ID | Item | Why |
| --- | --- | --- |
| D20 | In the Iced grid only, navigation y-positions use the unfiltered song count while rendering uses the filtered one | The Teksilo grid uses `GridView`'s own navigation |
| D21 | `widgets::apply_marquee` is never called by the Iced UI | Kept in `logic::marquee` for completeness |
