//! Tests of the player view-model: the transport on the shared cursor with the
//! engine's fake, the gate before the restore, the volume wheel and its
//! deadlines, the marquee and the pointer-derived views.

use std::sync::Arc;
use std::time::{Duration, Instant};

use teksilo::core::widget_tree::WidgetTree;
use teksilo::prelude::AsyncRuntimeHandle;

use super::*;
use crate::frontend::events::{PlaybackEvent, PlaybackSnapshot};
use crate::frontend::fakes::{InMemoryFakes, PlayerCall};
use crate::frontend::{AppContext, Event};
use crate::teksilo_ui::app::session::RestoreStatus;
use crate::teksilo_ui::mock_data::screenshot_library;
use crate::teksilo_ui::player::PlayerPanel;
use crate::teksilo_ui::shell::main_window::AppHandles;
use crate::teksilo_ui::test_support::{layout, run_until, tree_with_async};

const TIMEOUT: Duration = Duration::from_secs(10);

/// Services over an in-memory context with its fakes, seeded with the mock
/// library; the session is not restored.
fn services_with_fakes() -> (AppServices, InMemoryFakes) {
    let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
    screenshot_library()
        .seed_on(&ctx.db)
        .recv_blocking()
        .unwrap();
    let services = AppServices::new(&AppHandles::new(ctx), AsyncRuntimeHandle::new());
    (services, fakes)
}

/// [`services_with_fakes`] with the restore finished.
fn restored() -> (AppServices, InMemoryFakes) {
    let (services, fakes) = services_with_fakes();
    services
        .session
        .start_restore(&services.ctx, &services.runtime);
    let status = services.session.status();
    assert!(run_until(&services.runtime, TIMEOUT, || status
        .get()
        .is_settled()));
    assert!(matches!(status.get(), RestoreStatus::Restored(_)));
    (services, fakes)
}

fn finish(services: &AppServices) {
    assert!(services.ctx.shutdown());
}

/// A playback state with `title` and `artist`, as the monitor reports one.
fn track_state(services: &AppServices, title: &str, artist: &str, playing: bool) -> Event {
    let mut state = PlaybackSnapshot::from_state(&services.ctx.player.snapshot());
    state.title = title.to_string();
    state.artist = artist.to_string();
    state.is_playing = playing;
    state.duration_secs = 200.0;
    Event::Playback(PlaybackEvent::TrackChanged(Arc::new(state)))
}

fn at(ms: u64, origin: Instant) -> Instant {
    origin + Duration::from_millis(ms)
}

/// The view-model of a panel mounted in a tree (its effects are registered
/// when the panel builds).
fn mounted(services: &AppServices) -> (WidgetTree, PlayerViewModel) {
    let vm = PlayerViewModel::new(services);
    let (mut tree, _runtime) = tree_with_async(&services.ctx);
    tree.add(PlayerPanel::new(vm.clone()));
    layout(&mut tree);
    (tree, vm)
}

// --- The gate before the restore ----------------------------------------------

#[test]
fn nothing_is_sent_to_the_engine_before_the_restore_settles() {
    let (services, fakes) = services_with_fakes();
    let vm = PlayerViewModel::new(&services);
    assert!(!vm.transport_enabled().get());
    assert!(!vm.play_pause());
    assert!(!vm.play());
    assert!(!vm.pause());
    assert!(!vm.stop());
    assert!(vm.next().is_none());
    assert!(vm.previous().is_none());
    assert!(!vm.seek_to(10.0));
    assert!(!vm.seek_by(5.0));
    assert!(vm.set_volume(0.5).is_none());
    assert!(vm.scroll_volume(1.0).is_none());
    assert_eq!(vm.volume().get(), None, "no bubble either");
    assert!(fakes.player.calls().is_empty());
    finish(&services);
}

#[test]
fn the_transport_is_enabled_once_the_restore_settles() {
    let (services, _fakes) = services_with_fakes();
    let vm = PlayerViewModel::new(&services);
    assert!(!vm.transport_enabled().get());
    services
        .session
        .start_restore(&services.ctx, &services.runtime);
    let enabled = vm.transport_enabled();
    assert!(run_until(&services.runtime, TIMEOUT, || enabled.get()));
    finish(&services);
}

// --- The transport ----------------------------------------------------------------

#[test]
fn play_pause_toggles_the_engine_on_the_shared_cursor() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let before = fakes.player.calls().len();
    assert!(vm.play_pause());
    assert!(
        fakes.player.calls()[before..].contains(&PlayerCall::TogglePlayPause),
        "{:?}",
        fakes.player.calls()
    );
    finish(&services);
}

#[test]
fn play_and_pause_set_the_play_state() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let before = fakes.player.calls().len();
    assert!(vm.play());
    assert!(vm.pause());
    let calls = &fakes.player.calls()[before..];
    assert!(calls.contains(&PlayerCall::SetPlaying(true)), "{calls:?}");
    assert!(calls.contains(&PlayerCall::SetPlaying(false)), "{calls:?}");
    finish(&services);
}

#[test]
fn stop_stops_the_engine_and_wakes_nothing() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    assert!(vm.stop());
    assert!(fakes.player.calls().contains(&PlayerCall::Stop));
    assert!(
        !vm.controls_active().get(),
        "Iced's Stop does not wake the controls"
    );
    finish(&services);
}

#[test]
fn next_and_previous_move_the_shared_cursor_and_load_the_song() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let version = services.session.cursor_version();
    let start = services
        .session
        .read_cursor(|cursor| cursor.playing_index())
        .flatten();
    let before = version.get();
    let outcome = vm.next().expect("the restore has settled");
    assert!(outcome.is_ok(), "{outcome:?}");
    assert!(version.get() > before, "the cursor moved");
    let loaded = fakes
        .player
        .calls()
        .iter()
        .filter(|call| matches!(call, PlayerCall::Load { .. }))
        .count();
    assert!(loaded >= 1);
    assert!(vm.next().expect("settled").is_ok());
    let after_next = services
        .session
        .read_cursor(|cursor| cursor.playing_index())
        .flatten();
    assert_ne!(after_next, start, "Next changed the playing song");
    assert!(vm.previous().expect("settled").is_ok());
    let after_previous = services
        .session
        .read_cursor(|cursor| cursor.playing_index())
        .flatten();
    assert_ne!(after_previous, after_next, "Previous moved the song again");
    finish(&services);
}

#[test]
fn a_seek_is_clamped_to_the_track_and_goes_to_the_engine() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    assert!(vm.next().expect("settled").is_ok(), "load a track first");
    let duration = services.ctx.player.snapshot().total_duration_sec;
    assert!(duration > 0.0, "the loaded track has a duration");
    assert!(vm.seek_to(duration + 100.0));
    assert!(
        fakes.player.calls().contains(&PlayerCall::Seek(duration)),
        "{:?}",
        fakes.player.calls()
    );
    assert!(vm.seek_by(-10_000.0));
    assert!(fakes.player.calls().contains(&PlayerCall::Seek(0.0)));
    finish(&services);
}

#[test]
fn the_slider_seeks_to_a_fraction_of_the_duration() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    assert!(vm.next().expect("settled").is_ok(), "load a track first");
    services
        .now_playing
        .on_event(&track_state(&services, "A", "B", true));
    let duration = services.now_playing.duration_secs().get();
    assert_eq!(duration, 200.0);
    assert!(vm.seek_fraction(0.25));
    assert!(fakes.player.calls().contains(&PlayerCall::Seek(50.0)));
    finish(&services);
}

#[test]
fn a_seek_is_ignored_while_the_duration_is_unknown() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let mut state = PlaybackSnapshot::from_state(&services.ctx.player.snapshot());
    state.duration_secs = 0.0;
    services
        .now_playing
        .on_event(&Event::Playback(PlaybackEvent::TrackChanged(Arc::new(
            state,
        ))));
    let before = fakes.player.calls().len();
    assert!(!vm.seek_fraction(0.5));
    assert!(!vm.duration_known().get());
    assert_eq!(fakes.player.calls().len(), before);
    finish(&services);
}

#[test]
fn a_set_volume_is_clamped_and_reaches_the_engine() {
    let (services, fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    assert_eq!(vm.set_volume(1.5), Some(1.0));
    assert_eq!(vm.set_volume(f32::NAN), Some(0.0));
    assert!(fakes.player.calls().contains(&PlayerCall::SetVolume(1.0)));
    finish(&services);
}

// --- The volume wheel and its bubble -----------------------------------------------

#[test]
fn a_wheel_step_moves_the_volume_five_percent_and_shows_the_bubble() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    vm.set_volume(0.5);
    let t0 = Instant::now();
    assert_eq!(vm.scroll_volume_at(t0, 1.0), Some(55.0));
    assert_eq!(vm.volume().get(), Some(55.0));
    assert_eq!(vm.scroll_volume_at(t0, -1.0), Some(50.0));
    assert_eq!(vm.scroll_volume_at(t0, -0.5), Some(45.0));
    assert!((services.ctx.player.snapshot().volume - 0.45).abs() < 1e-6);
    finish(&services);
}

#[test]
fn the_volume_stays_between_zero_and_one_hundred_percent() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    vm.set_volume(0.98);
    let t0 = Instant::now();
    assert_eq!(vm.scroll_volume_at(t0, 1.0), Some(100.0));
    assert_eq!(vm.scroll_volume_at(t0, 1.0), Some(100.0));
    vm.set_volume(0.02);
    assert_eq!(vm.scroll_volume_at(t0, -1.0), Some(0.0));
    assert_eq!(vm.scroll_volume_at(t0, -1.0), Some(0.0));
    assert_eq!(PlayerViewModel::volume_icon(0.0), VolumeIcon::Off);
    finish(&services);
}

#[test]
fn the_bubble_hides_1500_ms_after_the_last_wheel_step() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let t0 = Instant::now();
    vm.scroll_volume_at(t0, 1.0);
    vm.on_tick(at(1499, t0));
    assert!(vm.volume().get().is_some());
    vm.on_tick(at(1500, t0));
    assert_eq!(vm.volume().get(), None);
    finish(&services);
}

#[test]
fn a_mouse_move_hides_the_bubble_one_second_after_the_move() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let t0 = Instant::now();
    vm.scroll_volume_at(t0, 1.0);
    vm.pointer_moved_at(at(200, t0), Some((200.0, 200.0)));
    vm.on_tick(at(1199, t0));
    assert!(vm.volume().get().is_some());
    vm.on_tick(at(1200, t0));
    assert_eq!(vm.volume().get(), None);
    finish(&services);
}

#[test]
fn a_click_hides_the_bubble_at_once() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    vm.scroll_volume(1.0);
    assert!(vm.volume().get().is_some());
    vm.clicked();
    assert_eq!(vm.volume().get(), None);
    finish(&services);
}

#[test]
fn a_transport_action_hides_the_bubble_and_wakes_the_controls() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    vm.scroll_volume(1.0);
    assert!(vm.play_pause());
    assert_eq!(vm.volume().get(), None);
    assert!(vm.controls_active().get());
    finish(&services);
}

#[test]
fn the_wake_up_is_set_to_the_earliest_deadline() {
    let (services, _fakes) = restored();
    let (tree, vm) = mounted(&services);
    let wake = tree.wake_at_handle();
    wake.set(None);
    let t0 = Instant::now();
    vm.scroll_volume_at(t0, 1.0);
    // The activity window has not started: only the bubble's deadline.
    assert_eq!(wake.get(), Some(at(1500, t0)));
    vm.pointer_moved_at(at(100, t0), Some((10.0, 10.0)));
    assert_eq!(
        wake.get(),
        Some(at(1100, t0)),
        "the move shortened the bubble's timeout, and the earlier deadline holds"
    );
    // Another party's earlier deadline is not postponed.
    wake.set(Some(at(50, t0)));
    vm.pointer_moved_at(at(120, t0), Some((10.0, 10.0)));
    assert_eq!(wake.get(), Some(at(50, t0)));
    finish(&services);
}

#[test]
fn a_frame_tick_runs_what_is_due() {
    let (services, _fakes) = restored();
    let (tree, vm) = mounted(&services);
    vm.scroll_volume_at(Instant::now() - Duration::from_secs(5), 1.0);
    assert!(vm.volume().get().is_some());
    // The tick effect reads the real clock: the deadline is long past.
    tree.frame_tick().set(16.0);
    assert_eq!(vm.volume().get(), None);
    finish(&services);
}

// --- The controls' activity -----------------------------------------------------------

#[test]
fn the_controls_are_active_for_1500_ms_after_a_mouse_move() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let t0 = Instant::now();
    assert!(!vm.controls_active().get());
    vm.pointer_moved_at(t0, Some((200.0, 200.0)));
    assert!(vm.controls_active().get());
    vm.on_tick(at(1499, t0));
    assert!(vm.controls_active().get());
    vm.on_tick(at(1500, t0));
    assert!(!vm.controls_active().get());
    finish(&services);
}

#[test]
fn the_zone_under_the_pointer_shows_feedback_only_while_active() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let t0 = Instant::now();
    vm.pointer_moved_at(t0, Some((50.0, 200.0)));
    assert_eq!(vm.hover_zone().get(), HoverZone::Previous);
    vm.pointer_moved_at(at(10, t0), Some((200.0, 200.0)));
    assert_eq!(vm.hover_zone().get(), HoverZone::PlayPause);
    vm.pointer_moved_at(at(20, t0), Some((300.0, 200.0)));
    assert_eq!(vm.hover_zone().get(), HoverZone::Next);
    vm.on_tick(at(2000, t0));
    assert_eq!(vm.hover_zone().get(), HoverZone::None, "quiet again");
    finish(&services);
}

#[test]
fn no_zone_shows_feedback_over_the_top_row_the_seek_bar_or_outside() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let t0 = Instant::now();
    for point in [(200.0, 20.0), (200.0, 380.0)] {
        vm.pointer_moved_at(t0, Some(point));
        assert_eq!(vm.hover_zone().get(), HoverZone::None, "{point:?}");
    }
    vm.pointer_moved_at(t0, Some((200.0, 200.0)));
    vm.pointer_moved_at(t0, None);
    assert_eq!(vm.hover_zone().get(), HoverZone::None);
    finish(&services);
}

#[test]
fn the_zones_show_no_feedback_while_the_volume_bubble_or_the_menu_is_up() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let t0 = Instant::now();
    vm.pointer_moved_at(t0, Some((200.0, 200.0)));
    assert_eq!(vm.hover_zone().get(), HoverZone::PlayPause);
    vm.scroll_volume_at(at(5, t0), 1.0);
    assert_eq!(vm.hover_zone().get(), HoverZone::None, "the bubble is up");
    vm.on_tick(at(1600, t0));
    vm.pointer_moved_at(at(1700, t0), Some((200.0, 200.0)));
    assert_eq!(vm.hover_zone().get(), HoverZone::PlayPause);
    vm.set_menu_open(true);
    assert_eq!(vm.hover_zone().get(), HoverZone::None, "the menu is open");
    vm.set_menu_open(false);
    assert_eq!(vm.hover_zone().get(), HoverZone::PlayPause);
    finish(&services);
}

#[test]
fn the_seek_tooltip_names_the_time_under_the_pointer_while_active() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    services
        .now_playing
        .on_event(&track_state(&services, "A", "B", true));
    let t0 = Instant::now();
    vm.pointer_moved_at(t0, Some((13.0 + 188.0, 380.0)));
    let hover = vm.seek_hover().get().expect("over the bar");
    assert_eq!(hover.text, "01:40");
    assert_eq!((hover.left, hover.top), (186.0, 350.0));
    vm.pointer_moved_at(at(1, t0), Some((200.0, 200.0)));
    assert_eq!(vm.seek_hover().get(), None);
    vm.pointer_moved_at(at(2, t0), Some((200.0, 380.0)));
    vm.on_tick(at(3000, t0));
    assert_eq!(vm.seek_hover().get(), None, "no tooltip once quiet");
    finish(&services);
}

// --- The track ---------------------------------------------------------------------------

#[test]
fn the_title_and_artist_follow_the_playing_track() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, "Heroes", "David Bowie", true));
    assert_eq!(vm.display_title().get(), "Heroes");
    assert_eq!(vm.display_artist().get(), "David Bowie");
    assert!(!vm.marquee_running().get(), "short text does not scroll");
    assert_eq!(vm.title_tooltip().get(), None);
    finish(&services);
}

#[test]
fn a_title_or_artist_over_35_bytes_has_a_tooltip_with_the_full_text() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    let long = "A long title that is over thirty five bytes";
    services
        .now_playing
        .on_event(&track_state(&services, long, "Short", true));
    assert_eq!(vm.title_tooltip().get().as_deref(), Some(long));
    assert_eq!(vm.artist_tooltip().get(), None);
    services
        .now_playing
        .on_event(&track_state(&services, "Short", long, true));
    assert_eq!(vm.title_tooltip().get(), None);
    assert_eq!(vm.artist_tooltip().get().as_deref(), Some(long));
    finish(&services);
}

#[test]
fn the_elapsed_time_the_slider_and_the_info_follow_the_position() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, "A", "B", false));
    assert!(vm.duration_known().get());
    assert!(!vm.info_visible().get(), "stopped at the start: no info");
    services
        .now_playing
        .on_event(&Event::Playback(PlaybackEvent::PositionChanged {
            position_secs: 75.0,
            duration_secs: 200.0,
        }));
    assert_eq!(vm.elapsed().get(), "01:15");
    assert!((vm.seek_value().get() - 0.375).abs() < 1e-6);
    assert!(vm.info_visible().get(), "paused past the start still shows");
    finish(&services);
}

#[test]
fn the_info_shows_while_playing_even_at_the_start() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, "A", "B", true));
    assert!(vm.info_visible().get());
    finish(&services);
}

// --- The marquee -------------------------------------------------------------------------

const LONG_TITLE: &str = "A title that is far too long to fit in the player slot";

#[test]
fn a_long_title_scrolls_while_the_track_plays() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, LONG_TITLE, "B", true));
    assert!(vm.marquee_running().get());
    let chars: Vec<char> = LONG_TITLE.chars().collect();
    let start = vm.display_title().get();
    assert_eq!(start, chars[0..39].iter().collect::<String>());
    // The window moves with the clock, a character per 500 ms.
    let t0 = Instant::now();
    vm.inner.timers.borrow_mut().clock.reset(t0);
    vm.on_tick(at(1000, t0));
    assert_eq!(
        vm.display_title().get(),
        chars[2..41].iter().collect::<String>()
    );
    finish(&services);
}

#[test]
fn the_marquee_stops_when_the_track_pauses_and_keeps_its_place() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, LONG_TITLE, "B", true));
    let t0 = Instant::now();
    vm.inner.timers.borrow_mut().clock.reset(t0);
    vm.update_display(at(1500, t0));
    let scrolled = vm.display_title().get();
    services
        .now_playing
        .on_event(&Event::Playback(PlaybackEvent::StateChanged(Arc::new({
            let mut state = PlaybackSnapshot::from_state(&services.ctx.player.snapshot());
            state.is_playing = false;
            state
        }))));
    assert!(!vm.marquee_running().get(), "paused: no guard");
    assert!(!vm.inner.timers.borrow().clock.is_running());
    assert!(
        !scrolled.is_empty() && vm.display_title().get().chars().count() == 39,
        "the window stays where it was"
    );
    finish(&services);
}

#[test]
fn a_new_title_starts_the_marquee_from_the_beginning() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, LONG_TITLE, "B", true));
    let t0 = Instant::now();
    vm.inner
        .timers
        .borrow_mut()
        .clock
        .reset(t0 - Duration::from_secs(10));
    vm.update_display(t0);
    let other = "Another title that is also far too long for the slot";
    services
        .now_playing
        .on_event(&track_state(&services, other, "B", true));
    let chars: Vec<char> = other.chars().collect();
    assert_eq!(
        vm.display_title().get(),
        chars[0..39].iter().collect::<String>()
    );
    finish(&services);
}

#[test]
fn a_long_artist_scrolls_over_a_forty_character_window() {
    let (services, _fakes) = restored();
    let (_tree, vm) = mounted(&services);
    services
        .now_playing
        .on_event(&track_state(&services, "Short", &"x".repeat(60), true));
    assert!(vm.marquee_running().get());
    assert_eq!(vm.display_artist().get().chars().count(), 40);
    assert_eq!(vm.display_title().get(), "Short");
    finish(&services);
}

// --- The menu's commands -------------------------------------------------------------------

#[test]
fn the_audio_entries_toggle_the_audio_center_on_their_tab() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    let link = &services.links.audio_center;
    vm.open_equalizer();
    assert!(link.open.get());
    assert_eq!(link.tab.get(), audio_center_tab::EQUALIZER);
    vm.open_effects();
    assert_eq!(link.tab.get(), audio_center_tab::EFFECTS);
    vm.open_effects();
    assert!(!link.open.get(), "the open tab closes it");
    vm.open_audio_config();
    assert!(link.open.get());
    assert_eq!(link.tab.get(), audio_center_tab::CONFIG);
    finish(&services);
}

#[test]
fn scanning_an_empty_path_fails_and_a_folder_starts_a_scan() {
    let (services, _fakes) = restored();
    let vm = PlayerViewModel::new(&services);
    assert!(vm.scan_folder("  ").is_err());
    let dir = std::env::temp_dir().join(format!("audoxidy-player-scan-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert!(vm.scan_folder(&dir.to_string_lossy()).is_ok());
    let _ = std::fs::remove_dir_all(&dir);
    finish(&services);
}
