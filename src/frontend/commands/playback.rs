//! Playback commands: the transport, the volume, playing a song, Next and
//! Previous, the end of a track, the preload, repeat and shuffle, and the
//! system media controls.
//!
//! The playback state belongs to the UI's view-model, as a
//! [`PlaybackCursor`]: the active playlist's groups, the playing song (by
//! id), repeat, shuffle and the shuffle session. Commands take the cursor by
//! `&mut`, update it, and do the I/O:
//!
//! - **The player** is called directly, on the caller's thread: loads,
//!   preloads and transport calls only send a command to the decoder or flip
//!   a state flag (see [`PlayerPort`](crate::frontend::ports::PlayerPort)), so
//!   they keep the order in which the UI issued them. The buffer purge Iced
//!   runs on pause and stop rebuilds the stream, so it is requested from the
//!   engine worker instead ([`EngineWorker::request_purge`]); it is skipped
//!   if playback resumed before it runs.
//! - **The ReplayGain values** come from the cursor, which holds the gains of
//!   every song of its playlist, read on the database worker with the
//!   playlist (`get_replay_gain_by_path`). They are always passed explicitly.
//! - **The persistence** (playlist state, shuffle session, volume) is queued
//!   on the database worker. The commands do not wait for it.
//!
//! The commands reproduce the Iced handlers of `src/gui/app.rs` (`PlayPause`,
//! `SetPlaying`, `Stop`, `SeekTo`, `VolumeChanged`, `PlayerScroll`,
//! `NextTrack`, `PreviousTrack`, `ToggleRepeat`, `ToggleShuffle`,
//! `ExternalControl`, `play_song_from_playlist` and the `Tick` preload and
//! end-of-track paths), with these Teksilo-only changes, recorded in
//! `docs/teksilo-port-quirk-fixes.md`:
//!
//! - the playing song is tracked by song id (quirk fix 11);
//! - Previous wraps under Repeat All and skips songs that left the playlist
//!   in shuffle, and enabling shuffle with no enabled song drops the old
//!   session (quirk fixes 22 to 24, Iced defects D14 to D16);
//! - Next and Previous settle a pending end of track first, so they move
//!   once from the track actually playing (quirk fix 28);
//! - a track change saves position 0 for the new track (quirk fix 29), unless
//!   the engine skips the load because that track already plays: then its
//!   position is saved;
//! - Repeat One replays the playing song, found by id even when a search
//!   hides it (quirk fix 11);
//! - a pending preload that is no longer the song Next would play is dropped
//!   from the engine (Iced defect D28).
//!
//! [`EngineWorker::request_purge`]: crate::frontend::engine_worker::EngineWorker::request_purge

#![warn(missing_docs)]

use std::collections::{HashMap, HashSet};

use super::playlist::{self, LoadedPlaylist};
use crate::db::database::{PlaylistFolderGroup, PlaylistSongRef, ShuffleSession};
use crate::frontend::app_context::AppContext;
use crate::frontend::error::FrontendError;
use crate::frontend::monitor::unix_now_secs;
use crate::frontend::ports::{ReplayGains, TrackRequest};
use crate::frontend::settings;
use crate::integrations::control::ExternalControlEvent;
use crate::logic::playlist_nav::{self as nav, RepeatMode};
use crate::logic::transport::{
    TransportAction, clamp_volume, external_control_action, fine_seek_enabled, seek_target,
};

/// How close to the end of a track the next one is preloaded, in seconds.
pub const PRELOAD_LEAD_SECS: f64 = 15.0;

/// The step of a mouse-wheel volume change on the player panel, in percent.
pub const VOLUME_WHEEL_STEP_PERCENT: f32 = 5.0;

/// The playback state of the active playlist, owned by the UI's view-model.
///
/// It holds the playlist's songs grouped by folder, the search applied to
/// them, the ReplayGain values of its songs, the playing song **by id**
/// (quirk fix 11), the repeat mode, the shuffle flag and the shuffle session.
///
/// # How a view-model uses it
///
/// - It gets one from [`session::restore`](crate::frontend::session::restore)
///   at startup and from [`playlist::PlaylistSwitch::apply`] when the user
///   opens another playlist; [`playlist::PlaylistContent::apply`] refreshes
///   its rows after a content change without touching playback.
/// - It owns the cursor on the UI thread and passes `&mut` to the playback
///   and playlist commands, which update it and talk to the backend.
/// - It renders the rows of [`PlaybackCursor::active_groups`] (the search
///   result when a search is applied, as in Iced); the selection and every
///   `linear_idx` a command takes are linear indices into that list. The
///   playing marker is the row whose song id is
///   [`PlaybackCursor::playing_song_id`], so a search cannot move it to the
///   wrong row.
/// - Navigation follows the displayed list, as in Iced. When the search hides
///   the playing song, Next starts from the top of the search result.
#[derive(Debug, Clone, Default)]
pub struct PlaybackCursor {
    pub(crate) playlist_id: i64,
    pub(crate) groups: Vec<PlaylistFolderGroup>,
    pub(crate) search: String,
    pub(crate) filtered: Option<Vec<PlaylistFolderGroup>>,
    pub(crate) gains: HashMap<i64, ReplayGains>,
    pub(crate) playing: Option<i64>,
    pub(crate) repeat: RepeatMode,
    pub(crate) shuffle: bool,
    pub(crate) session: Option<ShuffleSession>,
}

impl PlaybackCursor {
    /// The cursor of a loaded playlist: its songs, gains, repeat mode,
    /// shuffle flag and session, no search, and no playing song.
    pub fn new(loaded: LoadedPlaylist) -> Self {
        Self {
            playlist_id: loaded.data.id,
            groups: loaded.groups,
            search: String::new(),
            filtered: None,
            gains: loaded.gains,
            playing: None,
            repeat: RepeatMode::from_u8(u8::try_from(loaded.data.repeat_mode).unwrap_or(0)),
            shuffle: loaded.data.shuffle_active,
            session: loaded.session,
        }
    }

    /// The playlist's id.
    pub fn playlist_id(&self) -> i64 {
        self.playlist_id
    }

    /// Every song of the playlist, grouped by folder.
    pub fn groups(&self) -> &[PlaylistFolderGroup] {
        &self.groups
    }

    /// The groups on screen: the search result when a search is applied,
    /// otherwise every group.
    pub fn active_groups(&self) -> &[PlaylistFolderGroup] {
        self.filtered.as_deref().unwrap_or(&self.groups)
    }

    /// The search applied to the playlist ("" for none).
    pub fn search(&self) -> &str {
        &self.search
    }

    /// Whether a search currently filters the rows. A search that matches
    /// nothing shows every row, as in Iced.
    pub fn is_filtered(&self) -> bool {
        self.filtered.is_some()
    }

    /// Applies a search (title, artist, album artist or album, ignoring
    /// case; `logic::playlist_nav::filter_groups`). The selection's linear
    /// indices change with it.
    pub fn set_search(&mut self, query: &str) {
        self.search = query.to_string();
        self.refilter();
    }

    fn refilter(&mut self) {
        self.filtered = nav::filter_groups(&self.groups, &self.search);
    }

    /// The id of the playing song, if any.
    pub fn playing_song_id(&self) -> Option<i64> {
        self.playing
    }

    /// The linear index of the playing song in [`PlaybackCursor::active_groups`];
    /// `None` when nothing plays or the search hides it.
    pub fn playing_index(&self) -> Option<usize> {
        self.playing
            .and_then(|id| nav::linear_index_of_song_id(self.active_groups(), id))
    }

    /// The playing song, if it is in the playlist.
    pub fn playing_song(&self) -> Option<&PlaylistSongRef> {
        self.playing.and_then(|id| self.song(id))
    }

    /// The song `song_id` of the playlist (first occurrence).
    pub fn song(&self, song_id: i64) -> Option<&PlaylistSongRef> {
        nav::linear_index_of_song_id(&self.groups, song_id)
            .and_then(|idx| nav::song_at(&self.groups, idx))
    }

    /// The song at `linear_idx` in [`PlaybackCursor::active_groups`]; `None`
    /// for a separator.
    pub fn song_at(&self, linear_idx: usize) -> Option<&PlaylistSongRef> {
        nav::song_at(self.active_groups(), linear_idx)
    }

    /// The ReplayGain values of `song_id` (none when the song has none).
    pub fn gains(&self, song_id: i64) -> ReplayGains {
        self.gains.get(&song_id).copied().unwrap_or_default()
    }

    /// The repeat mode.
    pub fn repeat(&self) -> RepeatMode {
        self.repeat
    }

    /// Whether shuffle is on.
    pub fn shuffle_active(&self) -> bool {
        self.shuffle
    }

    /// The shuffle session, if any.
    pub fn shuffle_session(&self) -> Option<&ShuffleSession> {
        self.session.as_ref()
    }

    /// The song Next would play, for the preload and the cover prefetch:
    /// `None` when nothing plays, at a re-shuffle and at the end
    /// (`logic::playlist_nav::next_song_ref`). Under Repeat One it is the
    /// playing song, by id, even when a search hides it (quirk fix 11).
    pub fn next_song(&self) -> Option<&PlaylistSongRef> {
        if self.repeat == RepeatMode::One {
            return self.playing_song();
        }
        nav::next_song_ref(
            self.active_groups(),
            self.playing_index(),
            self.repeat,
            self.shuffle,
            self.session.as_ref(),
        )
    }

    /// The song ids a playlist action on the row `clicked` applies to (send
    /// to a list, remove): the whole selection when `clicked` is selected,
    /// otherwise `clicked` alone, resolved over the displayed rows
    /// (`logic::playlist_nav::action_target_song_ids`, Iced defect D2).
    pub fn action_targets(&self, selected: &HashSet<usize>, clicked: usize) -> Vec<i64> {
        nav::action_target_song_ids(self.active_groups(), selected, clicked)
    }

    /// Replaces the songs and their gains (a content reload); the search is
    /// applied again, playback is left alone.
    pub(crate) fn replace_content(
        &mut self,
        groups: Vec<PlaylistFolderGroup>,
        gains: HashMap<i64, ReplayGains>,
    ) {
        self.groups = groups;
        self.gains = gains;
        self.refilter();
    }

    /// The linear index of the playing song in `groups`.
    fn index_in(groups: &[PlaylistFolderGroup], playing: Option<i64>) -> Option<usize> {
        playing.and_then(|id| nav::linear_index_of_song_id(groups, id))
    }

    /// Plays the next song in the cursor (`logic::playlist_nav::advance_next`)
    /// and returns its id; `None` when there is nothing to play.
    ///
    /// Repeat One replays the playing song, found by id in the whole playlist
    /// (quirk fix 11): a search that hides it, or a change that moves it, does
    /// not stop playback. Iced replays whatever row its stale index points at
    /// (`src/gui/playlist.rs:1252-1265`); a song removed from the playlist
    /// has nothing to replay, and playback stops.
    fn advance(&mut self) -> Option<i64> {
        if self.repeat == RepeatMode::One {
            return self.playing_song().map(|song| song.song_id);
        }
        let groups = self.filtered.as_deref().unwrap_or(&self.groups);
        let mut playing = Self::index_in(groups, self.playing);
        let target = nav::advance_next(
            groups,
            &mut playing,
            self.repeat,
            &mut self.shuffle,
            &mut self.session,
            &mut fastrand::Rng::new(),
        );
        let song_id = target
            .and_then(|idx| nav::song_at(groups, idx))
            .map(|song| song.song_id);
        if song_id.is_some() {
            self.playing = song_id;
        }
        song_id
    }

    /// Goes back one song in the cursor (`logic::playlist_nav::step_prev_fixed`)
    /// and returns its id; `None` when there is no previous song.
    fn step_back(&mut self) -> Option<i64> {
        let groups = self.filtered.as_deref().unwrap_or(&self.groups);
        let target = nav::step_prev_fixed(
            groups,
            Self::index_in(groups, self.playing),
            self.repeat,
            self.shuffle,
            self.session.as_mut(),
        );
        let song_id = target
            .and_then(|idx| nav::song_at(groups, idx))
            .map(|song| song.song_id);
        if song_id.is_some() {
            self.playing = song_id;
        }
        song_id
    }
}

/// What a command did to the player's track.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackChange {
    /// This song was loaded (or confirmed, when the engine had already
    /// promoted it) and is now the playing song.
    Loaded(i64),
    /// The playing song restarted from the beginning.
    Restarted(i64),
    /// There was nothing to play: the track did not change.
    Unchanged,
}

/// What [`end_of_track`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndOfTrackOutcome {
    /// No end of track was pending (the event can repeat): nothing was done.
    NotPending,
    /// The playlist advanced to this song.
    Advanced(i64),
    /// The playlist ended: playback stays stopped.
    Stopped,
}

/// The track the engine is asked to load for `song`.
pub(crate) fn track_request(song: &PlaylistSongRef) -> TrackRequest {
    TrackRequest {
        path: song.file_path.to_string(),
        title: song.title.to_string(),
        artist: song.artist_name.to_string(),
        album: song.album_title.to_string(),
        cover_path: song.cover_path.as_deref().map(str::to_string),
    }
}

/// Loads `song_id` with its gains and starts playback (Iced's
/// `load_and_play`).
fn load_and_play(
    ctx: &AppContext,
    cursor: &PlaybackCursor,
    song_id: i64,
) -> Result<(), FrontendError> {
    let song = cursor.song(song_id).ok_or_else(|| {
        FrontendError::InvalidInput(format!("song {song_id} is not in the playlist"))
    })?;
    ctx.player
        .load_file(&track_request(song), cursor.gains(song_id))?;
    ctx.player.play();
    Ok(())
}

/// Whether the engine will skip a load of `song_id`: its track already
/// plays, and has not reached its end (the duplicate-load rule of
/// [`PlayerPort::load_file`](crate::frontend::ports::PlayerPort::load_file)).
/// Read before the load.
fn load_is_skipped(ctx: &AppContext, cursor: &PlaybackCursor, song_id: i64) -> bool {
    let state = ctx.player.snapshot();
    state.is_playing
        && !state.eof_reached
        && cursor
            .song(song_id)
            .is_some_and(|song| song.file_path.as_ref() == state.path.as_str())
}

/// Persists the playlist state after a track change: the new track starts at
/// 0 (quirk fix 29; the engine's position may still be the previous
/// track's). When the engine skipped the load (`skipped`: the same track
/// plays on, as after Next under Repeat One), its position is the track's
/// and is saved instead.
fn persist_after_change(ctx: &AppContext, cursor: &PlaybackCursor, skipped: bool) {
    let state = ctx.player.snapshot();
    let position = if skipped { state.current_pos_sec } else { 0.0 };
    let _ = playlist::persist_state_at(ctx, cursor, position, state.is_playing);
}

/// Drops a pending preload that is no longer the song Next would play (Iced
/// defect D28, fixed here only): the decoder would otherwise promote it at
/// the end of the track, playing a song the playlist did not choose while
/// the cursor stays on the ended one. It happens when the next song
/// disappears or changes after the preload: Repeat Off on the last song,
/// shuffle toggled, the next song removed, disabled or hidden by a search.
/// The engine's preload is purged and the bookkeeping forgotten, so
/// [`maybe_preload`] preloads the new next song, if any, on its next call.
/// Returns whether a preload was dropped. Iced never calls
/// `AudioManager::clear_preload` (`src/gui/app.rs:1371-1448`).
pub fn drop_stale_preload(ctx: &AppContext, cursor: &PlaybackCursor) -> bool {
    let Some(pending) = ctx.preload.preloaded_next_path() else {
        return false;
    };
    if cursor
        .next_song()
        .is_some_and(|next| next.file_path.as_ref() == pending.as_str())
    {
        return false;
    }
    if let Err(e) = ctx.player.purge_preload() {
        tracing::warn!("the stale preload could not be dropped: {e}");
    }
    ctx.preload.clear();
    true
}

/// Saves the shuffle session when shuffle is on, as Iced does after every
/// track change.
fn save_session_if_shuffling(ctx: &AppContext, cursor: &PlaybackCursor) {
    if cursor.shuffle
        && let Some(session) = &cursor.session
    {
        let _ = playlist::save_shuffle_session(ctx, cursor.playlist_id, session);
    }
}

/// After `take_eof` returned `true` outside [`end_of_track`]: when the engine
/// promoted the preloaded track, which now plays, the cursor catches up with
/// it (the end of track's advance, without a load). When playback stopped at
/// the end instead, the ended track is still the current one and nothing
/// changes.
fn settle_pending_end(ctx: &AppContext, cursor: &mut PlaybackCursor) {
    let playing_path = ctx.player.snapshot().path;
    let promoted = cursor.next_song().is_some_and(|next| {
        next.file_path.as_ref() == playing_path.as_str() && Some(next.song_id) != cursor.playing
    });
    if promoted {
        cursor.advance();
        ctx.preload.mark_track_advanced(unix_now_secs());
        save_session_if_shuffling(ctx, cursor);
    }
}

// --- Transport --------------------------------------------------------------

/// Sets the play state (Iced's `SetPlaying`): pausing requests a purge of the
/// stream buffers, which is skipped if playback resumes before it runs. The
/// playlist state is persisted.
pub fn set_playing(ctx: &AppContext, cursor: &PlaybackCursor, playing: bool) {
    ctx.player.set_playing(playing);
    if !playing {
        tracing::debug!("Audoxidy Audio: Cleaning Buffers on Pause (Memory Recovery).");
        let _ = ctx.engine.request_purge();
    }
    let _ = playlist::persist_state(ctx, cursor);
}

/// Starts or resumes playback ([`set_playing`] with `true`).
pub fn play(ctx: &AppContext, cursor: &PlaybackCursor) {
    set_playing(ctx, cursor, true);
}

/// Pauses playback ([`set_playing`] with `false`, so the buffers are purged).
pub fn pause(ctx: &AppContext, cursor: &PlaybackCursor) {
    set_playing(ctx, cursor, false);
}

/// Toggles between playing and paused (Iced's `PlayPause`) and returns the
/// new play state. Pausing requests a buffer purge, as [`set_playing`] does.
pub fn toggle_play_pause(ctx: &AppContext, cursor: &PlaybackCursor) -> bool {
    ctx.player.toggle_play_pause();
    let playing = ctx.player.is_playing();
    if !playing {
        tracing::debug!("Audoxidy Audio: Cleaning Buffers on Pause (Memory Recovery).");
        let _ = ctx.engine.request_purge();
    }
    let _ = playlist::persist_state(ctx, cursor);
    playing
}

/// Stops playback and rewinds, then requests a buffer purge (Iced's `Stop`).
/// Nothing is persisted, as in Iced.
pub fn stop(ctx: &AppContext) {
    ctx.player.stop();
    let _ = ctx.engine.request_purge();
}

/// Seeks to `position_secs` (Iced's `SeekTo`), clamped to the track
/// (`logic::transport::seek_target`), and persists the position. Returns
/// `false`, doing nothing, while the duration is unknown
/// (`logic::transport::fine_seek_enabled`).
pub fn seek(ctx: &AppContext, cursor: &PlaybackCursor, position_secs: f64) -> bool {
    let state = ctx.player.snapshot();
    if !fine_seek_enabled(state.total_duration_sec) {
        return false;
    }
    let target = seek_target(position_secs, 0.0, state.total_duration_sec);
    ctx.player.seek(target);
    let _ = playlist::persist_state_at(ctx, cursor, target, state.is_playing);
    true
}

/// Seeks by `delta_secs` from the current position, clamped to the track
/// (the relative seeks of the media controls and the keyboard). Returns
/// `false` while the duration is unknown.
pub fn seek_by(ctx: &AppContext, cursor: &PlaybackCursor, delta_secs: f64) -> bool {
    let state = ctx.player.snapshot();
    let target = seek_target(state.current_pos_sec, delta_secs, state.total_duration_sec);
    seek(ctx, cursor, target)
}

// --- Volume -------------------------------------------------------------------

/// Sets the volume (Iced's `VolumeChanged`), clamped to 0.0..=1.0 (a value
/// that is not a number is 0), and persists it under `player_volume`.
/// Returns the volume applied.
pub fn set_volume(ctx: &AppContext, volume: f32) -> f32 {
    let volume = clamp_volume(f64::from(volume));
    ctx.player.set_volume(volume);
    let _ = settings::write_player_volume(&ctx.db, volume);
    volume
}

/// A mouse-wheel step over the player panel (Iced's `PlayerScroll`): the
/// volume moves by [`VOLUME_WHEEL_STEP_PERCENT`] up for a positive `delta`
/// and down otherwise, clamped to 0..=100 %, and is persisted. Returns the
/// new volume in percent, for the volume overlay.
pub fn scroll_volume(ctx: &AppContext, delta: f32) -> f32 {
    let current = ctx.player.snapshot().volume * 100.0;
    let direction = if delta > 0.0 { 1.0 } else { -1.0 };
    let percent = (current + direction * VOLUME_WHEEL_STEP_PERCENT).clamp(0.0, 100.0);
    let volume = percent / 100.0;
    ctx.player.set_volume(volume);
    let _ = settings::write_player_volume(&ctx.db, volume);
    percent
}

// --- Track changes ------------------------------------------------------------

/// Plays the song `song_id` of the cursor's playlist (Iced's
/// `play_song_from_playlist`: double click, Enter, the context menu).
///
/// 1. A pending end of track is taken first, so the engine's duplicate-load
///    rule applies and the end of track is not handled again later.
/// 2. If the song is already playing, it restarts from the beginning, as in
///    Iced (the engine would ignore a load of the playing track).
/// 3. Otherwise it is loaded with its gains and played; it becomes the
///    playing song, and the shuffle session records it as played by hand.
///
/// The preload bookkeeping is reset and the playlist state persisted. A load
/// error leaves the cursor unchanged, as in Iced.
pub fn play_song(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    song_id: i64,
) -> Result<TrackChange, FrontendError> {
    let _ = ctx.player.take_eof();
    let song = cursor.song(song_id).ok_or_else(|| {
        FrontendError::InvalidInput(format!("song {song_id} is not in the playlist"))
    })?;
    let state = ctx.player.snapshot();
    if state.path == song.file_path.as_ref() && state.is_playing {
        ctx.player.seek(0.0);
        ctx.player.play();
        cursor.playing = Some(song_id);
        persist_after_change(ctx, cursor, false);
        return Ok(TrackChange::Restarted(song_id));
    }
    ctx.player
        .load_file(&track_request(song), cursor.gains(song_id))?;
    ctx.player.play();
    cursor.playing = Some(song_id);
    if cursor.shuffle
        && let Some(session) = cursor.session.as_mut()
    {
        nav::record_manual_play(session, song_id);
    }
    ctx.preload.mark_track_advanced(unix_now_secs());
    persist_after_change(ctx, cursor, false);
    Ok(TrackChange::Loaded(song_id))
}

/// Plays the row `linear_idx` of [`PlaybackCursor::active_groups`]: a song
/// row plays that song, a folder separator its first enabled song (Iced's
/// `PlaylistContextMenuPlay`). See [`play_song`].
pub fn play_item(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
) -> Result<TrackChange, FrontendError> {
    let groups = cursor.active_groups();
    let target = match nav::item_at(groups, linear_idx) {
        Some(nav::PlaylistItemType::Song(..)) => Some(linear_idx),
        Some(nav::PlaylistItemType::Separator(..)) => {
            nav::first_enabled_song_in_group(groups, linear_idx)
        }
        None => None,
    };
    match target
        .and_then(|idx| nav::song_at(groups, idx))
        .map(|song| song.song_id)
    {
        Some(song_id) => play_song(ctx, cursor, song_id),
        None => Ok(TrackChange::Unchanged),
    }
}

/// Plays the next song (Iced's `NextTrack`).
///
/// 1. A pending end of track is taken first. If the engine already promoted
///    the preloaded track, the cursor catches up with it, so Next moves on
///    from the track actually playing and the end of track is never handled
///    a second time (quirk fix 28).
/// 2. With the crossfade group and its manual crossfade enabled and a length
///    above 0, the next song is preloaded (with its gains) unless it already
///    is, and the engine crossfades into it.
/// 3. The cursor advances (`logic::playlist_nav::advance_next`: Repeat One,
///    shuffle, sequential order, re-shuffle) and the song is loaded and
///    played; after a crossfade the engine skips that load, since the song
///    already plays.
/// 4. The preload bookkeeping is reset, the playlist state persisted and, in
///    shuffle, the session saved.
///
/// At the end of the playlist (without Repeat All) the track does not change.
pub fn next(ctx: &AppContext, cursor: &mut PlaybackCursor) -> Result<TrackChange, FrontendError> {
    if ctx.player.take_eof() {
        settle_pending_end(ctx, cursor);
    }

    let state = ctx.player.snapshot();
    if state.crossfade_enabled
        && state.crossfade_manual_enabled
        && state.crossfade_manual_ms > 0.0
        && let Some(next) = cursor.next_song()
    {
        let path = next.file_path.to_string();
        if !ctx.preload.is_preloaded(&path) {
            if let Err(e) = ctx
                .player
                .preload_next(&track_request(next), cursor.gains(next.song_id))
            {
                tracing::warn!("the next track could not be preloaded for the crossfade: {e}");
            }
            ctx.preload.mark_preloaded(path);
        }
        if let Err(e) = ctx
            .player
            .crossfade_next(f64::from(state.crossfade_manual_ms))
        {
            tracing::warn!("the manual crossfade could not start: {e}");
        }
    }

    let target = cursor.advance();
    let skipped = target.is_some_and(|song_id| load_is_skipped(ctx, cursor, song_id));
    let loaded = target.map(|song_id| load_and_play(ctx, cursor, song_id));
    ctx.preload.mark_track_advanced(unix_now_secs());
    match target {
        Some(_) => persist_after_change(ctx, cursor, skipped),
        None => {
            let _ = playlist::persist_state(ctx, cursor);
        }
    }
    save_session_if_shuffling(ctx, cursor);
    loaded.transpose()?;
    Ok(target.map_or(TrackChange::Unchanged, TrackChange::Loaded))
}

/// Plays the previous song (Iced's `PreviousTrack`).
///
/// A pending end of track is settled first, as for [`next`]. The previous
/// song follows `logic::playlist_nav::step_prev_fixed`: in shuffle, the last
/// song of the history that is still in the playlist (quirk fix 23);
/// otherwise the previous enabled song, wrapping to the last one under Repeat
/// All (quirk fix 22). It is loaded and played; the preload bookkeeping is
/// reset, the playlist state persisted and, in shuffle, the session saved.
pub fn previous(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
) -> Result<TrackChange, FrontendError> {
    if ctx.player.take_eof() {
        settle_pending_end(ctx, cursor);
    }
    let target = cursor.step_back();
    let skipped = target.is_some_and(|song_id| load_is_skipped(ctx, cursor, song_id));
    let loaded = target.map(|song_id| load_and_play(ctx, cursor, song_id));
    match target {
        Some(_) => {
            ctx.preload.mark_track_advanced(unix_now_secs());
            persist_after_change(ctx, cursor, skipped);
        }
        None => {
            let _ = playlist::persist_state(ctx, cursor);
        }
    }
    save_session_if_shuffling(ctx, cursor);
    loaded.transpose()?;
    Ok(target.map_or(TrackChange::Unchanged, TrackChange::Loaded))
}

/// Handles the end of a track; the UI calls it on `Playback::EndOfTrack`
/// (Iced's `Tick` end-of-track path).
///
/// If [`take_eof`](crate::frontend::ports::PlayerPort::take_eof) returns
/// `false`, the end of track was already handled (the event repeated, or Next
/// settled it) and nothing happens. Otherwise the cursor advances
/// (`logic::playlist_nav::advance_next`) and the song is loaded and played:
/// when the engine promoted the preloaded track, that load is the one it
/// skips, since the end of track is already taken. The preload bookkeeping is
/// reset, the shuffle session saved in shuffle, and the playlist state
/// persisted.
pub fn end_of_track(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
) -> Result<EndOfTrackOutcome, FrontendError> {
    if !ctx.player.take_eof() {
        return Ok(EndOfTrackOutcome::NotPending);
    }
    let target = cursor.advance();
    let skipped = target.is_some_and(|song_id| load_is_skipped(ctx, cursor, song_id));
    let loaded = target.map(|song_id| load_and_play(ctx, cursor, song_id));
    ctx.preload.mark_track_advanced(unix_now_secs());
    save_session_if_shuffling(ctx, cursor);
    match target {
        Some(_) => persist_after_change(ctx, cursor, skipped),
        None => {
            let _ = playlist::persist_state(ctx, cursor);
        }
    }
    loaded.transpose()?;
    Ok(target.map_or(EndOfTrackOutcome::Stopped, EndOfTrackOutcome::Advanced))
}

/// Preloads the next song for a gapless transition (Iced's `Tick` preload
/// path); the UI calls it as the position advances.
///
/// While playing, when at most [`PRELOAD_LEAD_SECS`] remain of a track whose
/// duration is known, the song Next would play
/// ([`PlaybackCursor::next_song`]) is preloaded with its gains, once per
/// track: the preload bookkeeping records it. Returns that song's cover path
/// (whether or not it was just preloaded), so the UI can prefetch the cover;
/// `None` otherwise. A pending preload that is no longer that song is dropped
/// first ([`drop_stale_preload`]).
pub fn maybe_preload(
    ctx: &AppContext,
    cursor: &PlaybackCursor,
    position_secs: f64,
    duration_secs: f64,
) -> Option<String> {
    drop_stale_preload(ctx, cursor);
    if !ctx.player.is_playing() || duration_secs <= 0.0 {
        return None;
    }
    if duration_secs - position_secs > PRELOAD_LEAD_SECS {
        return None;
    }
    let next = cursor.next_song()?;
    let path = next.file_path.to_string();
    if !ctx.preload.is_preloaded(&path) {
        if let Err(e) = ctx
            .player
            .preload_next(&track_request(next), cursor.gains(next.song_id))
        {
            tracing::warn!("the next track could not be preloaded: {e}");
        }
        ctx.preload.mark_preloaded(path);
    }
    next.cover_path.as_deref().map(str::to_string)
}

// --- Repeat and shuffle -----------------------------------------------------------

/// Cycles the repeat mode Off → All → One → Off (Iced's `ToggleRepeat`),
/// persists it, and returns the new mode. A pending preload the new mode no
/// longer plays next is dropped ([`drop_stale_preload`]).
pub fn toggle_repeat(ctx: &AppContext, cursor: &mut PlaybackCursor) -> RepeatMode {
    cursor.repeat = cursor.repeat.cycled();
    drop_stale_preload(ctx, cursor);
    let _ = playlist::persist_state(ctx, cursor);
    cursor.repeat
}

/// Turns shuffle on or off (Iced's `ToggleShuffle`) and returns whether it is
/// on.
///
/// Turning it on draws a new session over the enabled songs of the displayed
/// list, with the playing song as its history, and saves it. With no enabled
/// song, shuffle stays off and the previous session is dropped, in memory and
/// in the database (quirk fix 24, Iced defect D16: Iced keeps it and saves it
/// again). Turning it off drops the session and clears it in the database.
/// The playlist state is persisted either way, and a pending preload that is
/// no longer the next song is dropped ([`drop_stale_preload`]).
pub fn toggle_shuffle(ctx: &AppContext, cursor: &mut PlaybackCursor) -> bool {
    if cursor.shuffle {
        cursor.shuffle = false;
        cursor.session = None;
    } else {
        let groups = cursor.filtered.as_deref().unwrap_or(&cursor.groups);
        let playing = PlaybackCursor::index_in(groups, cursor.playing);
        nav::activate_shuffle_fixed(
            groups,
            playing,
            &mut cursor.shuffle,
            &mut cursor.session,
            &mut fastrand::Rng::new(),
        );
    }
    drop_stale_preload(ctx, cursor);
    let _ = match &cursor.session {
        Some(session) => playlist::save_shuffle_session(ctx, cursor.playlist_id, session),
        None => playlist::clear_shuffle_session(ctx, cursor.playlist_id),
    };
    let _ = playlist::persist_state(ctx, cursor);
    cursor.shuffle
}

// --- System media controls ----------------------------------------------------

/// Carries out a request from the system media controls (Iced's
/// `ExternalControl`) and returns the action it mapped to
/// (`logic::transport::external_control_action`).
///
/// Play and Pause set the play state, Toggle flips it ([`set_playing`]), Stop
/// is [`stop`], Next and Previous are [`next`] and [`previous`], the seeks go
/// through [`seek`] (the fixed jumps are `SEEK_STEP_SECS`), and a volume is
/// [`set_volume`]. A volume request is also echoed back to the media controls,
/// as Iced does.
pub fn external_control(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    event: ExternalControlEvent,
) -> Result<TransportAction, FrontendError> {
    if let ExternalControlEvent::SetVolume(volume) = event {
        ctx.media.set_volume(f64::from(clamp_volume(volume)));
    }
    let state = ctx.player.snapshot();
    let action = external_control_action(&event);
    match action {
        TransportAction::Play | TransportAction::Pause | TransportAction::Toggle => {
            let playing = action.target_playing(state.is_playing).unwrap_or(false);
            set_playing(ctx, cursor, playing);
        }
        TransportAction::Stop => stop(ctx),
        TransportAction::Next => {
            next(ctx, cursor)?;
        }
        TransportAction::Previous => {
            previous(ctx, cursor)?;
        }
        TransportAction::SeekBy(_) | TransportAction::SeekTo(_) => {
            if let Some(target) =
                action.seek_destination(state.current_pos_sec, state.total_duration_sec)
            {
                seek(ctx, cursor, target);
            }
        }
        TransportAction::SetVolume(volume) => {
            set_volume(ctx, volume);
        }
    }
    Ok(action)
}

#[cfg(test)]
pub(crate) mod tests;
