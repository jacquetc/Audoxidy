use super::*;
use crate::db::database::PlaylistData;
use crate::frontend::commands::playlist::load_playlist;
use crate::frontend::fakes::{InMemoryFakes, MediaCall, PlayerCall};
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::keys;
use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture, SeededLibrary};
use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};

/// The titles of the five songs of the test playlist.
pub(crate) const TITLES: [&str; 5] = ["Alpha", "Bravo", "Charlie", "Delta", "Echo"];

/// The path of song `n` (1 to 5): songs 1 and 2 are in folder `/m/x`, songs 3
/// to 5 in `/m/y`.
pub(crate) fn path(n: usize) -> String {
    let folder = if n <= 2 { "/m/x" } else { "/m/y" };
    format!("{folder}/{n:02}.flac")
}

/// The gains of song 2, the only song with ReplayGain.
pub(crate) const GAINS_2: ReplayGains = ReplayGains {
    track: Some(-6.5),
    album: Some(-5.0),
};

/// The five songs, in no playlist.
pub(crate) fn library() -> LibraryFixture {
    let mut fixture = LibraryFixture::new();
    for n in 1..=5 {
        // The artist and album names share no letter with the searches the
        // tests use, which also match them.
        let album = if n <= 2 { "Qx" } else { "Qy" };
        let mut song = FixtureSong::new(&path(n), TITLES[n - 1], "Zed", album);
        song.duration_secs = 200.0;
        song.with_cover = n > 2;
        if n == 2 {
            song.track_gain = GAINS_2.track;
            song.album_gain = GAINS_2.album;
        }
        fixture = fixture.song(song);
    }
    fixture
}

/// The five songs, the playlist "List" holding songs `list` (with `disabled`
/// disabled) and "Other" holding songs 4 and 1.
pub(crate) fn fixture_with(list: &[usize], disabled: &[usize]) -> LibraryFixture {
    fn paths(numbers: &[usize]) -> Vec<String> {
        numbers.iter().map(|&n| path(n)).collect()
    }
    fn refs(paths: &[String]) -> Vec<&str> {
        paths.iter().map(String::as_str).collect()
    }
    let (list, disabled, other) = (paths(list), paths(disabled), paths(&[4, 1]));
    library()
        .playlist_with_disabled("List", &refs(&list), &refs(&disabled))
        .playlist("Other", &refs(&other))
}

/// Five songs in two folders, all in the playlist "List" (linear indices: 0
/// the separator of `/m/x`, 1 and 2 songs 1 and 2, 3 the separator of
/// `/m/y`, 4 to 6 songs 3 to 5), and "Other" holding songs 4 and 1.
pub(crate) fn fixture() -> LibraryFixture {
    fixture_with(&[1, 2, 3, 4, 5], &[])
}

/// A seeded in-memory context with the cursor of the playlist "List".
pub(crate) struct Rig {
    pub(crate) ctx: AppContext,
    pub(crate) fakes: InMemoryFakes,
    pub(crate) seeded: SeededLibrary,
    pub(crate) cursor: PlaybackCursor,
}

impl Rig {
    pub(crate) fn new() -> Self {
        Self::with(fixture())
    }

    pub(crate) fn with(fixture: LibraryFixture) -> Self {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        let seeded = fixture.seed_on(&ctx.db).recv_blocking().unwrap();
        let list = seeded.playlist("List").unwrap().id;
        let cursor = PlaybackCursor::new(load_playlist(&ctx, list).recv_blocking().unwrap());
        Self {
            ctx,
            fakes,
            seeded,
            cursor,
        }
    }

    /// The id of song `n`.
    pub(crate) fn id(&self, n: usize) -> i64 {
        self.seeded.song_id(&path(n)).unwrap()
    }

    /// The song number of a song id.
    pub(crate) fn number(&self, song_id: i64) -> usize {
        (1..=5).find(|&n| self.id(n) == song_id).unwrap()
    }

    pub(crate) fn playlist_id(&self, name: &str) -> i64 {
        self.seeded.playlist(name).unwrap().id
    }

    /// Waits until every job queued so far on both workers has run.
    pub(crate) fn flush(&self) {
        flush(&self.ctx);
    }

    /// The stored row of a playlist.
    pub(crate) fn stored(&self, playlist_id: i64) -> PlaylistData {
        self.flush();
        self.ctx
            .db
            .read(move |db| Ok(db.get_playlist_by_id(playlist_id)?))
            .recv_blocking()
            .unwrap()
            .unwrap()
    }

    /// The stored shuffle session of a playlist.
    pub(crate) fn stored_session(&self, playlist_id: i64) -> Option<ShuffleSession> {
        self.flush();
        self.ctx
            .db
            .read(move |db| Ok(db.load_shuffle_session(playlist_id)?))
            .recv_blocking()
            .unwrap()
    }

    /// Plays song `n` and forgets the calls that made it.
    pub(crate) fn start(&mut self, n: usize) {
        let id = self.id(n);
        play_song(&self.ctx, &mut self.cursor, id).unwrap();
        self.flush();
        self.fakes.player.take_calls();
    }

    /// The song number the cursor plays.
    pub(crate) fn playing(&self) -> Option<usize> {
        self.cursor.playing_song_id().map(|id| self.number(id))
    }

    pub(crate) fn take_calls(&self) -> Vec<PlayerCall> {
        self.flush();
        self.fakes.player.take_calls()
    }

    /// Turns the manual crossfade on, with `ms` milliseconds.
    pub(crate) fn enable_manual_crossfade(&self, ms: f32) {
        let state = self.fakes.player.state();
        let mut state = state.write();
        state.crossfade_enabled = true;
        state.crossfade_manual_enabled = true;
        state.crossfade_manual_ms = ms;
    }
}

/// Waits until every job queued so far on both workers has run.
pub(crate) fn flush(ctx: &AppContext) {
    ctx.db.read(|_| Ok(())).recv_blocking().unwrap();
    ctx.engine.run(|_| Ok(())).recv_blocking().unwrap();
    ctx.db.read(|_| Ok(())).recv_blocking().unwrap();
}

pub(crate) fn load(n: usize, gains: ReplayGains) -> PlayerCall {
    PlayerCall::Load {
        path: path(n),
        gains,
    }
}

pub(crate) fn plain(n: usize) -> PlayerCall {
    load(n, ReplayGains::default())
}

fn purges(calls: &[PlayerCall]) -> usize {
    calls
        .iter()
        .filter(|call| **call == PlayerCall::PurgeBuffers)
        .count()
}

// --- The cursor -----------------------------------------------------------------

#[test]
fn a_new_cursor_holds_the_playlist_its_gains_and_its_saved_modes() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let cursor = &rig.cursor;
        assert_eq!(cursor.playlist_id(), rig.playlist_id("List"));
        assert_eq!(cursor.groups().len(), 2);
        assert_eq!(nav::count_linear_items(cursor.active_groups()), 7);
        assert_eq!(cursor.gains(rig.id(2)), GAINS_2);
        assert_eq!(cursor.gains(rig.id(1)), ReplayGains::default());
        assert_eq!(cursor.repeat(), RepeatMode::Off);
        assert!(!cursor.shuffle_active());
        assert_eq!(cursor.playing_song_id(), None);
        assert!(cursor.next_song().is_none(), "nothing plays yet");
    });
}

#[test]
fn quirk_11_the_playing_song_is_tracked_by_id_under_a_search() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(4);
        assert_eq!(rig.cursor.playing_index(), Some(5));

        rig.cursor.set_search("delta");
        assert!(rig.cursor.is_filtered());
        assert_eq!(
            rig.cursor.playing_index(),
            Some(1),
            "the row of Delta in the search result, not linear index 5"
        );
        assert_eq!(rig.cursor.song_at(1).unwrap().song_id, rig.id(4));

        rig.cursor.set_search("bravo");
        assert_eq!(rig.cursor.playing_index(), None, "Delta is hidden");
        assert_eq!(rig.cursor.playing_song_id(), Some(rig.id(4)));

        rig.cursor.set_search("");
        assert!(!rig.cursor.is_filtered());
        assert_eq!(rig.cursor.playing_index(), Some(5));
    });
}

#[test]
fn next_follows_the_search_result_from_its_top_when_the_playing_song_is_hidden() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.cursor.set_search("arl");
        assert_eq!(nav::count_linear_items(rig.cursor.active_groups()), 2);
        assert_eq!(
            next(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(3))
        );
        assert_eq!(
            rig.playing(),
            Some(3),
            "Charlie, the first row of the result"
        );
    });
}

// --- Transport ------------------------------------------------------------------

#[test]
fn play_and_pause_set_the_play_state_and_persist_it() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        let list = rig.playlist_id("List");

        pause(&rig.ctx, &rig.cursor);
        let calls = rig.take_calls();
        assert_eq!(calls[0], PlayerCall::SetPlaying(false));
        assert_eq!(purges(&calls), 1, "a pause purges the buffers");
        assert!(!rig.stored(list).is_playing);

        play(&rig.ctx, &rig.cursor);
        let calls = rig.take_calls();
        assert_eq!(calls, vec![PlayerCall::SetPlaying(true)]);
        let stored = rig.stored(list);
        assert!(stored.is_playing);
        assert_eq!(stored.last_song_id, Some(rig.id(1)));

        set_playing(&rig.ctx, &rig.cursor, false);
        assert_eq!(purges(&rig.take_calls()), 1);
    });
}

#[test]
fn a_pause_quickly_followed_by_play_skips_the_purge() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let blocker = rig.ctx.engine.run(move |_| {
            let _ = started_tx.send(());
            let _ = release_rx.recv_timeout(TEST_TIMEOUT);
            Ok(())
        });
        started_rx.recv_timeout(TEST_TIMEOUT).unwrap();

        pause(&rig.ctx, &rig.cursor);
        play(&rig.ctx, &rig.cursor);
        release_tx.send(()).unwrap();
        blocker.recv_blocking().unwrap();

        let calls = rig.take_calls();
        assert_eq!(
            calls,
            vec![PlayerCall::SetPlaying(false), PlayerCall::SetPlaying(true)]
        );
        assert_eq!(purges(&calls), 0, "playback resumed before the purge ran");
    });
}

#[test]
fn toggle_play_pause_flips_the_state_and_purges_only_when_it_pauses() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        assert!(!toggle_play_pause(&rig.ctx, &rig.cursor));
        let calls = rig.take_calls();
        assert_eq!(calls[0], PlayerCall::TogglePlayPause);
        assert_eq!(purges(&calls), 1);
        assert!(!rig.stored(rig.playlist_id("List")).is_playing);

        assert!(toggle_play_pause(&rig.ctx, &rig.cursor));
        assert_eq!(rig.take_calls(), vec![PlayerCall::TogglePlayPause]);
        assert!(rig.stored(rig.playlist_id("List")).is_playing);
    });
}

#[test]
fn stop_stops_then_purges_without_persisting() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.fakes.player.advance(30.0);
        persist_state_now(&rig);
        stop(&rig.ctx);
        assert_eq!(
            rig.take_calls(),
            vec![PlayerCall::Stop, PlayerCall::PurgeBuffers]
        );
        let stored = rig.stored(rig.playlist_id("List"));
        assert_eq!(
            stored.last_pos_sec, 30.0,
            "Stop does not persist, as in Iced"
        );
        assert!(stored.is_playing);
    });
}

fn persist_state_now(rig: &Rig) {
    playlist::persist_state(&rig.ctx, &rig.cursor)
        .recv_blocking()
        .unwrap();
}

#[test]
fn seeking_clamps_to_the_track_and_persists_the_position() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.fakes.player.set_track_duration(&path(1), 200.0);
        rig.start(1);
        assert!(seek(&rig.ctx, &rig.cursor, 250.0));
        assert_eq!(rig.take_calls(), vec![PlayerCall::Seek(200.0)]);
        assert_eq!(rig.stored(rig.playlist_id("List")).last_pos_sec, 200.0);

        assert!(seek(&rig.ctx, &rig.cursor, 50.0));
        assert!(seek_by(&rig.ctx, &rig.cursor, 10.0));
        assert!(seek_by(&rig.ctx, &rig.cursor, -500.0));
        assert_eq!(
            rig.take_calls(),
            vec![
                PlayerCall::Seek(50.0),
                PlayerCall::Seek(60.0),
                PlayerCall::Seek(0.0)
            ]
        );
        assert_eq!(rig.stored(rig.playlist_id("List")).last_pos_sec, 0.0);
    });
}

#[test]
fn seeking_is_ignored_while_the_duration_is_unknown() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.fakes.player.set_track_duration(&path(1), 0.0);
        rig.start(1);
        assert!(!seek(&rig.ctx, &rig.cursor, 20.0));
        assert!(!seek_by(&rig.ctx, &rig.cursor, 10.0));
        assert!(rig.take_calls().is_empty());
    });
}

fn stored_volume(rig: &Rig) -> Option<String> {
    rig.flush();
    rig.ctx
        .db
        .read(|db| Ok(db.get_setting(keys::PLAYER_VOLUME)))
        .recv_blocking()
        .unwrap()
}

#[test]
fn set_volume_clamps_applies_and_persists_the_volume() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        assert_eq!(set_volume(&rig.ctx, 0.42), 0.42);
        assert_eq!(stored_volume(&rig).as_deref(), Some("0.4200"));
        assert_eq!(set_volume(&rig.ctx, 1.7), 1.0);
        assert_eq!(set_volume(&rig.ctx, f32::NAN), 0.0);
        assert_eq!(
            rig.take_calls(),
            vec![
                PlayerCall::SetVolume(0.42),
                PlayerCall::SetVolume(1.0),
                PlayerCall::SetVolume(0.0)
            ]
        );
        assert_eq!(stored_volume(&rig).as_deref(), Some("0.0000"));
    });
}

#[test]
fn the_wheel_moves_the_volume_by_five_percent_and_persists_it() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        set_volume(&rig.ctx, 0.5);
        assert!((scroll_volume(&rig.ctx, 1.0) - 55.0).abs() < 1e-3);
        assert!((rig.fakes.player.snapshot().volume - 0.55).abs() < 1e-6);
        assert_eq!(stored_volume(&rig).as_deref(), Some("0.5500"));
        assert!((scroll_volume(&rig.ctx, -3.0) - 50.0).abs() < 1e-3);

        set_volume(&rig.ctx, 0.98);
        assert_eq!(scroll_volume(&rig.ctx, 1.0), 100.0, "clamped");
        set_volume(&rig.ctx, 0.02);
        assert_eq!(scroll_volume(&rig.ctx, -1.0), 0.0, "clamped");
        assert_eq!(stored_volume(&rig).as_deref(), Some("0.0000"));
    });
}

// --- Playing a song ---------------------------------------------------------------

#[test]
fn playing_a_song_takes_the_eof_then_loads_it_with_its_gains_and_plays() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.fakes.player.state().write().eof_reached = true;
        let id = rig.id(2);
        let change = play_song(&rig.ctx, &mut rig.cursor, id).unwrap();
        assert_eq!(change, TrackChange::Loaded(rig.id(2)));
        assert_eq!(rig.take_calls(), vec![load(2, GAINS_2), PlayerCall::Play]);
        assert!(!rig.fakes.player.snapshot().eof_reached);
        assert_eq!(rig.playing(), Some(2));
        assert!(rig.ctx.preload.finished_at().is_some());
        let stored = rig.stored(rig.playlist_id("List"));
        assert_eq!(stored.last_song_id, Some(rig.id(2)));
        assert_eq!(stored.last_pos_sec, 0.0);
        assert!(stored.is_playing);
    });
}

#[test]
fn playing_the_playing_song_restarts_it() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(3);
        rig.fakes.player.advance(42.0);
        let id = rig.id(3);
        let change = play_song(&rig.ctx, &mut rig.cursor, id).unwrap();
        assert_eq!(change, TrackChange::Restarted(rig.id(3)));
        assert_eq!(
            rig.take_calls(),
            vec![PlayerCall::Seek(0.0), PlayerCall::Play]
        );
        assert_eq!(rig.fakes.player.snapshot().current_pos_sec, 0.0);
    });
}

#[test]
fn a_paused_song_played_again_is_loaded_again() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(3);
        pause(&rig.ctx, &rig.cursor);
        rig.take_calls();
        let id = rig.id(3);
        let change = play_song(&rig.ctx, &mut rig.cursor, id).unwrap();
        assert_eq!(change, TrackChange::Loaded(rig.id(3)));
        assert_eq!(rig.take_calls(), vec![plain(3), PlayerCall::Play]);
    });
}

#[test]
fn a_manual_play_in_shuffle_joins_the_history_and_leaves_the_order() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        assert!(toggle_shuffle(&rig.ctx, &mut rig.cursor));
        let pending = rig.cursor.shuffle_session().unwrap().shuffle_order.clone();
        let chosen = pending[pending.len() - 1];
        play_song(&rig.ctx, &mut rig.cursor, chosen).unwrap();
        let session = rig.cursor.shuffle_session().unwrap();
        assert_eq!(session.history.last(), Some(&chosen));
        assert!(!session.shuffle_order.contains(&chosen));
    });
}

#[test]
fn playing_a_separator_plays_the_first_enabled_song_of_its_folder() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::with(fixture_with(&[1, 3, 4, 5], &[3]));
        // Linear: 0 /m/x, 1 Alpha, 2 /m/y, 3 Charlie (disabled), 4 Delta, 5 Echo.
        let change = play_item(&rig.ctx, &mut rig.cursor, 2).unwrap();
        assert_eq!(change, TrackChange::Loaded(rig.id(4)));
        assert_eq!(
            play_item(&rig.ctx, &mut rig.cursor, 99).unwrap(),
            TrackChange::Unchanged
        );
    });
}

#[test]
fn playing_a_song_outside_the_playlist_is_refused() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        assert!(matches!(
            play_song(&rig.ctx, &mut rig.cursor, 9_999),
            Err(FrontendError::InvalidInput(_))
        ));
        assert!(rig.take_calls().is_empty());
        assert_eq!(rig.playing(), None);
    });
}

// --- Next, Previous and the end of a track -----------------------------------

#[test]
fn next_loads_and_plays_the_next_song_and_persists_it() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.fakes.player.advance(80.0);
        assert_eq!(
            next(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(2))
        );
        assert_eq!(rig.take_calls(), vec![load(2, GAINS_2), PlayerCall::Play]);
        let stored = rig.stored(rig.playlist_id("List"));
        assert_eq!(stored.last_song_id, Some(rig.id(2)));
        assert_eq!(stored.last_pos_sec, 0.0, "the new track starts at 0");
        assert!(rig.ctx.preload.finished_at().is_some());
    });
}

#[test]
fn next_with_the_manual_crossfade_preloads_then_crossfades_into_the_next_song() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.enable_manual_crossfade(1500.0);
        assert_eq!(
            next(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(2))
        );
        assert_eq!(
            rig.take_calls(),
            vec![
                PlayerCall::Preload {
                    path: path(2),
                    gains: GAINS_2
                },
                PlayerCall::CrossfadeNext(1500.0),
                PlayerCall::LoadSkipped { path: path(2) },
                PlayerCall::Play,
            ],
            "the crossfade already plays song 2, so its load is skipped"
        );
        assert_eq!(rig.fakes.player.snapshot().path, path(2));
    });
}

#[test]
fn the_crossfade_does_not_preload_a_song_twice() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.enable_manual_crossfade(1000.0);
        maybe_preload(&rig.ctx, &rig.cursor, 170.0, 180.0);
        rig.take_calls();
        next(&rig.ctx, &mut rig.cursor).unwrap();
        let calls = rig.take_calls();
        assert_eq!(calls[0], PlayerCall::CrossfadeNext(1000.0));
        assert!(
            !calls
                .iter()
                .any(|c| matches!(c, PlayerCall::Preload { .. }))
        );
    });
}

#[test]
fn the_crossfade_needs_the_group_the_manual_switch_and_a_length() {
    with_timeout(TEST_TIMEOUT, || {
        for (group, manual, ms) in [
            (false, true, 1000.0),
            (true, false, 1000.0),
            (true, true, 0.0),
        ] {
            let mut rig = Rig::new();
            rig.start(1);
            {
                let state = rig.fakes.player.state();
                let mut state = state.write();
                state.crossfade_enabled = group;
                state.crossfade_manual_enabled = manual;
                state.crossfade_manual_ms = ms;
            }
            next(&rig.ctx, &mut rig.cursor).unwrap();
            assert_eq!(
                rig.take_calls(),
                vec![load(2, GAINS_2), PlayerCall::Play],
                "{group} {manual} {ms}"
            );
        }
    });
}

#[test]
fn next_at_the_end_without_repeat_changes_nothing() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(5);
        assert_eq!(
            next(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Unchanged
        );
        assert!(rig.take_calls().is_empty());
        assert_eq!(rig.playing(), Some(5));
    });
}

#[test]
fn the_end_of_a_track_advances_once_and_a_repeated_event_is_a_no_op() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.fakes.player.finish_track();
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::Advanced(rig.id(2))
        );
        assert_eq!(rig.take_calls(), vec![load(2, GAINS_2), PlayerCall::Play]);
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::NotPending
        );
        assert!(rig.take_calls().is_empty());
        assert_eq!(rig.playing(), Some(2));
        assert_eq!(
            rig.stored(rig.playlist_id("List")).last_song_id,
            Some(rig.id(2))
        );
    });
}

#[test]
fn the_load_that_confirms_a_promoted_track_is_skipped() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        maybe_preload(&rig.ctx, &rig.cursor, 170.0, 180.0);
        rig.fakes.player.finish_track();
        rig.take_calls();
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::Advanced(rig.id(2))
        );
        assert_eq!(
            rig.take_calls(),
            vec![PlayerCall::LoadSkipped { path: path(2) }, PlayerCall::Play]
        );
        assert!(!rig.ctx.preload.snapshot().is_active());
    });
}

#[test]
fn quirk_28_a_manual_next_during_a_pending_end_of_track_moves_once_from_the_promoted_track() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        maybe_preload(&rig.ctx, &rig.cursor, 170.0, 180.0);
        // Song 1 ends and the engine promotes song 2 before the UI sees it.
        rig.fakes.player.finish_track();
        rig.take_calls();

        assert_eq!(
            next(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(3)),
            "Next from song 2, which plays"
        );
        assert_eq!(rig.take_calls(), vec![plain(3), PlayerCall::Play]);

        // The end-of-track event arrives afterwards: already handled.
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::NotPending
        );
        assert!(rig.take_calls().is_empty());
        assert_eq!(rig.playing(), Some(3));
        assert_eq!(rig.fakes.player.snapshot().path, path(3));
    });
}

#[test]
fn a_manual_next_after_playback_stopped_at_the_end_moves_to_the_next_song_once() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        rig.fakes.player.finish_track(); // no preload: playback stops
        assert_eq!(
            next(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(2))
        );
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::NotPending
        );
        assert_eq!(rig.playing(), Some(2));
    });
}

#[test]
fn quirk_28_previous_during_a_pending_end_of_track_goes_back_from_the_promoted_track() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(2);
        maybe_preload(&rig.ctx, &rig.cursor, 170.0, 180.0);
        rig.fakes.player.finish_track(); // song 3 promoted
        rig.take_calls();
        assert_eq!(
            previous(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(2))
        );
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::NotPending
        );
        assert_eq!(rig.playing(), Some(2));
    });
}

#[test]
fn the_end_of_the_playlist_stops() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(5);
        rig.fakes.player.finish_track();
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::Stopped
        );
        assert!(rig.take_calls().is_empty());
        assert_eq!(rig.playing(), Some(5));
        assert!(!rig.fakes.player.is_playing());
    });
}

#[test]
fn repeat_all_wraps_at_the_end_of_the_playlist() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(5);
        toggle_repeat(&rig.ctx, &mut rig.cursor);
        rig.fakes.player.finish_track();
        assert_eq!(
            end_of_track(&rig.ctx, &mut rig.cursor).unwrap(),
            EndOfTrackOutcome::Advanced(rig.id(1))
        );
    });
}

#[test]
fn previous_loads_the_previous_song_and_persists_it() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(4);
        assert_eq!(
            previous(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(3))
        );
        assert_eq!(rig.take_calls(), vec![plain(3), PlayerCall::Play]);
        assert_eq!(
            rig.stored(rig.playlist_id("List")).last_song_id,
            Some(rig.id(3))
        );
    });
}

#[test]
fn quirk_22_previous_from_the_first_song_wraps_to_the_last_with_repeat_all() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        assert_eq!(
            previous(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Unchanged,
            "no wrap without Repeat All"
        );
        assert_eq!(toggle_repeat(&rig.ctx, &mut rig.cursor), RepeatMode::All);
        assert_eq!(
            previous(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(5))
        );
    });
}

#[test]
fn quirk_23_shuffle_previous_skips_a_song_that_left_the_playlist() {
    with_timeout(TEST_TIMEOUT, || {
        // Song 4 was played, then removed from the playlist.
        let mut rig = Rig::with(fixture_with(&[1, 2, 3, 5], &[]));
        rig.start(3);
        rig.cursor.shuffle = true;
        rig.cursor.session = Some(ShuffleSession {
            session_id: "s".to_string(),
            shuffle_order: vec![rig.id(1), rig.id(4), rig.id(3), rig.id(5)],
            current_position: 3,
            history: vec![rig.id(1), rig.id(4), rig.id(3)],
        });
        assert_eq!(
            previous(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(1))
        );
        let session = rig.cursor.shuffle_session().unwrap();
        assert_eq!(session.history, vec![rig.id(1)]);
        assert_eq!(session.current_position, 1);
        // The database round trip also puts the first pending song in the
        // history (see `enabling_shuffle_saves_a_session_and_disabling_it_clears_it`).
        let stored = rig.stored_session(rig.playlist_id("List")).unwrap();
        assert_eq!(
            stored.history.last(),
            Some(&rig.id(1)),
            "the session is saved"
        );
        assert!(!stored.history.contains(&rig.id(3)));
    });
}

// --- Preload ----------------------------------------------------------------------

#[test]
fn the_next_song_is_preloaded_once_within_fifteen_seconds_of_the_end() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        assert_eq!(maybe_preload(&rig.ctx, &rig.cursor, 100.0, 200.0), None);
        assert_eq!(maybe_preload(&rig.ctx, &rig.cursor, 184.9, 200.0), None);
        assert!(rig.take_calls().is_empty());

        maybe_preload(&rig.ctx, &rig.cursor, 185.0, 200.0);
        maybe_preload(&rig.ctx, &rig.cursor, 190.0, 200.0);
        assert_eq!(
            rig.take_calls(),
            vec![PlayerCall::Preload {
                path: path(2),
                gains: GAINS_2
            }],
            "once per track"
        );
        assert!(rig.ctx.preload.is_preloaded(&path(2)));

        // The track advances; the next one is preloaded for song 3.
        rig.fakes.player.finish_track();
        end_of_track(&rig.ctx, &mut rig.cursor).unwrap();
        rig.take_calls();
        let cover = maybe_preload(&rig.ctx, &rig.cursor, 190.0, 200.0);
        assert_eq!(
            rig.take_calls(),
            vec![PlayerCall::Preload {
                path: path(3),
                gains: ReplayGains::default()
            }]
        );
        assert!(cover.is_some(), "song 3's album has a cover");
    });
}

#[test]
fn nothing_is_preloaded_while_paused_with_an_unknown_duration_or_at_the_end() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(5);
        assert_eq!(maybe_preload(&rig.ctx, &rig.cursor, 195.0, 200.0), None);
        rig.start(1);
        assert_eq!(maybe_preload(&rig.ctx, &rig.cursor, 195.0, 0.0), None);
        pause(&rig.ctx, &rig.cursor);
        assert_eq!(maybe_preload(&rig.ctx, &rig.cursor, 195.0, 200.0), None);
        let calls = rig.take_calls();
        assert!(
            !calls
                .iter()
                .any(|c| matches!(c, PlayerCall::Preload { .. }))
        );
    });
}

#[test]
fn the_preloaded_song_is_the_one_next_plays_in_shuffle() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        toggle_shuffle(&rig.ctx, &mut rig.cursor);
        maybe_preload(&rig.ctx, &rig.cursor, 190.0, 200.0);
        let preloaded = rig.fakes.player.preloaded_path().unwrap();
        next(&rig.ctx, &mut rig.cursor).unwrap();
        assert_eq!(
            rig.cursor.playing_song().unwrap().file_path.as_ref(),
            preloaded
        );
    });
}

// --- Repeat and shuffle -----------------------------------------------------------

#[test]
fn toggle_repeat_cycles_off_all_one_and_persists_it() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        for (expected, stored) in [
            (RepeatMode::All, 1),
            (RepeatMode::One, 2),
            (RepeatMode::Off, 0),
        ] {
            assert_eq!(toggle_repeat(&rig.ctx, &mut rig.cursor), expected);
            assert_eq!(rig.stored(list).repeat_mode, stored);
        }
    });
}

#[test]
fn enabling_shuffle_saves_a_session_and_disabling_it_clears_it() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(2);
        let list = rig.playlist_id("List");
        assert!(toggle_shuffle(&rig.ctx, &mut rig.cursor));
        let session = rig.cursor.shuffle_session().unwrap().clone();
        assert_eq!(session.history, vec![rig.id(2)]);
        assert_eq!(session.shuffle_order.len(), 4);
        let stored = rig.stored_session(list).unwrap();
        assert_eq!(stored.session_id, session.session_id);
        // The session reads back exactly as saved (defect D26, fixed in the
        // database for both UIs: the round trip used to move the first pending
        // song into the history).
        assert_eq!(stored.shuffle_order, session.shuffle_order);
        assert_eq!(stored.history, session.history);
        let row = rig.stored(list);
        assert!(row.shuffle_active);
        assert_eq!(row.shuffle_id.as_deref(), Some(session.session_id.as_str()));

        assert!(!toggle_shuffle(&rig.ctx, &mut rig.cursor));
        assert!(rig.cursor.shuffle_session().is_none());
        assert!(rig.stored_session(list).is_none());
        assert!(!rig.stored(list).shuffle_active);
    });
}

#[test]
fn quirk_24_enabling_shuffle_with_no_enabled_song_drops_the_old_session() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::with(fixture_with(&[1, 2, 3, 4, 5], &[1, 2, 3, 4, 5]));
        let list = rig.playlist_id("List");
        let stale = ShuffleSession {
            session_id: "stale".to_string(),
            shuffle_order: vec![rig.id(1), rig.id(2)],
            current_position: 1,
            history: vec![rig.id(1)],
        };
        playlist::save_shuffle_session(&rig.ctx, list, &stale)
            .recv_blocking()
            .unwrap();
        rig.cursor.session = Some(stale);

        assert!(!toggle_shuffle(&rig.ctx, &mut rig.cursor));
        assert!(rig.cursor.shuffle_session().is_none());
        assert!(rig.stored_session(list).is_none(), "not saved again");
        assert_eq!(rig.stored(list).shuffle_id, None);
    });
}

// --- System media controls --------------------------------------------------------

#[test]
fn each_external_control_maps_to_its_command() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.fakes.player.set_track_duration(&path(1), 200.0);
        rig.start(1);
        rig.fakes.player.advance(30.0);
        let mut run = |event: ExternalControlEvent| {
            let action = external_control(&rig.ctx, &mut rig.cursor, event).unwrap();
            flush(&rig.ctx);
            (action, rig.fakes.player.take_calls())
        };

        let (action, calls) = run(ExternalControlEvent::Pause);
        assert_eq!(action, TransportAction::Pause);
        assert_eq!(
            calls,
            vec![PlayerCall::SetPlaying(false), PlayerCall::PurgeBuffers]
        );
        let (_, calls) = run(ExternalControlEvent::Toggle);
        assert_eq!(
            calls,
            vec![PlayerCall::SetPlaying(true)],
            "toggle from paused"
        );
        let (_, calls) = run(ExternalControlEvent::Play);
        assert_eq!(calls, vec![PlayerCall::SetPlaying(true)]);
        let (_, calls) = run(ExternalControlEvent::SeekForward);
        assert_eq!(calls, vec![PlayerCall::Seek(40.0)]);
        let (_, calls) = run(ExternalControlEvent::SeekBackward);
        assert_eq!(calls, vec![PlayerCall::Seek(30.0)]);
        let (_, calls) = run(ExternalControlEvent::SeekRelative(-5.5));
        assert_eq!(calls, vec![PlayerCall::Seek(24.5)]);
        let (_, calls) = run(ExternalControlEvent::SeekTo(250.0));
        assert_eq!(calls, vec![PlayerCall::Seek(200.0)], "clamped to the track");
        let (_, calls) = run(ExternalControlEvent::SetVolume(1.5));
        assert_eq!(calls, vec![PlayerCall::SetVolume(1.0)]);
        let (action, calls) = run(ExternalControlEvent::Next);
        assert_eq!(action, TransportAction::Next);
        assert_eq!(calls, vec![load(2, GAINS_2), PlayerCall::Play]);
        let (_, calls) = run(ExternalControlEvent::Previous);
        assert_eq!(calls, vec![plain(1), PlayerCall::Play]);
        let (_, calls) = run(ExternalControlEvent::Stop);
        assert_eq!(calls, vec![PlayerCall::Stop, PlayerCall::PurgeBuffers]);

        let echoed: Vec<MediaCall> = rig
            .fakes
            .media
            .calls()
            .into_iter()
            .filter(|call| matches!(call, MediaCall::SetVolume(_)))
            .collect();
        assert_eq!(
            echoed,
            vec![MediaCall::SetVolume(1.0)],
            "the volume is echoed"
        );
        assert_eq!(stored_volume(&rig).as_deref(), Some("1.0000"));
    });
}

#[test]
fn an_external_seek_is_ignored_while_the_duration_is_unknown() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.fakes.player.set_track_duration(&path(1), 0.0);
        rig.start(1);
        external_control(
            &rig.ctx,
            &mut rig.cursor,
            ExternalControlEvent::SeekTo(20.0),
        )
        .unwrap();
        assert!(rig.take_calls().is_empty());
    });
}

// --- Saved position after a track change --------------------------------------------

/// A player whose loads take effect only when the test applies them, like
/// the engine, whose decoder thread applies a load after `load_file` returns:
/// until then the state still shows the previous track and its position.
struct LaggingPlayer {
    inner: Arc<crate::frontend::fakes::FakePlayer>,
    pending: parking_lot::Mutex<Option<(TrackRequest, ReplayGains)>>,
}

impl LaggingPlayer {
    fn apply_pending_load(&self) {
        if let Some((track, gains)) = self.pending.lock().take() {
            self.inner.load_file(&track, gains).unwrap();
        }
    }
}

use crate::audio::AudioError;
use crate::audio::dsp::DspChain;
use crate::audio::engine::{AudioDeviceInfo, AudioSettings, AudioState};
use crate::audio::meter::MeterData;
use crate::audio::preset::EqPreset;
use std::sync::Arc;

impl PlayerPort for LaggingPlayer {
    fn play(&self) {
        self.inner.play();
    }
    fn pause(&self) {
        self.inner.pause();
    }
    fn set_playing(&self, playing: bool) {
        self.inner.set_playing(playing);
    }
    fn stop(&self) {
        self.inner.stop();
    }
    fn toggle_play_pause(&self) {
        self.inner.toggle_play_pause();
    }
    fn is_playing(&self) -> bool {
        self.inner.is_playing()
    }
    fn seek(&self, position_secs: f64) {
        self.inner.seek(position_secs);
    }
    fn take_eof(&self) -> bool {
        self.inner.take_eof()
    }
    fn load_file(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
        *self.pending.lock() = Some((track.clone(), gains));
        Ok(())
    }
    fn preload_next(&self, track: &TrackRequest, gains: ReplayGains) -> Result<(), AudioError> {
        self.inner.preload_next(track, gains)
    }
    fn purge_preload(&self) -> Result<(), AudioError> {
        self.inner.purge_preload()
    }
    fn crossfade_next(&self, ms: f64) -> Result<(), AudioError> {
        self.inner.crossfade_next(ms)
    }
    fn set_volume(&self, volume: f32) {
        self.inner.set_volume(volume);
    }
    fn preamp_gain(&self) -> f32 {
        self.inner.preamp_gain()
    }
    fn set_preamp_gain(&self, db: f32) {
        self.inner.set_preamp_gain(db);
    }
    fn eq_enabled(&self) -> bool {
        self.inner.eq_enabled()
    }
    fn set_eq_enabled(&self, enabled: bool) {
        self.inner.set_eq_enabled(enabled);
    }
    fn eq_band_count(&self) -> usize {
        self.inner.eq_band_count()
    }
    fn set_eq_mode(&self, num_bands: usize) {
        self.inner.set_eq_mode(num_bands);
    }
    fn eq_band(&self, index: usize) -> Option<(f32, f32)> {
        self.inner.eq_band(index)
    }
    fn set_eq_band_gain(&self, index: usize, db: f32) {
        self.inner.set_eq_band_gain(index, db);
    }
    fn apply_eq_preset(&self, preset: &EqPreset) {
        self.inner.apply_eq_preset(preset);
    }
    fn apply_eq_preset_gains(&self, bands_20: &[f32], bands_31: &[f32]) {
        self.inner.apply_eq_preset_gains(bands_20, bands_31);
    }
    fn reset_dsp_defaults(&self) {
        self.inner.reset_dsp_defaults();
    }
    fn update_dsp(&self, f: &mut dyn FnMut(&mut DspChain)) {
        self.inner.update_dsp(f);
    }
    fn read_dsp(&self, f: &mut dyn FnMut(&DspChain)) {
        self.inner.read_dsp(f);
    }
    fn state(&self) -> Arc<parking_lot::RwLock<AudioState>> {
        self.inner.state()
    }
    fn snapshot(&self) -> AudioState {
        self.inner.snapshot()
    }
    fn meter(&self) -> Arc<MeterData> {
        self.inner.meter()
    }
    fn buffer_size(&self) -> u32 {
        self.inner.buffer_size()
    }
    fn available_hosts(&self) -> Vec<String> {
        self.inner.available_hosts()
    }
    fn devices(&self) -> Vec<AudioDeviceInfo> {
        self.inner.devices()
    }
    fn apply_audio_settings(&self, settings: AudioSettings) -> Result<(), AudioError> {
        self.inner.apply_audio_settings(settings)
    }
    fn purge_buffers(&self) -> Result<(), AudioError> {
        self.inner.purge_buffers()
    }
}

#[test]
fn quirk_29_a_track_change_saves_position_zero_before_the_engine_applies_the_load() {
    use crate::db::Database;
    use crate::db::scanner::Scanner;
    use crate::frontend::app_context::Parts;
    use crate::frontend::db_worker::SharedDatabase;
    use crate::frontend::fakes::{FakeMediaControls, FakeMemoryProbe, FakePlayer, FakeSystemAudio};
    use crate::integrations::control::external_control_channel;
    use crate::utils::config::AppConfig;

    with_timeout(TEST_TIMEOUT, || {
        let database: SharedDatabase =
            Arc::new(std::sync::Mutex::new(Database::new_memory().unwrap()));
        let player = Arc::new(LaggingPlayer {
            inner: Arc::new(FakePlayer::new()),
            pending: parking_lot::Mutex::new(None),
        });
        let (_external_tx, external_rx) = external_control_channel();
        let ctx = AppContext::assemble(Parts {
            scanner: Arc::new(Scanner::new(Arc::clone(&database))),
            database,
            player: player.clone(),
            media: Arc::new(FakeMediaControls::new()),
            system_audio: Arc::new(FakeSystemAudio::new()),
            external: external_rx,
            probe: Arc::new(FakeMemoryProbe::new()),
            config: AppConfig::default(),
        })
        .unwrap();
        let seeded = fixture().seed_on(&ctx.db).recv_blocking().unwrap();
        let list = seeded.playlist("List").unwrap().id;
        let mut cursor = PlaybackCursor::new(load_playlist(&ctx, list).recv_blocking().unwrap());
        let one = seeded.song_id(&path(1)).unwrap();
        play_song(&ctx, &mut cursor, one).unwrap();
        player.apply_pending_load();
        player.inner.advance(80.0);

        next(&ctx, &mut cursor).unwrap();
        assert_eq!(
            player.snapshot().current_pos_sec,
            80.0,
            "the load is pending"
        );
        flush(&ctx);
        let stored = ctx
            .db
            .read(move |db| Ok(db.get_playlist_by_id(list)?))
            .recv_blocking()
            .unwrap()
            .unwrap();
        assert_eq!(stored.last_song_id, seeded.song_id(&path(2)));
        assert_eq!(
            stored.last_pos_sec, 0.0,
            "not the previous track's 80 s, which Iced would save"
        );
    });
}
