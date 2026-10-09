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
| D5 | The peak hold never decays and pins the bars at 0 dBFS | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions |
| D6 | The RMS bar is always 0 | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions |
| D7 | The pause fade goes to full scale instead of silence | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions |
| D8 | Phase correlation and M/S are computed on dB values | **fix (Teksilo)**: a correct meter for the Teksilo UI; Iced keeps the old functions |
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
| **No decision needed** | | |
| D20 | Iced grid navigation y-positions use the unfiltered song count | not applicable |
| D21 | `widgets::apply_marquee` is never called | not applicable |

---

## Crash and data integrity

### D1. Shuffle Repeat All can recurse until the stack overflows

- **Where:** `play_next` (`src/gui/playlist.rs`), now mirrored by `logic::playlist_nav::advance_next`.
- **Defect:** with shuffle and Repeat All, at the end of the shuffle order, `play_next` re-shuffles and tries again. If every song id's first occurrence in the playlist is disabled (the same song added twice, with the first copy disabled), it re-shuffles forever and the stack overflows.

### D2. Send-to-list and Delete target the wrong song while a search is active

- **Where:** the `PlaylistSendToNewList`, `PlaylistSendToList` and `PlaylistDeleteSelection` handlers (`src/gui/app.rs`).
- **Defect:**
  - Send-to indexes the **full** group with the song's position in the **filtered** group, so a different song is sent.
  - Deleting a separator removes the whole full group, while a selected separator expands to its filtered songs only.
- **Note:** these handlers were left in the Iced code, because their rules are inconsistent.

### D3. Portable export overwrites files with the same name

- **Where:** `ExportPlaylist` with `ExportMode::PortableFolder` (`src/gui/app.rs:2657`), now `logic::playlist_export::plan_export`.
- **Defect:** two tracks with the same file name in the same `<artist>/<album>/` folder overwrite each other, so one file is lost from the copy.

### D4. `|` in a name breaks album and filter-node lookups

- **Where:** composite album ids (`"{artist}|{hash}"`) and filter-node ids (`"Type|L1|L2|L3"`), now in `logic::library_groups` and `logic::filter_index`.
- **Defect:** the separator is not escaped. An artist, album or label containing `|` makes `songs_for_album_id` fail, because it expects exactly two parts, and shifts the filter levels.

## VU meter maths

These are all in `update_meter` (`src/gui/app.rs:6477-6600`), now `logic::meter::MeterState`.

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

## No decision needed

| ID | Item | Why |
| --- | --- | --- |
| D20 | In the Iced grid only, navigation y-positions use the unfiltered song count while rendering uses the filtered one | The Teksilo grid uses `GridView`'s own navigation |
| D21 | `widgets::apply_marquee` is never called by the Iced UI | Kept in `logic::marquee` for completeness |
