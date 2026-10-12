//! The playback session: the startup restore, the save on close and at each
//! autosave, and the final stop.
//!
//! [`restore`] is the whole startup restore of the Iced UI's
//! `AudoxidyApp::new`, run **once** per context, on the facade's workers
//! (quirk fix 6: Iced restores the playlist and track a second time from
//! `InitStartup`):
//!
//! 1. every persisted audio setting, as
//!    [`restore_persisted`](engine_settings::restore_persisted) restores them
//!    (the sound-server clock, the stream, the volume, mix, crossfade, meter,
//!    equalizer, effects and downmix);
//! 2. the last active playlist (`last_active_playlist_id`, playlist 1 when it
//!    is not set, as in Iced) with its repeat mode, shuffle flag and shuffle
//!    session (Iced's position rule);
//! 3. its last song, loaded with its ReplayGain values, seeked to the saved
//!    position and played or paused as saved: playback resumes where it was,
//!    as in Iced;
//! 4. a pending end-of-track flag is taken once, so that the restored load
//!    does not count as the end of a track;
//! 5. `Session::Restored` is published. The UI sends no playback command
//!    before it.
//!
//! When the saved playlist no longer exists, the first playlist is restored
//! instead (quirk fix 30; Iced would show an empty list).
//!
//! [`persist_now`] saves the playback state; the UI calls it from the window's
//! close hook and on `Session::AutosaveDue`. [`stop`] stops playback and the
//! facade's threads, after the queued saves are written (quirk fix 3).

#![warn(missing_docs)]

use super::app_context::AppContext;
use super::commands::engine_settings::{self, RestoreReport};
use super::commands::playback::{PlaybackCursor, track_request};
use super::commands::playlist::{self, LoadedPlaylist};
use super::error::FrontendError;
use super::events::{Event, RestoredSession, SessionEvent};
use super::reply::Reply;
use super::settings::PlayerSettings;
use crate::db::Database;

/// The playlist restored when `last_active_playlist_id` is not set, as in
/// Iced (`PlaylistManager::default`).
pub const DEFAULT_PLAYLIST_ID: i64 = 1;

/// What [`restore`] put back.
#[derive(Debug, Clone)]
pub struct SessionRestore {
    /// The restored session, as published in `Session::Restored`.
    pub session: RestoredSession,
    /// The restored playlist, with the restored song as the playing one: the
    /// playlist view-model's starting state.
    pub cursor: PlaybackCursor,
    /// What the settings restore did (the view-models of the Audio Center
    /// start from its settings).
    pub settings: RestoreReport,
}

/// Restores the session at startup; see the module documentation for the
/// order.
///
/// Everything runs on the workers, never on the caller's thread: the
/// settings and the track are applied as two ordered jobs on the engine
/// worker, the reads on the database worker. It runs once per context: a
/// second call, or a call after
/// [`restore_persisted`](engine_settings::restore_persisted), resolves to
/// [`FrontendError::InvalidInput`] without touching anything. On failure,
/// nothing is published and the reply carries the error.
pub fn restore(ctx: &AppContext) -> Reply<SessionRestore> {
    if !ctx.claim_startup_restore() {
        return Reply::ready(Err(FrontendError::InvalidInput(
            "the session was already restored on this context".to_string(),
        )));
    }
    let settings = engine_settings::restore_engine_settings(ctx);
    let loaded = ctx.db.read(read_last_playlist);
    let hub = ctx.hub.clone();
    // Queued after the settings job, so the track is restored after them.
    ctx.engine.run(move |engine| {
        let settings = settings.recv_blocking()?;
        let loaded = loaded.recv_blocking()?;
        let saved = loaded.data.clone();
        let mut cursor = PlaybackCursor::new(loaded);
        let player = &engine.player;

        // A window closed while the restore waited in the queue: the
        // shutdown drains the queue, and this job must not start playback
        // behind the stop (the cursor still comes back with nothing playing).
        let song = saved
            .last_song_id
            .filter(|_| !engine.is_closing())
            .and_then(|id| cursor.song(id).map(|song| (id, track_request(song))));
        let (song_id, position_secs, is_playing) = match song {
            Some((song_id, track)) => {
                if let Err(e) = player.load_file(&track, cursor.gains(song_id)) {
                    tracing::warn!("the last song could not be loaded at startup: {e}");
                }
                player.seek(saved.last_pos_sec);
                player.set_playing(saved.is_playing);
                cursor.playing = Some(song_id);
                (Some(song_id), saved.last_pos_sec, saved.is_playing)
            }
            None => (None, 0.0, false),
        };
        let _ = player.take_eof();

        let session = RestoredSession {
            playlist_id: cursor.playlist_id(),
            song_id,
            position_secs,
            is_playing,
            shuffle: cursor.shuffle_active(),
            repeat: cursor.repeat(),
        };
        hub.publish(Event::Session(SessionEvent::Restored(session.clone())));
        Ok(SessionRestore {
            session,
            cursor,
            settings,
        })
    })
}

/// Reads the playlist to restore: the last active one, or the first one when
/// it no longer exists.
fn read_last_playlist(db: &Database) -> Result<LoadedPlaylist, FrontendError> {
    let saved = PlayerSettings::read(db)
        .last_active_playlist_id
        .unwrap_or(DEFAULT_PLAYLIST_ID);
    let playlist_id = if db.get_playlist_by_id(saved)?.is_some() {
        saved
    } else {
        let first = db
            .get_all_playlists()?
            .first()
            .map(|playlist| playlist.id)
            .ok_or_else(|| {
                FrontendError::InvalidInput("the database holds no playlist".to_string())
            })?;
        tracing::warn!(
            saved,
            first,
            "the last active playlist no longer exists; restoring the first playlist"
        );
        first
    };
    playlist::read_playlist(db, playlist_id)
}

/// Saves the playback state now (quirk fixes 3 and 13): the playlist state
/// with the player's current position and play state
/// ([`playlist::persist_state`]) and, in shuffle, the shuffle session.
///
/// The player is read at once and the writes are queued on the database
/// worker; the reply resolves once they are stored. The window's close hook
/// calls it, then [`stop`]: the shutdown writes every queued job before the
/// database worker stops.
pub fn persist_now(ctx: &AppContext, cursor: &PlaybackCursor) -> Reply<()> {
    let mut replies = vec![playlist::persist_state(ctx, cursor)];
    if cursor.shuffle_active()
        && let Some(session) = cursor.shuffle_session()
    {
        replies.push(playlist::save_shuffle_session(
            ctx,
            cursor.playlist_id(),
            session,
        ));
    }
    playlist::all_settled(&ctx.db, replies)
}

/// Stops playback, then the facade's threads ([`AppContext::shutdown`]): the
/// database worker writes every queued job, including the saves of
/// [`persist_now`], before it stops. Returns `true` when every thread was
/// joined in time.
///
/// A [`restore`] still queued on the engine worker (a window closed during
/// the startup) runs during the shutdown; it sees the worker closing and
/// loads nothing, and playback is stopped once more after the workers have
/// stopped, so nothing plays after the close.
pub fn stop(ctx: &AppContext) -> bool {
    ctx.player.stop();
    let joined = ctx.shutdown();
    ctx.player.stop();
    joined
}

#[cfg(test)]
mod tests;
