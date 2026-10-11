//! The view-model over the mock library: the "David Bowie" playlist, active
//! after the restore, groups "FLAC (16bit-44.1kHz)" (linear 0, songs 1 to 10)
//! and "CD 02" (linear 11, songs 12 to 15). Nothing plays after the restore.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use teksilo::prelude::AsyncRuntimeHandle;

use super::*;
use crate::frontend::AppContext;
use crate::frontend::events::LongOpKind;
use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
use crate::frontend::long_ops::OpStatus;
use crate::frontend::ports::PlayerPort;
use crate::frontend::test_fixtures::SeededLibrary;
use crate::teksilo_ui::mock_data::screenshot_library;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{run_until, with_shipped_messages};

const TIMEOUT: Duration = Duration::from_secs(10);

/// What a test drives: the services, the fakes behind them, the seeded data
/// and the view-model.
struct Env {
    services: AppServices,
    fakes: InMemoryFakes,
    seeded: SeededLibrary,
    vm: PlaylistViewModel,
}

impl Env {
    fn runtime(&self) -> &AsyncRuntimeHandle {
        &self.services.runtime
    }

    fn wait(&self, done: impl Fn() -> bool) {
        assert!(run_until(self.runtime(), TIMEOUT, done), "timed out");
    }

    fn playlist_id(&self, name: &str) -> i64 {
        self.seeded.playlist(name).unwrap().id
    }

    fn song_id(&self, linear_idx: usize) -> i64 {
        self.vm.song_at(linear_idx).unwrap().song_id
    }

    fn rows(&self) -> Vec<usize> {
        self.vm
            .rows()
            .get()
            .iter()
            .map(RowItem::linear_idx)
            .collect()
    }

    fn selected(&self) -> Vec<usize> {
        let mut selected: Vec<usize> = self.vm.selection().get().selected.into_iter().collect();
        selected.sort_unstable();
        selected
    }

    fn loads(&self) -> usize {
        self.fakes
            .player
            .calls()
            .iter()
            .filter(|call| matches!(call, PlayerCall::Load { .. }))
            .count()
    }

    fn reload_content(&self) {
        let version = self.services.playlist_content.version().get();
        self.services.playlist_content.reload();
        self.wait(|| self.services.playlist_content.version().get() > version);
    }

    fn reload_tabs(&self) {
        let version = self.services.playlist_tabs.version().get();
        self.services.playlist_tabs.reload();
        self.wait(|| self.services.playlist_tabs.version().get() > version);
    }

    fn wait_op(&self, op: OpId) {
        let ops = self.services.ctx.ops.clone();
        self.wait(|| ops.status(op) != Some(OpStatus::Running));
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.services.ctx.shutdown();
    }
}

/// Services over the mock library with their fakes, and a view-model; the
/// session is not restored.
fn unrestored() -> Env {
    let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
    let seeded = screenshot_library()
        .seed_on(&ctx.db)
        .recv_blocking()
        .unwrap();
    let services = AppServices::new(&AppHandles::new(ctx), AsyncRuntimeHandle::new());
    let vm = PlaylistViewModel::new(&services);
    Env {
        services,
        fakes,
        seeded,
        vm,
    }
}

/// [`unrestored`], restored, with the tabs loaded.
fn restored() -> Env {
    let env = unrestored();
    env.services
        .session
        .start_restore(&env.services.ctx, &env.services.runtime);
    env.services.playlist_tabs.reload();
    env.wait(|| {
        env.services.session.status().get().is_settled()
            && env.services.playlist_tabs.list().len() == 3
    });
    env
}

const NONE: SelectModifiers = SelectModifiers {
    toggle: false,
    range: false,
};
const CTRL: SelectModifiers = SelectModifiers {
    toggle: true,
    range: false,
};
const SHIFT: SelectModifiers = SelectModifiers {
    toggle: false,
    range: true,
};

// ---------------------------------------------------------------------------
// Rows and selection
// ---------------------------------------------------------------------------

#[test]
fn the_rows_are_the_restored_playlist_in_linear_order() {
    let env = restored();
    assert_eq!(env.rows(), (0..16).collect::<Vec<_>>());
    let rows = env.vm.rows().get();
    let RowItem::Separator(flac) = &rows[0] else {
        panic!("the first row is a separator")
    };
    assert_eq!(&*flac.folder_name, "FLAC (16bit-44.1kHz)");
    assert_eq!(flac.song_count, 10);
    let RowItem::Separator(cd2) = &rows[11] else {
        panic!("row 11 is the second separator")
    };
    assert_eq!(&*cd2.folder_name, "CD 02");
    assert_eq!(cd2.song_count, 4);
    assert_eq!(env.vm.playing_index().get(), None);
    assert_eq!(
        env.vm.active_playlist().get(),
        Some(env.playlist_id("David Bowie"))
    );
}

#[test]
fn nothing_shows_before_the_restore() {
    let env = unrestored();
    assert!(env.vm.rows().get().is_empty());
    assert!(!env.vm.is_ready());
}

#[test]
fn a_click_selects_ctrl_toggles_and_shift_takes_the_range_from_the_pivot() {
    let env = restored();
    let t = Instant::now();
    env.vm.click(2, NONE, t);
    assert_eq!(env.selected(), vec![2]);
    env.vm.click(4, CTRL, t + Duration::from_secs(1));
    assert_eq!(env.selected(), vec![2, 4]);
    env.vm.click(7, SHIFT, t + Duration::from_secs(2));
    assert_eq!(env.selected(), vec![4, 5, 6, 7], "from the pivot 4");
    env.vm.deselect();
    assert!(env.selected().is_empty());
    assert_eq!(env.vm.selection().get().focused, None);
}

#[test]
fn the_arrows_step_over_a_folded_group_and_shift_extends() {
    let env = restored();
    let t = Instant::now();
    env.vm.click(3, NONE, t);
    env.vm.collapse_selected();
    assert_eq!(
        env.selected(),
        vec![0],
        "the selection moved to the separator"
    );
    assert_eq!(env.rows(), vec![0, 11, 12, 13, 14, 15]);
    env.vm.key_move(true, false);
    assert_eq!(env.selected(), vec![11], "the folded songs have no row");
    env.vm.key_move(true, true);
    env.vm.key_move(true, true);
    assert_eq!(env.selected(), vec![11, 12, 13]);
    assert_eq!(env.vm.selection().get().focused, Some(13));
    env.vm.key_move(false, false);
    assert_eq!(env.selected(), vec![12]);
    assert_eq!(
        env.vm.scroll_request().get().map(|(_, target)| target),
        Some(ScrollTarget::Item(12)),
        "the focused row scrolls into view"
    );
    env.vm.expand_selected();
    assert_eq!(env.rows().len(), 6, "a song touches its own open group");
    env.vm.click(0, NONE, t + Duration::from_secs(1));
    env.vm.expand_selected();
    assert_eq!(env.rows().len(), 16);
}

#[test]
fn the_arrows_walk_the_search_result() {
    let env = restored();
    env.vm.search().set("2017".to_string());
    // The search keeps the four songs of "CD 02": separator 0, songs 1 to 4.
    assert_eq!(env.rows(), vec![0, 1, 2, 3, 4]);
    assert_eq!(
        env.vm.scroll_request().get().map(|(_, target)| target),
        Some(ScrollTarget::Top)
    );
    env.vm.key_move(true, false);
    env.vm.key_move(true, false);
    assert_eq!(env.selected(), vec![1]);
    assert_eq!(
        env.vm.song_at(1).unwrap().title.as_ref(),
        "Beauty and the Beast (2017 Remaster)"
    );
    env.vm.search().set(String::new());
    assert_eq!(env.rows().len(), 16);
}

#[test]
fn select_all_takes_every_row() {
    let env = restored();
    env.vm.select_all();
    assert_eq!(env.selected(), (0..16).collect::<Vec<_>>());
}

#[test]
fn a_menu_on_a_row_outside_the_selection_selects_it() {
    let env = restored();
    let t = Instant::now();
    env.vm.click(2, NONE, t);
    env.vm.click(3, CTRL, t + Duration::from_secs(1));
    env.vm.menu_opened_on(3);
    assert_eq!(env.selected(), vec![2, 3], "inside the selection: kept");
    env.vm.menu_opened_on(9);
    assert_eq!(env.selected(), vec![9]);
}

// ---------------------------------------------------------------------------
// Playing
// ---------------------------------------------------------------------------

#[test]
fn a_double_click_plays_and_a_slow_second_click_does_not() {
    let env = restored();
    let t = Instant::now();
    env.vm.click(3, NONE, t);
    env.vm.click(3, NONE, t + Duration::from_millis(450));
    assert_eq!(env.loads(), 0, "450 ms apart is two single clicks");
    env.vm.click(4, NONE, t + Duration::from_millis(500));
    env.vm.click(3, NONE, t + Duration::from_millis(600));
    assert_eq!(env.loads(), 0, "two different rows");
    env.vm.click(3, NONE, t + Duration::from_millis(900));
    assert_eq!(env.loads(), 1);
    assert_eq!(env.vm.playing_index().get(), Some(3));
    assert_eq!(
        env.vm.read_cursor(|c| c.playing_song_id()).flatten(),
        Some(env.song_id(3))
    );
}

#[test]
fn a_double_click_on_a_separator_folds_its_group() {
    let env = restored();
    let t = Instant::now();
    env.vm.click(11, NONE, t);
    env.vm.click(11, NONE, t + Duration::from_millis(100));
    assert_eq!(env.rows(), (0..12).collect::<Vec<_>>());
    assert_eq!(env.loads(), 0);
}

#[test]
fn no_song_plays_before_the_restore_has_settled() {
    // Quirk fix 6.
    let env = unrestored();
    env.vm.play_row(1);
    env.vm.play_item(1);
    env.vm.switch_playlist(1, true);
    assert!(env.vm.end_of_track().is_none());
    assert!(env.vm.position_changed(170.0, 180.0).is_none());
    env.vm.external_control(ExternalControlEvent::Next);
    assert!(env.fakes.player.calls().is_empty());
}

#[test]
fn enter_plays_the_focused_song_and_not_a_separator() {
    let env = restored();
    env.vm.click(0, NONE, Instant::now());
    env.vm.activate_focused();
    assert_eq!(env.loads(), 0);
    env.vm.key_move(true, false);
    env.vm.activate_focused();
    assert_eq!(env.vm.playing_index().get(), Some(1));
}

#[test]
fn the_menu_play_of_a_separator_plays_its_first_song_and_selects_it() {
    let env = restored();
    env.vm.play_item(11);
    assert_eq!(env.vm.playing_index().get(), Some(12));
    assert_eq!(env.selected(), vec![12]);
}

#[test]
fn the_end_of_a_track_advances_exactly_once() {
    let env = restored();
    env.vm.play_row(1);
    env.fakes.player.finish_track();
    assert_eq!(
        env.vm.end_of_track(),
        Some(EndOfTrackOutcome::Advanced(env.song_id(2)))
    );
    assert_eq!(
        env.vm.end_of_track(),
        Some(EndOfTrackOutcome::NotPending),
        "a repeated event does not advance again"
    );
    assert_eq!(env.vm.playing_index().get(), Some(2));
}

#[test]
fn the_end_of_a_track_scrolls_unless_the_pointer_is_over_the_list() {
    let env = restored();
    env.vm.play_row(1);
    let before = env.vm.scroll_request().get().map(|(seq, _)| seq);
    env.fakes.player.finish_track();
    env.vm.end_of_track();
    let after = env.vm.scroll_request().get();
    assert_eq!(after.map(|(_, target)| target), Some(ScrollTarget::Item(2)));
    assert_ne!(after.map(|(seq, _)| seq), before);
    env.vm.hovered().set(true);
    env.fakes.player.finish_track();
    env.vm.end_of_track();
    assert_eq!(env.vm.playing_index().get(), Some(3));
    assert_eq!(
        env.vm.scroll_request().get(),
        after,
        "no scroll under the pointer"
    );
}

#[test]
fn the_next_song_is_preloaded_once_per_track() {
    let env = restored();
    env.vm.play_row(1);
    assert!(env.vm.position_changed(100.0, 180.0).is_none(), "too early");
    env.vm.position_changed(170.0, 180.0);
    env.vm.position_changed(171.0, 180.0);
    let preloads: Vec<String> = env
        .fakes
        .player
        .calls()
        .into_iter()
        .filter_map(|call| match call {
            PlayerCall::Preload { path, .. } => Some(path),
            _ => None,
        })
        .collect();
    assert_eq!(preloads.len(), 1);
    assert_eq!(
        preloads[0],
        env.vm.song_at(2).unwrap().file_path.to_string()
    );
}

#[test]
fn the_autosave_persists_the_position() {
    // Quirk fix 13: the playlist position is saved on `Session::AutosaveDue`.
    let env = restored();
    env.vm.play_row(4);
    env.fakes.player.advance(42.0);
    env.vm.on_event(&Event::Session(SessionEvent::AutosaveDue));
    let reply = env.vm.autosave().unwrap();
    assert!(resolve(&env, reply).is_ok());
    let bowie = env.playlist_id("David Bowie");
    let data = playlist::load_playlist(&env.services.ctx, bowie)
        .recv_blocking()
        .unwrap()
        .data;
    assert_eq!(data.last_song_id, Some(env.song_id(4)));
    assert_eq!(data.last_pos_sec, 42.0);
}

#[test]
fn the_media_controls_drive_the_transport() {
    let env = restored();
    env.vm.play_row(1);
    env.vm
        .on_event(&Event::ExternalControl(ExternalControlEvent::Next));
    assert_eq!(env.vm.playing_index().get(), Some(2));
    env.vm.external_control(ExternalControlEvent::Previous);
    assert_eq!(env.vm.playing_index().get(), Some(1));
    env.vm.external_control(ExternalControlEvent::Pause);
    assert!(!env.fakes.player.snapshot().is_playing);
    env.vm.external_control(ExternalControlEvent::Toggle);
    assert!(env.fakes.player.snapshot().is_playing);
}

#[test]
fn the_playing_row_is_found_by_song_id_under_a_search() {
    // Quirk fix 11.
    let env = restored();
    env.vm.play_row(13);
    let playing = env.song_id(13);
    env.vm.search().set("2017".to_string());
    assert_eq!(
        env.vm.playing_index().get(),
        Some(2),
        "13 is row 2 of the result"
    );
    assert_eq!(env.song_id(2), playing);
    env.vm.search().set("Watch That Man".to_string());
    assert_eq!(env.vm.playing_index().get(), None, "the search hides it");
    env.vm.search().set(String::new());
    assert_eq!(env.vm.playing_index().get(), Some(13));
}

#[test]
fn shuffle_and_repeat_follow_the_cursor() {
    let env = restored();
    env.vm.toggle_repeat();
    assert_eq!(env.vm.repeat().get(), RepeatMode::All);
    env.vm.toggle_repeat();
    assert_eq!(env.vm.repeat().get(), RepeatMode::One);
    env.vm.toggle_shuffle();
    assert!(env.vm.shuffle().get());
    env.vm.toggle_shuffle();
    assert!(!env.vm.shuffle().get());
}

#[test]
fn the_equalizer_icon_toggles_the_audio_center_on_its_tab() {
    let env = restored();
    let link = env.services.links.audio_center.clone();
    env.vm.toggle_equalizer();
    assert!(link.open.get());
    assert_eq!(link.tab.get(), audio_center_tab::EQUALIZER);
    env.vm.toggle_equalizer();
    assert!(!link.open.get());
}

// ---------------------------------------------------------------------------
// Enabled flags and folding
// ---------------------------------------------------------------------------

fn enabled(env: &Env, linear_idx: usize) -> bool {
    env.vm.song_at(linear_idx).unwrap().enabled
}

#[test]
fn the_indicators_toggle_a_song_and_a_group() {
    let env = restored();
    env.vm.toggle_song_enabled(1);
    assert!(!enabled(&env, 1));
    let rows = env.vm.rows().get();
    assert!(matches!(&rows[1], RowItem::Song(song) if !song.song.enabled));
    env.vm.toggle_song_enabled(1);
    assert!(enabled(&env, 1));
    // A separator: some song enabled → the group is disabled.
    env.vm.toggle_group_enabled(11);
    assert!((12..16).all(|idx| !enabled(&env, idx)));
    let rows = env.vm.rows().get();
    assert!(matches!(&rows[11], RowItem::Separator(sep) if !sep.any_enabled));
    // Every song disabled → enabled again.
    env.vm.toggle_group_enabled(11);
    assert!((12..16).all(|idx| enabled(&env, idx)));
    // The flags are stored.
    env.reload_content();
    assert!(enabled(&env, 12));
}

#[test]
fn space_toggles_the_selection_when_the_focused_row_is_in_it() {
    let env = restored();
    let t = Instant::now();
    env.vm.click(2, NONE, t);
    env.vm.click(4, SHIFT, t + Duration::from_secs(1));
    env.vm.toggle_enabled_focused();
    assert!((2..=4).all(|idx| !enabled(&env, idx)));
    assert!(enabled(&env, 1) && enabled(&env, 5));
    // One disabled among the targets → all enabled.
    env.vm.toggle_song_enabled(3);
    env.vm.toggle_enabled(3);
    assert!((2..=4).all(|idx| enabled(&env, idx)));
}

#[test]
fn the_flags_survive_a_content_reload_and_keep_the_selection() {
    let env = restored();
    env.vm.click(5, NONE, Instant::now());
    env.vm.toggle_enabled(5);
    env.reload_content();
    assert!(!enabled(&env, 5));
    assert_eq!(env.selected(), vec![5], "flags are not a change of songs");
}

#[test]
fn plegar_desplegar_folds_every_group_then_unfolds_them() {
    let env = restored();
    env.vm.toggle_all_groups();
    assert_eq!(env.rows(), vec![0, 11]);
    env.vm.toggle_all_groups();
    assert_eq!(env.rows().len(), 16);
}

// ---------------------------------------------------------------------------
// Songs
// ---------------------------------------------------------------------------

/// Waits for `reply` and returns its result.
fn resolve<T>(env: &Env, reply: Reply<T>) -> Result<T, FrontendError> {
    let result = std::cell::RefCell::new(None);
    env.wait(|| {
        if result.borrow().is_none() {
            *result.borrow_mut() = reply.try_recv();
        }
        result.borrow().is_some()
    });
    result.into_inner().unwrap()
}

#[test]
fn removing_takes_the_selection_and_reselects_the_playing_row() {
    let env = restored();
    env.vm.play_row(12);
    let t = Instant::now();
    env.vm.click(2, NONE, t);
    env.vm.click(3, CTRL, t + Duration::from_secs(1));
    env.vm.remove_focused();
    env.wait(|| {
        playlist::load_playlist(&env.services.ctx, env.playlist_id("David Bowie"))
            .recv_blocking()
            .is_ok_and(|loaded| loaded.groups[0].songs.len() == 8)
    });
    env.reload_content();
    assert_eq!(env.rows().len(), 14);
    assert_eq!(
        env.vm.playing_index().get(),
        Some(10),
        "the playing song moved up"
    );
    assert_eq!(
        env.selected(),
        vec![10],
        "the songs changed: the playing row"
    );
}

#[test]
fn send_to_takes_what_the_search_shows() {
    // Iced defect D2: a selected separator under a search sends the songs
    // the search shows, not the whole folder.
    let env = restored();
    env.vm.search().set("Beauty".to_string());
    assert_eq!(env.rows(), vec![0, 1]);
    env.vm.click(0, NONE, Instant::now());
    let beauty = env.song_id(1);
    assert_eq!(env.vm.send_targets(0), vec![beauty]);
    let drift = env.playlist_id("Oxidy Drift");
    let added = resolve(&env, env.vm.send_to(0, drift).unwrap()).unwrap();
    assert_eq!(added, 1);
    let loaded = playlist::load_playlist(&env.services.ctx, drift)
        .recv_blocking()
        .unwrap();
    let ids: Vec<i64> = loaded
        .groups
        .iter()
        .flat_map(|g| g.songs.iter().map(|s| s.song_id))
        .collect();
    assert_eq!(ids, vec![beauty]);
}

#[test]
fn the_send_to_lists_put_the_system_lists_first_then_sort_the_rest() {
    let env = restored();
    for name in ["zeta", "Alpha", "beta"] {
        playlist::create_playlist(&env.services.ctx, name, Vec::new())
            .recv_blocking()
            .unwrap();
    }
    env.reload_tabs();
    let send: Vec<String> = env.vm.send_to_lists().into_iter().map(|t| t.name).collect();
    assert_eq!(
        send,
        vec![
            "Audoxidy",
            "Oxidy Drift",
            "Alpha",
            "beta",
            "David Bowie",
            "zeta"
        ]
    );
    let overflow: Vec<String> = env
        .vm
        .overflow_lists()
        .into_iter()
        .map(|t| t.name)
        .collect();
    assert_eq!(
        overflow,
        vec![
            "Audoxidy",
            "Oxidy Drift",
            "David Bowie",
            "zeta",
            "Alpha",
            "beta"
        ],
        "the overflow menu keeps the tab order"
    );
}

#[test]
fn show_in_library_asks_for_the_song_or_the_groups_first_song() {
    let env = restored();
    let request = env.services.links.reveal_in_library.latest();
    env.vm.show_in_library(3);
    assert_eq!(
        request.get().map(|(_, path)| path),
        Some(env.vm.song_at(3).unwrap().file_path.to_string())
    );
    env.vm.show_in_library(11);
    assert_eq!(
        request.get().map(|(_, path)| path),
        Some(env.vm.song_at(12).unwrap().file_path.to_string())
    );
}

// ---------------------------------------------------------------------------
// Playlists
// ---------------------------------------------------------------------------

#[test]
fn switching_shows_the_playlist_and_scrolls_to_the_top() {
    let env = restored();
    let audoxidy = env.playlist_id("Audoxidy");
    env.vm.click(3, NONE, Instant::now());
    env.vm.switch_playlist(audoxidy, false);
    env.wait(|| env.vm.active_playlist().get() == Some(audoxidy));
    assert!(!env.rows().is_empty());
    assert!(env.selected().is_empty(), "nothing plays there");
    assert_eq!(
        env.vm.scroll_request().get().map(|(_, target)| target),
        Some(ScrollTarget::Top)
    );
    assert_eq!(
        env.vm.tab_scroll_request().get().map(|(_, id)| id),
        Some(audoxidy)
    );
}

#[test]
fn switching_with_autoplay_plays_and_selects_the_first_song() {
    let env = restored();
    let audoxidy = env.playlist_id("Audoxidy");
    env.vm.switch_playlist(audoxidy, true);
    env.wait(|| env.vm.active_playlist().get() == Some(audoxidy));
    assert_eq!(env.vm.playing_index().get(), Some(1));
    assert_eq!(env.selected(), vec![1]);
    assert_eq!(
        env.vm.scroll_request().get().map(|(_, target)| target),
        Some(ScrollTarget::Item(1))
    );
}

#[test]
fn a_switch_overtaken_by_another_is_dropped() {
    let env = restored();
    let audoxidy = env.playlist_id("Audoxidy");
    let drift = env.playlist_id("Oxidy Drift");
    env.vm.switch_playlist(audoxidy, false);
    env.vm.switch_playlist(drift, false);
    env.wait(|| env.vm.active_playlist().get() == Some(drift));
    env.services.runtime.tick();
    assert_eq!(env.vm.active_playlist().get(), Some(drift));
    assert!(env.rows().is_empty(), "Oxidy Drift is empty");
}

#[test]
fn creating_a_playlist_switches_to_it_and_a_blank_name_gets_the_default() {
    let env = restored();
    let songs = vec![env.song_id(1), env.song_id(2)];
    env.vm.create_playlist("  ", songs, None);
    env.wait(|| {
        env.vm
            .active_playlist()
            .get()
            .is_some_and(|id| id > env.playlist_id("David Bowie"))
    });
    env.reload_tabs();
    let id = env.vm.active_playlist().get().unwrap();
    let tab = env.services.playlist_tabs.tab(id).unwrap();
    assert_eq!(tab.name, "New playlist 4", "three playlists before it");
    assert_eq!(env.rows().len(), 3, "a separator and the two songs");
}

#[test]
fn the_default_name_is_spanish_in_spanish() {
    let env = restored();
    with_shipped_messages("es-ES", || {
        assert_eq!(env.vm.default_playlist_name(), "Nueva lista 4");
    });
}

#[test]
fn renaming_a_system_playlist_switches_to_a_renamed_copy() {
    let env = restored();
    let audoxidy = env.playlist_id("Audoxidy");
    env.vm.rename_playlist(audoxidy, "Mis favoritas", None);
    env.wait(|| {
        env.vm
            .active_playlist()
            .get()
            .is_some_and(|id| id != audoxidy && id != env.playlist_id("David Bowie"))
    });
    env.reload_tabs();
    let copy = env.vm.active_playlist().get().unwrap();
    assert_eq!(
        env.services.playlist_tabs.tab(copy).unwrap().name,
        "Mis favoritas"
    );
    assert_eq!(
        env.services.playlist_tabs.tab(audoxidy).unwrap().name,
        "Audoxidy"
    );
    env.wait(|| env.services.active_playlist.name().get() == "Mis favoritas");
}

#[test]
fn renaming_a_user_playlist_renames_it_and_a_blank_name_does_nothing() {
    let env = restored();
    let bowie = env.playlist_id("David Bowie");
    env.vm.rename_playlist(bowie, "   ", None);
    env.vm.rename_playlist(bowie, "Bowie", None);
    env.wait(|| env.services.active_playlist.name().get() == "Bowie");
    env.reload_tabs();
    assert_eq!(env.services.playlist_tabs.tab(bowie).unwrap().name, "Bowie");
}

#[test]
fn a_system_playlist_cannot_be_deleted() {
    // Quirk fix 16, the view-model side: the entry asks `can_delete`.
    let env = restored();
    assert!(!env.vm.can_delete(env.playlist_id("Audoxidy")));
    assert!(!env.vm.can_delete(env.playlist_id("Oxidy Drift")));
    assert!(env.vm.can_delete(env.playlist_id("David Bowie")));
}

#[test]
fn deleting_the_active_playlist_switches_to_the_first() {
    let env = restored();
    let bowie = env.playlist_id("David Bowie");
    env.vm.delete_playlist(bowie, None);
    env.wait(|| env.vm.active_playlist().get() == Some(env.playlist_id("Audoxidy")));
    env.reload_tabs();
    assert!(env.services.playlist_tabs.tab(bowie).is_none());
}

// ---------------------------------------------------------------------------
// Long operations
// ---------------------------------------------------------------------------

fn ended(env: &Env, op: OpId, kind: LongOpKind) -> LongOpEvent {
    env.wait_op(op);
    let info = LongOpInfo {
        op_id: op,
        kind,
        progress: None,
        error: None,
        cancelled: false,
    };
    match env.services.ctx.ops.status(op) {
        Some(OpStatus::Completed) => LongOpEvent::Completed(info),
        _ => LongOpEvent::Failed(info),
    }
}

/// A fresh directory under the system temporary directory.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "audoxidy-playlist-{name}-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn an_export_that_writes_its_list_is_a_success_notice() {
    let env = restored();
    let dir = temp_dir("export");
    let path = dir.join("bowie.m3u8");
    let bowie = env.playlist_id("David Bowie");
    let op = env.vm.export(
        bowie,
        path.clone(),
        ExportFormat::M3U8,
        ExportMode::SingleFile,
    );
    assert_eq!(env.vm.pending_ops(), vec![(op, PendingOp::Export)]);
    let event = ended(&env, op, LongOpKind::ExportPlaylist);
    assert_eq!(
        env.vm.on_long_op(&event),
        Some(Notice::Exported {
            path: path.clone(),
            copied: 0,
            failed: 0
        })
    );
    assert!(path.exists());
    assert!(env.vm.pending_ops().is_empty());
    assert_eq!(env.vm.on_long_op(&event), None, "an operation ends once");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_failed_export_is_an_error_notice() {
    // Quirk fix 8.
    let env = restored();
    let path = PathBuf::from("/nonexistent-audoxidy-dir/list.m3u8");
    let op = env.vm.export(
        env.playlist_id("David Bowie"),
        path,
        ExportFormat::M3U8,
        ExportMode::SingleFile,
    );
    let event = ended(&env, op, LongOpKind::ExportPlaylist);
    assert!(matches!(event, LongOpEvent::Failed(_)));
    assert!(matches!(
        env.vm.on_long_op(&event),
        Some(Notice::ExportFailed(_))
    ));
}

#[test]
fn a_failed_import_is_an_error_notice() {
    // Quirk fix 8: the list names files that do not exist.
    let env = restored();
    let dir = temp_dir("import");
    let path = dir.join("missing.m3u");
    std::fs::write(&path, "#EXTM3U\n/nowhere/a.flac\n").unwrap();
    let op = env.vm.import(path);
    let event = ended(&env, op, LongOpKind::ImportPlaylist);
    assert!(matches!(
        env.vm.on_long_op(&event),
        Some(Notice::ImportFailed(_))
    ));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_cancelled_operation_and_someone_elses_say_nothing() {
    let env = restored();
    let info = |op_id, cancelled| LongOpInfo {
        op_id,
        kind: LongOpKind::ExportPlaylist,
        progress: None,
        error: Some("cancelled".into()),
        cancelled,
    };
    assert_eq!(
        env.vm
            .on_long_op(&LongOpEvent::Failed(info(OpId(999), false))),
        None
    );
    let dir = temp_dir("cancel");
    let op = env.vm.export(
        env.playlist_id("David Bowie"),
        dir.join("x.m3u8"),
        ExportFormat::M3U8,
        ExportMode::SingleFile,
    );
    env.wait_op(op);
    assert_eq!(
        env.vm.on_long_op(&LongOpEvent::Failed(info(op, true))),
        None
    );
    assert!(env.vm.pending_ops().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn adding_files_runs_on_the_active_playlist() {
    let env = restored();
    let dir = temp_dir("add");
    let op = env.vm.add_external(vec![dir.clone()]).unwrap();
    assert_eq!(env.vm.pending_ops(), vec![(op, PendingOp::AddFiles)]);
    let event = ended(&env, op, LongOpKind::ExternalBatch);
    assert_eq!(env.vm.on_long_op(&event), None);
    assert!(env.vm.pending_ops().is_empty());
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

#[test]
fn the_chevron_shows_once_the_estimated_tabs_pass_380_pixels() {
    assert_eq!(estimate_tab_width("Audoxidy"), 70.0);
    assert!(!tabs_overflow(["Audoxidy", "Oxidy Drift", "David Bowie"]));
    let long = [
        "Audoxidy",
        "Oxidy Drift",
        "David Bowie",
        "Una lista muy larga",
        "Otra más",
    ];
    assert!(tabs_overflow(long));
}

#[test]
fn a_tab_out_of_view_scrolls_to_15_pixels_from_the_edge() {
    let names = ["Audoxidy", "Oxidy Drift", "David Bowie", "Mixtape"];
    // 75 + 97.5 + 97.5 = 270: "Mixtape" (62.5) ends at 332.5.
    assert_eq!(tab_scroll_offset(&names, 3, 0.0, 380.0), None);
    assert_eq!(tab_scroll_offset(&names, 3, 0.0, 300.0), Some(255.0));
    assert_eq!(tab_scroll_offset(&names, 0, 100.0, 300.0), Some(0.0));
    assert_eq!(
        tab_scroll_offset(&names, 9, 0.0, 300.0),
        None,
        "no such tab"
    );
}
