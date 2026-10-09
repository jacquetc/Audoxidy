use super::*;
use crate::frontend::commands::engine_settings::rig::Rig as RecordingRig;
use crate::frontend::commands::playback::tests::{GAINS_2, Rig, fixture, path};
use crate::frontend::commands::playback::{self, TrackChange};
use crate::frontend::events::Origin;
use crate::frontend::fakes::PlayerCall;
use crate::frontend::ports::PlayerPort;
use crate::frontend::settings::keys;
use crate::frontend::test_fixtures::SeededLibrary;
use crate::frontend::threading::test_support::{TEST_TIMEOUT, with_timeout};
use crate::logic::playlist_nav::RepeatMode;
use std::sync::{Arc, mpsc};

/// Collects the `Session` events.
fn session_events(
    ctx: &AppContext,
) -> (
    crate::frontend::events::SubscriptionToken,
    mpsc::Receiver<SessionEvent>,
) {
    let (tx, rx) = mpsc::channel();
    let tx = parking_lot::Mutex::new(tx);
    let token = ctx.hub.subscribe(
        Origin::Session,
        Arc::new(move |event| {
            if let Event::Session(event) = event {
                let _ = tx.lock().send(event);
            }
        }),
    );
    (token, rx)
}

/// The saved state of a session: playlist `name` active, its song `song`
/// at `position_secs`, playing or not.
struct Saved<'a> {
    playlist: &'a str,
    song: usize,
    position_secs: f64,
    playing: bool,
    shuffle: bool,
}

fn seed(ctx: &AppContext, saved: &Saved<'_>) -> SeededLibrary {
    let seeded = fixture()
        .setting(keys::PLAYER_VOLUME, "0.4200")
        .setting(keys::EQ_ENABLED, "1")
        .seed_on(&ctx.db)
        .recv_blocking()
        .unwrap();
    let playlist_id = seeded.playlist(saved.playlist).unwrap().id;
    let song_id = seeded.song_id(&path(saved.song)).unwrap();
    let (position, playing, shuffle) = (saved.position_secs, saved.playing, saved.shuffle);
    ctx.db
        .write(move |db| {
            db.set_setting(keys::LAST_ACTIVE_PLAYLIST_ID, &playlist_id.to_string())?;
            Ok(db.update_playlist_persistence(
                playlist_id,
                Some(song_id),
                position,
                playing,
                shuffle,
                1,
                0,
                None,
            )?)
        })
        .recv_blocking()
        .unwrap();
    seeded
}

const PLAYING_2: Saved<'static> = Saved {
    playlist: "List",
    song: 2,
    position_secs: 42.0,
    playing: true,
    shuffle: false,
};

#[test]
fn quirk_6_the_restore_runs_the_settings_then_the_track_once_and_publishes_restored() {
    with_timeout(TEST_TIMEOUT, || {
        let rig = RecordingRig::new();
        let seeded = seed(&rig.ctx, &PLAYING_2);
        let (_token, events) = session_events(&rig.ctx);

        let restored = restore(&rig.ctx).recv_blocking().unwrap();

        let names = rig.step_names();
        let dsp = names.iter().position(|n| *n == "update_dsp").unwrap();
        let load = names.iter().position(|n| *n == "load_file").unwrap();
        assert!(dsp < load, "the settings come first: {names:?}");
        assert_eq!(names[load..], ["load_file", "seek", "set_playing"]);
        assert_eq!(names[0], "is_pipewire_active");

        let list = seeded.playlist("List").unwrap().id;
        let two = seeded.song_id(&path(2)).unwrap();
        let expected = RestoredSession {
            playlist_id: list,
            song_id: Some(two),
            position_secs: 42.0,
            is_playing: true,
            shuffle: false,
            repeat: RepeatMode::All,
        };
        assert_eq!(restored.session, expected);
        assert_eq!(
            events.recv_timeout(TEST_TIMEOUT).unwrap(),
            SessionEvent::Restored(expected)
        );
        assert_eq!(restored.cursor.playlist_id(), list);
        assert_eq!(restored.cursor.playing_song_id(), Some(two));
        assert_eq!(restored.settings.settings.player.volume, Some(0.42));

        let state = rig.player.snapshot();
        assert_eq!(state.path, path(2));
        assert_eq!(state.current_pos_sec, 42.0);
        assert!(state.is_playing, "playback resumes, as in Iced");
        assert_eq!(state.volume, 0.42);
        assert!(rig.player.calls().contains(&PlayerCall::Load {
            path: path(2),
            gains: GAINS_2
        }));

        assert!(matches!(
            restore(&rig.ctx.clone()).recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
        assert!(matches!(
            engine_settings::restore_persisted(&rig.ctx).recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
        assert_eq!(rig.step_names(), names, "the later calls touched nothing");
        assert!(events.try_recv().is_err(), "Restored is published once");
    });
}

#[test]
fn a_settings_restore_first_blocks_the_session_restore() {
    with_timeout(TEST_TIMEOUT, || {
        let (ctx, _fakes) = AppContext::in_memory_with_fakes().unwrap();
        seed(&ctx, &PLAYING_2);
        engine_settings::restore_persisted(&ctx)
            .recv_blocking()
            .unwrap();
        assert!(matches!(
            restore(&ctx).recv_blocking(),
            Err(FrontendError::InvalidInput(_))
        ));
    });
}

#[test]
fn a_session_saved_paused_is_restored_paused() {
    with_timeout(TEST_TIMEOUT, || {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        seed(
            &ctx,
            &Saved {
                playing: false,
                ..PLAYING_2
            },
        );
        let restored = restore(&ctx).recv_blocking().unwrap();
        assert!(!restored.session.is_playing);
        let state = fakes.player.snapshot();
        assert!(!state.is_playing);
        assert_eq!(state.current_pos_sec, 42.0);
        let calls = fakes.player.calls();
        let tail = &calls[calls.len() - 2..];
        assert_eq!(
            tail,
            [PlayerCall::Seek(42.0), PlayerCall::SetPlaying(false)]
        );
    });
}

#[test]
fn a_restored_shuffle_playlist_comes_back_with_its_session() {
    with_timeout(TEST_TIMEOUT, || {
        let (ctx, _fakes) = AppContext::in_memory_with_fakes().unwrap();
        let seeded = seed(
            &ctx,
            &Saved {
                shuffle: true,
                ..PLAYING_2
            },
        );
        let list = seeded.playlist("List").unwrap().id;
        let ids: Vec<i64> = [4, 1, 2, 5, 3]
            .iter()
            .map(|&n| seeded.song_id(&path(n)).unwrap())
            .collect();
        let session = crate::db::database::ShuffleSession {
            session_id: "s".to_string(),
            shuffle_order: ids.clone(),
            current_position: 0,
            history: Vec::new(),
        };
        ctx.db
            .write(move |db| Ok(db.save_shuffle_session(list, &session)?))
            .recv_blocking()
            .unwrap();

        let restored = restore(&ctx).recv_blocking().unwrap();
        assert!(restored.session.shuffle);
        let session = restored.cursor.shuffle_session().unwrap();
        let two = ids[2];
        let at = session
            .shuffle_order
            .iter()
            .position(|&id| id == two)
            .unwrap();
        assert_eq!(session.current_position, at + 1, "Iced's position rule");
        assert!(session.history.contains(&two));
    });
}

#[test]
fn quirk_30_the_restore_falls_back_to_the_first_playlist_when_the_saved_one_is_gone() {
    with_timeout(TEST_TIMEOUT, || {
        let (ctx, fakes) = AppContext::in_memory_with_fakes().unwrap();
        let seeded = seed(&ctx, &PLAYING_2);
        ctx.db
            .write(|db| Ok(db.set_setting(keys::LAST_ACTIVE_PLAYLIST_ID, "9999")?))
            .recv_blocking()
            .unwrap();
        fakes.player.state().write().eof_reached = true;
        let restored = restore(&ctx).recv_blocking().unwrap();
        assert!(
            !fakes.player.snapshot().eof_reached,
            "a pending end of track is taken once by the restore"
        );
        assert_eq!(restored.session.playlist_id, seeded.main_playlist_id);
        assert_eq!(restored.session.song_id, None);
        assert!(!restored.session.is_playing);
        assert!(
            !fakes
                .player
                .calls()
                .iter()
                .any(|call| matches!(call, PlayerCall::Load { .. })),
            "the first playlist has no saved song"
        );
    });
}

#[test]
fn without_a_saved_playlist_the_restore_opens_playlist_one() {
    with_timeout(TEST_TIMEOUT, || {
        let (ctx, _fakes) = AppContext::in_memory_with_fakes().unwrap();
        let seeded = fixture().seed_on(&ctx.db).recv_blocking().unwrap();
        assert_eq!(seeded.main_playlist_id, DEFAULT_PLAYLIST_ID);
        let restored = restore(&ctx).recv_blocking().unwrap();
        assert_eq!(restored.session.playlist_id, DEFAULT_PLAYLIST_ID);
        assert_eq!(restored.session.song_id, None);
        assert_eq!(restored.session.repeat, RepeatMode::Off);
    });
}

#[test]
fn quirk_3_persist_now_then_stop_stores_the_position_and_the_session() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        rig.start(2);
        playback::toggle_shuffle(&rig.ctx, &mut rig.cursor);
        rig.fakes.player.advance(77.0);

        let saved = persist_now(&rig.ctx, &rig.cursor);
        assert!(stop(&rig.ctx), "every thread stopped in time");
        assert!(rig.ctx.is_shut_down());
        assert!(
            saved.recv_blocking().is_ok(),
            "written before the worker stopped"
        );
        assert_eq!(rig.fakes.player.calls().last(), Some(&PlayerCall::Stop));

        // The workers are gone: read the connection directly.
        let connection = rig.ctx.db.connection();
        let db = connection.lock().unwrap();
        let stored = db.get_playlist_by_id(list).unwrap().unwrap();
        assert_eq!(stored.last_song_id, Some(rig.id(2)));
        assert_eq!(stored.last_pos_sec, 77.0);
        assert!(stored.is_playing, "the next start resumes playback");
        assert!(stored.shuffle_active);
        let session = db.load_shuffle_session(list).unwrap().unwrap();
        assert_eq!(
            Some(session.session_id.as_str()),
            rig.cursor.shuffle_session().map(|s| s.session_id.as_str())
        );
    });
}

#[test]
fn quirk_13_persist_now_on_autosave_resolves_once_the_state_is_stored() {
    with_timeout(TEST_TIMEOUT, || {
        let mut rig = Rig::new();
        let list = rig.playlist_id("List");
        assert_eq!(
            playback::play_song(
                &rig.ctx,
                &mut rig.cursor,
                rig.seeded.song_id(&path(4)).unwrap()
            )
            .unwrap(),
            TrackChange::Loaded(rig.id(4))
        );
        rig.fakes.player.advance(20.0);
        persist_now(&rig.ctx, &rig.cursor).recv_blocking().unwrap();
        let stored = rig
            .ctx
            .db
            .read(move |db| Ok(db.get_playlist_by_id(list)?))
            .recv_blocking()
            .unwrap()
            .unwrap();
        assert_eq!(stored.last_pos_sec, 20.0);
        assert_eq!(stored.last_song_id, Some(rig.id(4)));
    });
}
