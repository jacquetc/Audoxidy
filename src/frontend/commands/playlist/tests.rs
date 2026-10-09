use super::*;
use crate::frontend::commands::playback::tests::{
    GAINS_2, Rig, fixture, fixture_with, flush, library, path, plain,
};
use crate::frontend::commands::playback::{self, PlaybackCursor};
use crate::frontend::events::{LongOpEvent, Origin, SubscriptionToken};
use crate::frontend::fakes::PlayerCall;
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::keys;
use crate::frontend::test_fixtures::{FixtureSong, LibraryFixture};
use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Duration;

/// Collects the `Playlists` events.
fn playlist_events(ctx: &AppContext) -> (SubscriptionToken, mpsc::Receiver<PlaylistsEvent>) {
    let (tx, rx) = mpsc::channel();
    let tx = parking_lot::Mutex::new(tx);
    let token = ctx.hub.subscribe(
        Origin::Playlists,
        Arc::new(move |event| {
            if let Event::Playlists(event) = event {
                let _ = tx.lock().send(event);
            }
        }),
    );
    (token, rx)
}

fn next_event(rx: &mpsc::Receiver<PlaylistsEvent>) -> PlaylistsEvent {
    rx.recv_timeout(TEST_TIMEOUT).unwrap()
}

/// The song ids of a playlist, in order, with their enabled flags.
fn stored_songs(ctx: &AppContext, playlist_id: i64) -> Vec<(i64, bool)> {
    flush(ctx);
    ctx.db
        .read(move |db| Ok(db.get_playlist_songs(playlist_id)?))
        .recv_blocking()
        .unwrap()
        .into_iter()
        .map(|song| (song.song_id, song.enabled))
        .collect()
}

fn ids(songs: &[(i64, bool)]) -> Vec<i64> {
    songs.iter().map(|(id, _)| *id).collect()
}

fn playlist_named(ctx: &AppContext, name: &str) -> Option<PlaylistData> {
    flush(ctx);
    list_playlists(ctx)
        .recv_blocking()
        .unwrap()
        .into_iter()
        .find(|playlist| playlist.name == name)
}

/// Stores a playback state for a playlist.
fn save_state(ctx: &AppContext, playlist_id: i64, song_id: Option<i64>, pos: f64, playing: bool) {
    ctx.db
        .write(move |db| {
            Ok(db.update_playlist_persistence(
                playlist_id,
                song_id,
                pos,
                playing,
                false,
                2,
                0,
                None,
            )?)
        })
        .recv_blocking()
        .unwrap();
}

/// Waits for a long operation's result.
fn wait_op<T: Send + 'static>(ctx: &AppContext, op: OpId) -> Result<T, FrontendError> {
    let deadline = std::time::Instant::now() + TEST_TIMEOUT;
    loop {
        if let Some(result) = ctx.ops.take_result::<T>(op) {
            return result;
        }
        assert!(std::time::Instant::now() < deadline, "the operation hangs");
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// A fresh, empty directory for one test.
fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("audoxidy-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// --- Listing and loading -----------------------------------------------------------

#[test]
fn listing_returns_every_playlist_in_tab_order() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let names: Vec<String> = list_playlists(&rig.ctx)
            .recv_blocking()
            .unwrap()
            .into_iter()
            .map(|playlist| playlist.name)
            .collect();
        assert_eq!(names, ["Audoxidy", "Oxidy Drift", "List", "Other"]);
    });
}

#[test]
fn loading_a_playlist_reads_its_rows_gains_and_saved_modes() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let list = rig.playlist_id("List");
        save_state(&rig.ctx, list, Some(rig.id(3)), 12.0, true);
        let loaded = load_playlist(&rig.ctx, list).recv_blocking().unwrap();
        assert_eq!(loaded.data.name, "List");
        assert_eq!(loaded.data.last_song_id, Some(rig.id(3)));
        assert_eq!(loaded.groups.len(), 2);
        assert_eq!(loaded.groups[1].songs.len(), 3);
        assert_eq!(loaded.gains.get(&rig.id(2)), Some(&GAINS_2));
        assert_eq!(loaded.gains.len(), 1, "songs without gains are left out");
        assert!(loaded.session.is_none(), "shuffle is off");
        let cursor = PlaybackCursor::new(loaded);
        assert_eq!(cursor.repeat(), RepeatMode::One);
    });
}

#[test]
fn a_loaded_shuffle_session_points_just_after_the_saved_song() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let list = rig.playlist_id("List");
        let order = vec![rig.id(3), rig.id(5), rig.id(1), rig.id(2), rig.id(4)];
        let session = ShuffleSession {
            session_id: "s1".to_string(),
            shuffle_order: order,
            current_position: 0,
            history: Vec::new(),
        };
        let last = rig.id(1);
        rig.ctx
            .db
            .write(move |db| {
                db.save_shuffle_session(list, &session)?;
                Ok(db.update_playlist_persistence(
                    list,
                    Some(last),
                    5.0,
                    false,
                    true,
                    1,
                    0,
                    Some("s1".into()),
                )?)
            })
            .recv_blocking()
            .unwrap();

        let loaded = load_playlist(&rig.ctx, list).recv_blocking().unwrap();
        let session = loaded.session.clone().unwrap();
        let at = session
            .shuffle_order
            .iter()
            .position(|&id| id == last)
            .unwrap();
        assert_eq!(session.current_position, at + 1, "just after the last song");
        assert!(session.history.contains(&last));
        let cursor = PlaybackCursor::new(loaded);
        assert!(cursor.shuffle_active());
        assert_eq!(cursor.repeat(), RepeatMode::All);
    });
}

#[test]
fn loading_an_unknown_playlist_is_refused() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        assert!(matches!(
            load_playlist(&rig.ctx, 9_999).recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
    });
}

// --- Switching ----------------------------------------------------------------------

#[test]
fn switching_persists_the_old_playlist_then_restores_the_saved_song_position_and_play_state() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let (list, other) = (rig.playlist_id("List"), rig.playlist_id("Other"));
        save_state(&rig.ctx, other, Some(rig.id(4)), 33.0, false);
        rig.start(1);
        rig.fakes.player.advance(12.0);
        rig.fakes.player.state().write().eof_reached = true;

        let switch = switch_playlist(&rig.ctx, &rig.cursor, other, false)
            .recv_blocking()
            .unwrap();
        assert_eq!(switch.playlist_id(), other);
        assert_eq!(
            switch.apply(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(4))
        );
        assert_eq!(
            rig.take_calls(),
            vec![
                plain(4),
                PlayerCall::Seek(33.0),
                PlayerCall::SetPlaying(false)
            ]
        );
        assert!(
            !rig.fakes.player.snapshot().eof_reached,
            "taken before the load"
        );
        assert_eq!(rig.cursor.playlist_id(), other);
        assert_eq!(rig.playing(), Some(4));
        assert_eq!(rig.cursor.repeat(), RepeatMode::One);

        let old = rig.stored(list);
        assert_eq!(old.last_song_id, Some(rig.id(1)));
        assert_eq!(old.last_pos_sec, 12.0);
        let active = rig
            .ctx
            .db
            .read(|db| Ok(db.get_setting(keys::LAST_ACTIVE_PLAYLIST_ID)))
            .recv_blocking()
            .unwrap();
        assert_eq!(active, Some(other.to_string()));
    });
}

#[test]
fn switching_with_auto_play_plays_the_first_enabled_song_from_the_start() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let other = rig.playlist_id("Other");
        save_state(&rig.ctx, other, Some(rig.id(4)), 33.0, false);
        let four = rig.id(4);
        rig.ctx
            .db
            .write(move |db| Ok(db.set_song_enabled_in_playlist(other, four, false)?))
            .recv_blocking()
            .unwrap();

        let switch = switch_playlist(&rig.ctx, &rig.cursor, other, true)
            .recv_blocking()
            .unwrap();
        assert_eq!(
            switch.apply(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Loaded(rig.id(1))
        );
        assert_eq!(
            rig.take_calls(),
            vec![
                plain(1),
                PlayerCall::Seek(0.0),
                PlayerCall::SetPlaying(true)
            ]
        );
    });
}

#[test]
fn switching_to_a_playlist_without_a_saved_song_leaves_the_player_alone() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(2);
        let other = rig.playlist_id("Other");
        let switch = switch_playlist(&rig.ctx, &rig.cursor, other, false)
            .recv_blocking()
            .unwrap();
        assert_eq!(
            switch.apply(&rig.ctx, &mut rig.cursor).unwrap(),
            TrackChange::Unchanged
        );
        assert!(rig.take_calls().is_empty());
        assert_eq!(rig.cursor.playlist_id(), other);
        assert_eq!(rig.cursor.playing_song_id(), None);
        assert_eq!(rig.fakes.player.snapshot().path, path(2), "still playing");
    });
}

#[test]
fn switching_keeps_the_search_and_applies_it_to_the_new_rows() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.cursor.set_search("delta");
        let other = rig.playlist_id("Other");
        let switch = switch_playlist(&rig.ctx, &rig.cursor, other, false)
            .recv_blocking()
            .unwrap();
        switch.apply(&rig.ctx, &mut rig.cursor).unwrap();
        assert_eq!(rig.cursor.search(), "delta");
        assert_eq!(nav::count_linear_items(rig.cursor.active_groups()), 2);
        assert_eq!(nav::count_linear_items(rig.cursor.groups()), 4);
    });
}

// --- Content ------------------------------------------------------------------------

#[test]
fn quirk_12_reloading_the_content_refreshes_the_rows_without_touching_the_player() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::with(fixture_with(&[1, 2, 3], &[]));
        let (_token, events) = playlist_events(&rig.ctx);
        let list = rig.playlist_id("List");
        rig.start(1);
        rig.fakes.player.advance(50.0);

        let added = add_songs(&rig.ctx, list, vec![rig.id(4), rig.id(5)])
            .recv_blocking()
            .unwrap();
        assert_eq!(added, 2);
        assert_eq!(
            next_event(&events),
            PlaylistsEvent::ContentChanged { playlist_id: list }
        );

        let content = reload_content(&rig.ctx, &rig.cursor)
            .recv_blocking()
            .unwrap();
        assert_eq!(content.playlist_id(), list);
        assert!(content.apply(&mut rig.cursor));
        assert_eq!(nav::flatten_groups(rig.cursor.groups()).len(), 5);
        assert!(rig.take_calls().is_empty(), "no load, seek or play state");
        assert_eq!(rig.fakes.player.snapshot().current_pos_sec, 50.0);
        assert_eq!(rig.playing(), Some(1));
    });
}

#[test]
fn content_reloaded_for_another_playlist_is_not_applied() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let content = reload_content(&rig.ctx, &rig.cursor)
            .recv_blocking()
            .unwrap();
        let other = rig.playlist_id("Other");
        let switch = switch_playlist(&rig.ctx, &rig.cursor, other, false)
            .recv_blocking()
            .unwrap();
        switch.apply(&rig.ctx, &mut rig.cursor).unwrap();
        assert!(!content.apply(&mut rig.cursor));
        assert_eq!(nav::flatten_groups(rig.cursor.groups()).len(), 2);
    });
}

// --- Creating, renaming, deleting --------------------------------------------------

#[test]
fn creating_a_playlist_with_pending_songs_publishes_list_changed() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let (_token, events) = playlist_events(&rig.ctx);
        let id = create_playlist(&rig.ctx, "Mix", vec![rig.id(3), rig.id(1)])
            .recv_blocking()
            .unwrap();
        assert_eq!(next_event(&events), PlaylistsEvent::ListChanged);
        assert_eq!(ids(&stored_songs(&rig.ctx, id)), [rig.id(3), rig.id(1)]);
        assert!(!playlist_named(&rig.ctx, "Mix").unwrap().is_system);

        let empty = create_playlist(&rig.ctx, "Empty", Vec::new())
            .recv_blocking()
            .unwrap();
        assert!(stored_songs(&rig.ctx, empty).is_empty());
    });
}

#[test]
fn a_blank_or_taken_name_is_refused() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        assert!(matches!(
            create_playlist(&rig.ctx, "  ", Vec::new()).recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
        assert!(matches!(
            rename_playlist(&rig.ctx, rig.playlist_id("Other"), "").recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
        assert!(matches!(
            create_playlist(&rig.ctx, "List", Vec::new()).recv_blocking(),
            Err(FrontendError::Database(_))
        ));
    });
}

#[test]
fn renaming_a_user_playlist_renames_it() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let (_token, events) = playlist_events(&rig.ctx);
        let other = rig.playlist_id("Other");
        assert_eq!(
            rename_playlist(&rig.ctx, other, "Road")
                .recv_blocking()
                .unwrap(),
            RenameOutcome::Renamed(other)
        );
        assert_eq!(next_event(&events), PlaylistsEvent::ListChanged);
        assert_eq!(playlist_named(&rig.ctx, "Road").unwrap().id, other);
        assert!(playlist_named(&rig.ctx, "Other").is_none());
    });
}

#[test]
fn renaming_a_system_playlist_creates_a_copy_with_all_its_songs() {
    with_timeout(TEST_TIMEOUT, || {
        let (two, five) = (path(2), path(5));
        let rig = Rig::with(fixture().playlist_with_disabled(
            "Audoxidy",
            &[two.as_str(), five.as_str()],
            &[five.as_str()],
        ));
        let main = rig.seeded.main_playlist_id;
        let outcome = rename_playlist(&rig.ctx, main, "  Copia ")
            .recv_blocking()
            .unwrap();
        let RenameOutcome::Copied(copy) = outcome else {
            panic!("expected a copy, got {outcome:?}");
        };
        let copy_row = playlist_named(&rig.ctx, "Copia").unwrap();
        assert_eq!(copy_row.id, copy);
        assert!(!copy_row.is_system);
        assert_eq!(
            stored_songs(&rig.ctx, copy),
            [(rig.id(2), true), (rig.id(5), true)],
            "every song, enabled, as Iced copies them"
        );
        assert!(playlist_named(&rig.ctx, "Audoxidy").unwrap().is_system);
    });
}

#[test]
fn quirk_16_deleting_a_system_playlist_is_refused() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        for id in [rig.seeded.main_playlist_id, rig.seeded.external_playlist_id] {
            assert!(matches!(
                delete_playlist(&rig.ctx, id).recv_blocking(),
                Err(FrontendError::InvalidInput(message)) if message.contains("system playlist")
            ));
        }
        assert!(playlist_named(&rig.ctx, "Audoxidy").is_some());
        assert!(matches!(
            delete_playlist(&rig.ctx, 9_999).recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
    });
}

#[test]
fn deleting_a_user_playlist_removes_it() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let (_token, events) = playlist_events(&rig.ctx);
        delete_playlist(&rig.ctx, rig.playlist_id("Other"))
            .recv_blocking()
            .unwrap();
        assert_eq!(next_event(&events), PlaylistsEvent::ListChanged);
        assert!(playlist_named(&rig.ctx, "Other").is_none());
    });
}

// --- Songs under a search ------------------------------------------------------------

#[test]
fn sending_under_a_search_sends_the_visible_songs() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let (_token, events) = playlist_events(&rig.ctx);
        let mix = create_playlist(&rig.ctx, "Mix", Vec::new())
            .recv_blocking()
            .unwrap();
        assert_eq!(next_event(&events), PlaylistsEvent::ListChanged);
        rig.cursor.set_search("delta"); // 0 the /m/y separator, 1 Delta

        let sent = send_to(&rig.ctx, &rig.cursor, &HashSet::new(), 1, mix)
            .recv_blocking()
            .unwrap();
        assert_eq!(sent, 1);
        assert_eq!(
            next_event(&events),
            PlaylistsEvent::ContentChanged { playlist_id: mix }
        );
        assert_eq!(ids(&stored_songs(&rig.ctx, mix)), [rig.id(4)]);

        // A selected separator sends only its displayed songs.
        let other = rig.playlist_id("Other");
        let mut before = ids(&stored_songs(&rig.ctx, other));
        send_to(&rig.ctx, &rig.cursor, &HashSet::from([0]), 0, other)
            .recv_blocking()
            .unwrap();
        before.sort_unstable();
        let mut after = ids(&stored_songs(&rig.ctx, other));
        after.sort_unstable();
        assert_eq!(
            after, before,
            "Delta was already in Other; Charlie and Echo are hidden"
        );
        assert_eq!(
            send_to(&rig.ctx, &rig.cursor, &HashSet::new(), 99, other)
                .recv_blocking()
                .unwrap(),
            0
        );
    });
}

#[test]
fn removing_under_a_search_removes_only_the_visible_songs_and_purges() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let (_token, events) = playlist_events(&rig.ctx);
        let list = rig.playlist_id("List");
        // "l": Alpha, Charlie and Delta. Rows: 0 /m/x, 1 Alpha, 2 /m/y,
        // 3 Charlie, 4 Delta; Echo is hidden.
        rig.cursor.set_search("l");
        assert_eq!(nav::count_linear_items(rig.cursor.active_groups()), 5);

        let removed = remove_songs(&rig.ctx, &rig.cursor, &HashSet::from([2]), 2)
            .recv_blocking()
            .unwrap();
        assert_eq!(removed, 2);
        assert_eq!(
            next_event(&events),
            PlaylistsEvent::ContentChanged { playlist_id: list }
        );
        assert_eq!(
            ids(&stored_songs(&rig.ctx, list)),
            [rig.id(1), rig.id(2), rig.id(5)]
        );
        assert!(
            rig.take_calls().contains(&PlayerCall::PurgePreload),
            "the memory purge runs, as in Iced"
        );

        reload_content(&rig.ctx, &rig.cursor)
            .recv_blocking()
            .unwrap()
            .apply(&mut rig.cursor);
        assert_eq!(
            nav::count_linear_items(rig.cursor.active_groups()),
            2,
            "Alpha"
        );
    });
}

// --- Enabled flags -------------------------------------------------------------------

#[test]
fn toggling_under_a_search_updates_both_lists_and_the_database() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        rig.cursor.set_search("l"); // 0 /m/x, 1 Alpha, 2 /m/y, 3 Charlie, 4 Delta

        assert_eq!(
            toggle_song_enabled(&rig.ctx, &mut rig.cursor, 3),
            Some(false)
        );
        assert!(!rig.cursor.song_at(3).unwrap().enabled);
        assert!(!rig.cursor.song(rig.id(3)).unwrap().enabled, "mirrored");
        assert!(stored_songs(&rig.ctx, list).contains(&(rig.id(3), false)));
        assert_eq!(
            toggle_song_enabled(&rig.ctx, &mut rig.cursor, 2),
            None,
            "a separator"
        );

        // Delta is still enabled, so the group is disabled, Echo included.
        assert_eq!(
            toggle_group_enabled(&rig.ctx, &mut rig.cursor, 2),
            Some(false)
        );
        assert!(
            nav::flatten_groups(&rig.cursor.groups()[1..])
                .iter()
                .all(|song| !song.enabled)
        );
        let stored = stored_songs(&rig.ctx, list);
        for n in 3..=5 {
            assert!(stored.contains(&(rig.id(n), false)), "song {n}");
        }
        assert_eq!(
            toggle_group_enabled(&rig.ctx, &mut rig.cursor, 2),
            Some(true)
        );
        assert!(stored_songs(&rig.ctx, list).contains(&(rig.id(5), true)));
        assert_eq!(
            toggle_group_enabled(&rig.ctx, &mut rig.cursor, 1),
            None,
            "a song"
        );

        set_song_enabled(&rig.ctx, &mut rig.cursor, 1, false)
            .recv_blocking()
            .unwrap();
        set_group_enabled(&rig.ctx, &mut rig.cursor, 0, true)
            .recv_blocking()
            .unwrap();
        assert!(stored_songs(&rig.ctx, list).contains(&(rig.id(1), true)));
    });
}

#[test]
fn the_bulk_toggle_enables_everything_when_anything_is_disabled() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::with(fixture_with(&[1, 2, 3, 4, 5], &[2]));
        let (_token, events) = playlist_events(&rig.ctx);
        let list = rig.playlist_id("List");
        let selection = HashSet::from([1, 2, 3]); // Alpha, Bravo, the /m/y separator

        assert!(toggle_enabled(&rig.ctx, &mut rig.cursor, &selection, 1));
        assert_eq!(
            next_event(&events),
            PlaylistsEvent::ContentChanged { playlist_id: list }
        );
        assert!(
            stored_songs(&rig.ctx, list)
                .iter()
                .all(|(_, enabled)| *enabled)
        );

        assert!(!toggle_enabled(&rig.ctx, &mut rig.cursor, &selection, 2));
        assert!(
            stored_songs(&rig.ctx, list)
                .iter()
                .all(|(_, enabled)| !enabled)
        );

        // A click outside the selection toggles that row alone.
        assert!(toggle_enabled(&rig.ctx, &mut rig.cursor, &selection, 6));
        let enabled: Vec<i64> = stored_songs(&rig.ctx, list)
            .into_iter()
            .filter(|(_, enabled)| *enabled)
            .map(|(id, _)| id)
            .collect();
        assert_eq!(enabled, [rig.id(5)]);
    });
}

#[test]
fn clearing_the_playlist_empties_it_and_stops_playback() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        rig.start(2);
        clear_playlist(&rig.ctx, &mut rig.cursor)
            .recv_blocking()
            .unwrap();
        assert!(rig.cursor.groups().is_empty());
        assert_eq!(rig.cursor.playing_song_id(), None);
        assert_eq!(rig.take_calls()[0], PlayerCall::Stop);
        assert!(stored_songs(&rig.ctx, list).is_empty());
        assert_eq!(rig.stored(list).last_song_id, None);
    });
}

// --- Persistence ----------------------------------------------------------------------

#[test]
fn quirk_13_persisting_stores_the_player_position_state_and_modes() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        rig.start(2);
        playback::toggle_repeat(&rig.ctx, &mut rig.cursor);
        rig.fakes.player.advance(42.0);
        persist_state(&rig.ctx, &rig.cursor)
            .recv_blocking()
            .unwrap();
        let stored = rig.stored(list);
        assert_eq!(stored.last_song_id, Some(rig.id(2)));
        assert_eq!(stored.last_pos_sec, 42.0);
        assert!(stored.is_playing);
        assert_eq!(stored.repeat_mode, 1);
        assert!(!stored.shuffle_active);
    });
}

#[test]
fn quick_successive_saves_keep_the_last_state() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        rig.start(1);
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel();
        let blocker = rig.ctx.db.write(move |_| {
            let _ = started_tx.send(());
            let _ = release_rx.recv_timeout(TEST_TIMEOUT);
            Ok(())
        });
        started_rx.recv_timeout(TEST_TIMEOUT).unwrap();

        rig.fakes.player.advance(10.0);
        let first = persist_state(&rig.ctx, &rig.cursor);
        rig.fakes.player.advance(10.0);
        let second = persist_state(&rig.ctx, &rig.cursor);
        release_tx.send(()).unwrap();
        blocker.recv_blocking().unwrap();

        first.recv_blocking().unwrap();
        second.recv_blocking().unwrap();
        assert_eq!(rig.stored(list).last_pos_sec, 20.0);
    });
}

// --- Export ---------------------------------------------------------------------------

/// Songs with accented and plain titles in the playlist "Export".
fn export_fixture(extra: Option<FixtureSong>) -> LibraryFixture {
    let mut songs = vec![
        FixtureSong {
            duration_secs: 252.6,
            ..FixtureSong::new("/m/e/01.flac", "Café", "Édith", "Été")
        },
        FixtureSong {
            duration_secs: 301.4,
            ..FixtureSong::new("/m/e/02.flac", "Ñandú", "Édith", "Été")
        },
        FixtureSong {
            duration_secs: 59.5,
            ..FixtureSong::new("/m/e/03.flac", "Plain", "Édith", "Été")
        },
    ];
    songs.extend(extra);
    let paths: Vec<String> = songs.iter().map(|song| song.path.clone()).collect();
    let paths: Vec<&str> = paths.iter().map(String::as_str).collect();
    let mut fixture = library();
    for song in songs {
        fixture = fixture.song(song);
    }
    fixture.playlist("Export", &paths).playlist("List", &[])
}

fn kovac() -> FixtureSong {
    FixtureSong {
        duration_secs: 210.0,
        ..FixtureSong::new("/m/e/04.flac", "Mira Kovač", "Mira Kovač", "Sol")
    }
}

#[test]
fn quirk_25_an_m3u_export_is_latin_1_with_rounded_durations() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::with(export_fixture(None));
        let dir = temp_dir("export-m3u");
        let target = dir.join("list.m3u");
        let op = export(
            &rig.ctx,
            rig.playlist_id("Export"),
            target.clone(),
            ExportFormat::M3U,
            ExportMode::SingleFile,
        );
        let report = wait_op::<ExportReport>(&rig.ctx, op).unwrap();
        assert_eq!(report.playlist_path, target);
        assert_eq!(report.entries, 3);
        assert!(report.failed_copies.is_empty());
        assert_eq!(
            std::fs::read(&target).unwrap(),
            b"#EXTM3U\n\
              #EXTINF:253,Caf\xe9\n/m/e/01.flac\n\
              #EXTINF:301,\xd1and\xfa\n/m/e/02.flac\n\
              #EXTINF:60,Plain\n/m/e/03.flac\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn quirk_25_an_entry_outside_latin_1_fails_the_m3u_export_without_writing() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::with(export_fixture(Some(kovac())));
        let (tx, rx) = mpsc::channel();
        let tx = parking_lot::Mutex::new(tx);
        let _token = rig.ctx.hub.subscribe(
            Origin::LongOperation,
            Arc::new(move |event| {
                if let Event::LongOperation(LongOpEvent::Failed(info)) = event {
                    let _ = tx.lock().send(info);
                }
            }),
        );
        let dir = temp_dir("export-latin1");
        let target = dir.join("list.m3u");
        let op = export(
            &rig.ctx,
            rig.playlist_id("Export"),
            target.clone(),
            ExportFormat::M3U,
            ExportMode::SingleFile,
        );
        let failed = rx.recv_timeout(TEST_TIMEOUT).unwrap();
        assert_eq!(failed.op_id, op);
        assert!(
            !failed.cancelled,
            "quirk fix 8: the UI shows an error toast"
        );
        match wait_op::<ExportReport>(&rig.ctx, op) {
            Err(FrontendError::InvalidInput(message)) => {
                assert!(
                    message.contains("\"Mira Kovač\" (/m/e/04.flac)"),
                    "{message}"
                );
                assert!(message.contains("'č'"), "{message}");
                assert!(message.contains("M3U8"), "{message}");
            }
            other => panic!("expected an invalid input, got {other:?}"),
        }
        assert!(!target.exists(), "no lossy file is written");
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn quirk_25_an_m3u8_export_is_utf_8() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::with(export_fixture(Some(kovac())));
        let dir = temp_dir("export-m3u8");
        let target = dir.join("list.m3u8");
        let op = export(
            &rig.ctx,
            rig.playlist_id("Export"),
            target.clone(),
            ExportFormat::M3U8,
            ExportMode::SingleFile,
        );
        wait_op::<ExportReport>(&rig.ctx, op).unwrap();
        let text = String::from_utf8(std::fs::read(&target).unwrap()).unwrap();
        assert!(text.starts_with("#EXTM3U\n#EXTINF:253,Café\n"));
        assert!(text.ends_with("#EXTINF:210,Mira Kovač\n/m/e/04.flac\n"));
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn a_portable_export_copies_the_songs_and_lists_them_relatively() {
    with_timeout(TEST_TIMEOUT, || {
        let dir = temp_dir("export-portable");
        let source = dir.join("source");
        std::fs::create_dir_all(&source).unwrap();
        let file = |name: &str| source.join(name).to_string_lossy().into_owned();
        std::fs::write(file("a.flac"), b"song a").unwrap();
        std::fs::write(file("b.flac"), b"song b").unwrap();
        let (a, b, missing) = (file("a.flac"), file("b.flac"), file("gone.flac"));
        let fixture = LibraryFixture::new()
            .song(FixtureSong::new(&a, "A", "Band", "First"))
            .song(FixtureSong::new(&b, "B", "Band", "Second"))
            .song(FixtureSong::new(&missing, "Gone", "Band", "First"))
            .playlist("List", &[a.as_str(), b.as_str(), missing.as_str()]);
        let rig = Rig::with(fixture);
        let out = dir.join("out");
        std::fs::create_dir_all(&out).unwrap();

        let op = export(
            &rig.ctx,
            rig.playlist_id("List"),
            out.clone(),
            ExportFormat::M3U8,
            ExportMode::PortableFolder,
        );
        let report = wait_op::<ExportReport>(&rig.ctx, op).unwrap();
        assert_eq!(report.playlist_path, out.join("List.m3u8"));
        assert_eq!(report.copied, 2);
        assert_eq!(report.failed_copies.len(), 1);
        assert_eq!(report.failed_copies[0].source, missing);
        assert_eq!(
            std::fs::read(out.join("Band/First/a.flac")).unwrap(),
            b"song a"
        );
        assert_eq!(
            std::fs::read(out.join("Band/Second/b.flac")).unwrap(),
            b"song b"
        );
        assert_eq!(
            std::fs::read_to_string(out.join("List.m3u8")).unwrap(),
            "#EXTM3U\n\
             #EXTINF:240,A\nBand/First/a.flac\n\
             #EXTINF:240,B\nBand/Second/b.flac\n\
             #EXTINF:240,Gone\nBand/First/gone.flac\n"
        );
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn quirk_8_an_export_that_cannot_write_its_list_fails() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let target = temp_dir("export-io").join("missing-dir").join("list.m3u8");
        let op = export(
            &rig.ctx,
            rig.playlist_id("List"),
            target,
            ExportFormat::M3U8,
            ExportMode::SingleFile,
        );
        assert!(matches!(
            wait_op::<ExportReport>(&rig.ctx, op),
            Err(FrontendError::Io(_))
        ));
    });
}

#[test]
fn rounded_durations_never_go_negative() {
    assert_eq!(rounded_duration(252.5), 253.0);
    assert_eq!(rounded_duration(252.49), 252.0);
    assert_eq!(rounded_duration(-3.0), 0.0);
    assert_eq!(rounded_duration(f64::NAN), 0.0);
}

// --- Import and external files ------------------------------------------------------

#[test]
fn importing_an_m3u_creates_a_playlist_named_after_the_file() {
    with_timeout(TEST_TIMEOUT, || {
        let dir = temp_dir("import");
        let file = |name: &str| dir.join(name).to_string_lossy().into_owned();
        for name in ["a.flac", "b.flac"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let (a, b) = (file("a.flac"), file("b.flac"));
        let rig = Rig::with(
            library()
                .song(FixtureSong::new(&a, "A", "Band", "First"))
                .song(FixtureSong::new(&b, "B", "Band", "First"))
                .playlist("List", &[]),
        );
        let (_token, events) = playlist_events(&rig.ctx);
        let m3u = dir.join("Road Trip.m3u");
        std::fs::write(
            &m3u,
            format!("#EXTM3U\n#EXTINF:1,B\nb.flac\n{a}\nmissing.flac\n"),
        )
        .unwrap();

        let id = wait_op::<i64>(&rig.ctx, import_m3u(&rig.ctx, m3u)).unwrap();
        assert_eq!(next_event(&events), PlaylistsEvent::ListChanged);
        assert_eq!(playlist_named(&rig.ctx, "Road Trip").unwrap().id, id);
        let expected = [
            rig.seeded.song_id(&b).unwrap(),
            rig.seeded.song_id(&a).unwrap(),
        ];
        assert_eq!(ids(&stored_songs(&rig.ctx, id)), expected);
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn quirk_8_an_import_that_finds_nothing_or_no_file_fails() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let dir = temp_dir("import-fail");
        let m3u = dir.join("Nothing.m3u8");
        std::fs::write(&m3u, "#EXTM3U\n/nowhere/a.flac\n").unwrap();
        assert!(matches!(
            wait_op::<i64>(&rig.ctx, import_m3u(&rig.ctx, m3u)),
            Err(FrontendError::InvalidInput(_))
        ));
        assert!(matches!(
            wait_op::<i64>(&rig.ctx, import_m3u(&rig.ctx, dir.join("absent.m3u"))),
            Err(FrontendError::Io(_))
        ));
        assert!(playlist_named(&rig.ctx, "Nothing").is_none());
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn adding_external_files_adds_the_audio_files_and_publishes_the_change() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = Rig::new();
        let (_token, events) = playlist_events(&rig.ctx);
        let other = rig.playlist_id("Other");
        let dir = temp_dir("external");
        std::fs::write(dir.join("new.flac"), b"not really audio").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignored").unwrap();

        wait_op::<()>(&rig.ctx, add_external(&rig.ctx, vec![dir.clone()], other)).unwrap();
        assert_eq!(
            next_event(&events),
            PlaylistsEvent::ContentChanged { playlist_id: other }
        );
        let new_path = dir.join("new.flac").to_string_lossy().into_owned();
        let songs = rig
            .ctx
            .db
            .read(move |db| Ok(db.get_playlist_songs(other)?))
            .recv_blocking()
            .unwrap();
        let paths: Vec<String> = songs.iter().map(|s| s.file_path.to_string()).collect();
        assert_eq!(paths.len(), 3, "{paths:?}");
        assert!(paths.contains(&new_path));
        assert!(!paths.iter().any(|p| p.ends_with("notes.txt")));
        assert!(
            events.try_recv().is_err(),
            "no playlist file, no ListChanged"
        );
        let _ = std::fs::remove_dir_all(dir);
    });
}

#[test]
fn only_playlist_files_count_as_lists_to_import() {
    let dir = temp_dir("is-list");
    for name in ["a.m3u", "b.M3U8", "c.flac"] {
        std::fs::write(dir.join(name), b"x").unwrap();
    }
    assert!(is_playlist_file(&dir.join("a.m3u")));
    assert!(is_playlist_file(&dir.join("b.M3U8")));
    assert!(!is_playlist_file(&dir.join("c.flac")));
    assert!(!is_playlist_file(&dir.join("missing.m3u")));
    assert!(!is_playlist_file(&dir));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_playing_song_keeps_its_gains_after_a_reload() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        rig.start(1);
        reload_content(&rig.ctx, &rig.cursor)
            .recv_blocking()
            .unwrap()
            .apply(&mut rig.cursor);
        assert_eq!(rig.cursor.gains(rig.id(2)), GAINS_2);
        let id = rig.id(2);
        playback::play_song(&rig.ctx, &mut rig.cursor, id).unwrap();
        assert_eq!(
            rig.take_calls(),
            vec![
                PlayerCall::Load {
                    path: path(2),
                    gains: GAINS_2
                },
                PlayerCall::Play
            ]
        );
        assert!(rig.fakes.player.is_playing());
    });
}
