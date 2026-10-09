//! Playlist commands: listing and loading playlists, switching the active
//! one, reloading its rows, creating, renaming and deleting playlists,
//! adding, sending and removing songs, the enabled flags, the persistence of
//! the playback state, and the export, M3U import and external-file long
//! operations.
//!
//! Every database access runs on the database worker or in a long operation.
//! Each command that changes a playlist publishes, once the change is
//! stored, [`PlaylistsEvent::ListChanged`] (a playlist was created, renamed
//! or deleted) or [`PlaylistsEvent::ContentChanged`] (the songs or flags of
//! one playlist changed).
//!
//! Loading a playlist and applying it are two steps, as in Iced
//! (`SwitchPlaylist`, then `InternalPlaylistLoaded`): the load runs on the
//! database worker, and the view-model applies the result to its
//! [`PlaybackCursor`] on its own thread ([`PlaylistSwitch::apply`],
//! [`PlaylistContent::apply`]).
//!
//! Teksilo-only changes, recorded in `docs/teksilo-port-quirk-fixes.md`:
//!
//! - a content change (add, import, send-to, remove) reloads only the rows
//!   ([`reload_content`]); Iced switches to the playlist again, which
//!   reloads, seeks and re-applies the play state of the current track
//!   (quirk fix 12);
//! - deleting a system playlist is refused with an error (the UI disables
//!   the entry, quirk fix 16); Iced silently does nothing;
//! - the failures of the export, the M3U import and the external-file batch
//!   come back as errors, so the UI can show them (quirk fix 8);
//! - an `.m3u` export is written in Latin-1 and an `.m3u8` export in UTF-8,
//!   with durations rounded to the second (quirk fix 25, Iced defect D19).

#![warn(missing_docs)]

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::playback::{PlaybackCursor, TrackChange, track_request};
use crate::db::Database;
use crate::db::database::{PlaylistData, PlaylistFolderGroup, PlaylistSongRef, ShuffleSession};
use crate::db::scanner::Scanner;
use crate::frontend::app_context::AppContext;
use crate::frontend::db_worker::DbWorker;
use crate::frontend::error::FrontendError;
use crate::frontend::events::{Event, LongOpKind, OpId, PlaylistsEvent};
use crate::frontend::long_ops::OpContext;
use crate::frontend::monitor::{queue_backend_purge, unix_now_secs};
use crate::frontend::ports::ReplayGains;
use crate::frontend::reply::Reply;
use crate::frontend::settings;
use crate::logic::playlist_export::{
    ExportFormat, ExportMode, FALLBACK_PLAYLIST_NAME, plan_export,
};
use crate::logic::playlist_nav::{self as nav, PlaylistItemType, RepeatMode};

// --- Loading ----------------------------------------------------------------------

/// A playlist as read from the database, ready to become a
/// [`PlaybackCursor`].
#[derive(Debug, Clone)]
pub struct LoadedPlaylist {
    /// The playlist row: its name, whether it is a system playlist, and its
    /// saved playback state (last song, position, play state, shuffle,
    /// repeat).
    pub data: PlaylistData,
    /// Its songs, grouped by folder, in playlist order.
    pub groups: Vec<PlaylistFolderGroup>,
    /// The ReplayGain values of its songs, by song id (songs without any are
    /// left out).
    pub gains: HashMap<i64, ReplayGains>,
    /// Its shuffle session, when shuffle is on and a session is saved. As in
    /// Iced, when the saved last song is in the shuffle order the position
    /// points just after it, and the song is in the history.
    pub session: Option<ShuffleSession>,
}

/// Every playlist, in tab order.
pub fn list_playlists(ctx: &AppContext) -> Reply<Vec<PlaylistData>> {
    ctx.db.read(|db| Ok(db.get_all_playlists()?))
}

/// Loads playlist `playlist_id`: its row, its songs grouped by folder, the
/// ReplayGain values of its songs and its shuffle session (see
/// [`LoadedPlaylist`]). An unknown id is [`FrontendError::InvalidInput`].
pub fn load_playlist(ctx: &AppContext, playlist_id: i64) -> Reply<LoadedPlaylist> {
    ctx.db.read(move |db| read_playlist(db, playlist_id))
}

/// Reads a playlist on the database worker; see [`load_playlist`].
pub(crate) fn read_playlist(
    db: &Database,
    playlist_id: i64,
) -> Result<LoadedPlaylist, FrontendError> {
    let data = db.get_playlist_by_id(playlist_id)?.ok_or_else(|| {
        FrontendError::InvalidInput(format!("there is no playlist {playlist_id}"))
    })?;
    let groups = db.get_playlist_songs_grouped_by_folder(playlist_id)?;
    let gains = read_gains(db, &groups);
    let session = read_session(db, &data);
    Ok(LoadedPlaylist {
        data,
        groups,
        gains,
        session,
    })
}

/// The gains of every song of `groups`, looked up by path as Iced does
/// (`get_replay_gain_by_path`). A failed lookup is logged and counts as no
/// gain, as in Iced.
fn read_gains(db: &Database, groups: &[PlaylistFolderGroup]) -> HashMap<i64, ReplayGains> {
    let mut gains = HashMap::new();
    for song in nav::flatten_groups(groups) {
        if gains.contains_key(&song.song_id) {
            continue;
        }
        match db.get_replay_gain_by_path(&song.file_path) {
            Ok((None, None)) => {}
            Ok((track, album)) => {
                gains.insert(song.song_id, ReplayGains { track, album });
            }
            Err(e) => {
                tracing::error!("Error consultando ReplayGain en BD: {e}");
            }
        }
    }
    gains
}

/// The saved shuffle session of a playlist with shuffle on, with Iced's
/// position rule (`SwitchPlaylist` and `AudoxidyApp::new`): when the last song
/// is in the order, the position points after it and it joins the history.
/// A failed read counts as no session, as in Iced.
fn read_session(db: &Database, data: &PlaylistData) -> Option<ShuffleSession> {
    if !data.shuffle_active {
        return None;
    }
    let mut session = match db.load_shuffle_session(data.id) {
        Ok(session) => session?,
        Err(e) => {
            tracing::warn!(
                playlist_id = data.id,
                "the shuffle session could not be read: {e}"
            );
            return None;
        }
    };
    if let Some(song_id) = data.last_song_id
        && let Some(position) = session.shuffle_order.iter().position(|&id| id == song_id)
    {
        session.current_position = position + 1;
        if !session.history.contains(&song_id) {
            session.history.push(song_id);
        }
    }
    Some(session)
}

// --- Switching and reloading ---------------------------------------------------

/// A playlist loaded by [`switch_playlist`], to apply to the cursor.
#[derive(Debug, Clone)]
pub struct PlaylistSwitch {
    loaded: LoadedPlaylist,
    auto_play: bool,
}

impl PlaylistSwitch {
    /// The id of the playlist switched to. A view-model that switched again
    /// meanwhile drops this switch instead of applying it (as Iced ignores a
    /// load for a playlist that is no longer the active one).
    pub fn playlist_id(&self) -> i64 {
        self.loaded.data.id
    }

    /// Makes the loaded playlist the cursor's and puts its song in the player
    /// (Iced's `InternalPlaylistLoaded`).
    ///
    /// The cursor takes the playlist's songs, gains, repeat mode, shuffle flag
    /// and session; its search is kept and applied to the new rows. The song
    /// is the first enabled song with `auto_play`, otherwise the playlist's
    /// saved last song; both are looked up in the whole playlist, whatever
    /// the search. When there is one, a pending end of track is taken, the
    /// song is loaded with its gains, seeked to the saved position (0 with
    /// `auto_play`), and played or paused as saved (played with `auto_play`).
    /// When there is none, nothing plays in the cursor and the player is left
    /// as it is, as in Iced. A load error is returned after the seek and the
    /// play state are applied, as Iced ignores it.
    pub fn apply(
        self,
        ctx: &AppContext,
        cursor: &mut PlaybackCursor,
    ) -> Result<TrackChange, FrontendError> {
        let Self { loaded, auto_play } = self;
        let saved = loaded.data.clone();
        let search = std::mem::take(&mut cursor.search);
        *cursor = PlaybackCursor::new(loaded);
        cursor.set_search(&search);

        let song_id = if auto_play {
            nav::find_next_enabled(&cursor.groups, None, cursor.repeat == RepeatMode::All)
                .and_then(|idx| nav::song_at(&cursor.groups, idx))
                .map(|song| song.song_id)
        } else {
            saved.last_song_id
        };
        let Some((song_id, track)) =
            song_id.and_then(|id| cursor.song(id).map(|song| (id, track_request(song))))
        else {
            return Ok(TrackChange::Unchanged);
        };
        let (position, playing) = if auto_play {
            (0.0, true)
        } else {
            (saved.last_pos_sec, saved.is_playing)
        };

        let _ = ctx.player.take_eof();
        let loaded = ctx.player.load_file(&track, cursor.gains(song_id));
        ctx.player.seek(position);
        ctx.player.set_playing(playing);
        cursor.playing = Some(song_id);
        ctx.preload.mark_track_advanced(unix_now_secs());
        loaded?;
        Ok(TrackChange::Loaded(song_id))
    }
}

/// Starts switching to playlist `playlist_id` (Iced's `SwitchPlaylist`).
///
/// The state of the current playlist is persisted and `playlist_id` is saved
/// as `last_active_playlist_id`, then the new playlist is loaded on the
/// database worker. Apply the reply with [`PlaylistSwitch::apply`].
pub fn switch_playlist(
    ctx: &AppContext,
    cursor: &PlaybackCursor,
    playlist_id: i64,
    auto_play: bool,
) -> Reply<PlaylistSwitch> {
    let _ = persist_state(ctx, cursor);
    let _ = settings::write_last_active_playlist_id(&ctx.db, playlist_id);
    ctx.db.read(move |db| {
        Ok(PlaylistSwitch {
            loaded: read_playlist(db, playlist_id)?,
            auto_play,
        })
    })
}

/// The rows of a playlist, reloaded by [`reload_content`].
#[derive(Debug, Clone)]
pub struct PlaylistContent {
    playlist_id: i64,
    groups: Vec<PlaylistFolderGroup>,
    gains: HashMap<i64, ReplayGains>,
}

impl PlaylistContent {
    /// The playlist the rows belong to.
    pub fn playlist_id(&self) -> i64 {
        self.playlist_id
    }

    /// Replaces the cursor's songs and gains with these (its search is
    /// applied again) and returns `true`; returns `false` without a change
    /// when the cursor shows another playlist by now.
    ///
    /// Playback is not touched: no load, no seek, no play-state change, and
    /// the playing song, repeat, shuffle and session stay (quirk fix 12).
    pub fn apply(self, cursor: &mut PlaybackCursor) -> bool {
        if cursor.playlist_id != self.playlist_id {
            return false;
        }
        cursor.replace_content(self.groups, self.gains);
        true
    }
}

/// Reloads the rows of the cursor's playlist after its content changed (add,
/// import, send-to, remove: the [`PlaylistsEvent::ContentChanged`] of the
/// active playlist). Apply the reply with [`PlaylistContent::apply`].
pub fn reload_content(ctx: &AppContext, cursor: &PlaybackCursor) -> Reply<PlaylistContent> {
    let playlist_id = cursor.playlist_id;
    ctx.db.read(move |db| {
        let groups = db.get_playlist_songs_grouped_by_folder(playlist_id)?;
        let gains = read_gains(db, &groups);
        Ok(PlaylistContent {
            playlist_id,
            groups,
            gains,
        })
    })
}

// --- Creating, renaming, deleting ------------------------------------------------

/// What [`rename_playlist`] did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameOutcome {
    /// The playlist was renamed.
    Renamed(i64),
    /// The playlist is a system playlist, which keeps its name: a copy with
    /// the new name and every song of the original was created instead, as
    /// in Iced. The UI switches to it.
    Copied(i64),
}

/// Creates a playlist named `name` holding `song_ids` (Iced's "Nueva lista"
/// dialog, also used to send songs to a new list) and returns its id.
///
/// A blank name is refused with [`FrontendError::InvalidInput`]: the UI puts
/// its localised default in the field (Iced's `"Nueva lista {n}"`, `n` being
/// the number of playlists plus one). A name already taken fails, as the
/// database requires unique names. As in Iced, a failure to add the songs is
/// logged and the playlist is kept. Publishes `ListChanged`; the UI then
/// switches to the new playlist.
pub fn create_playlist(ctx: &AppContext, name: &str, song_ids: Vec<i64>) -> Reply<i64> {
    if name.trim().is_empty() {
        return Reply::ready(Err(FrontendError::InvalidInput(
            "a playlist needs a name".to_string(),
        )));
    }
    let name = name.to_string();
    let hub = ctx.hub.clone();
    ctx.db.write(move |db| {
        let playlist_id = db.create_playlist(&name, false)?;
        add_logged(db, playlist_id, &song_ids);
        hub.publish(Event::Playlists(PlaylistsEvent::ListChanged));
        Ok(playlist_id)
    })
}

/// Adds `song_ids` to a playlist, logging a failure as Iced does.
fn add_logged(db: &mut Database, playlist_id: i64, song_ids: &[i64]) {
    if song_ids.is_empty() {
        return;
    }
    if let Err(e) = db.add_songs_to_playlist(playlist_id, song_ids) {
        tracing::warn!(
            target: "audoxidy::persistence",
            playlist_id,
            count = song_ids.len(),
            error = %e,
            "the songs could not be added to the playlist"
        );
    }
}

/// Renames a playlist (Iced's rename dialog).
///
/// A system playlist keeps its name: a new playlist named `new_name`
/// (trimmed) with all its songs is created instead, as in Iced
/// ([`RenameOutcome::Copied`]). A blank name or an unknown playlist is
/// [`FrontendError::InvalidInput`]. Publishes `ListChanged`.
pub fn rename_playlist(ctx: &AppContext, playlist_id: i64, new_name: &str) -> Reply<RenameOutcome> {
    if new_name.trim().is_empty() {
        return Reply::ready(Err(FrontendError::InvalidInput(
            "a playlist needs a name".to_string(),
        )));
    }
    let new_name = new_name.to_string();
    let hub = ctx.hub.clone();
    ctx.db.write(move |db| {
        let data = db.get_playlist_by_id(playlist_id)?.ok_or_else(|| {
            FrontendError::InvalidInput(format!("there is no playlist {playlist_id}"))
        })?;
        let outcome = if data.is_system {
            let copy_id = db.create_playlist(new_name.trim(), false)?;
            match db.get_playlist_all_song_ids(playlist_id) {
                Ok(song_ids) => add_logged(db, copy_id, &song_ids),
                Err(e) => tracing::warn!(
                    target: "audoxidy::persistence",
                    playlist_id,
                    error = %e,
                    "the songs of the system playlist could not be read for its copy"
                ),
            }
            RenameOutcome::Copied(copy_id)
        } else {
            db.rename_playlist(playlist_id, &new_name)?;
            RenameOutcome::Renamed(playlist_id)
        };
        hub.publish(Event::Playlists(PlaylistsEvent::ListChanged));
        Ok(outcome)
    })
}

/// Deletes a playlist (Iced's delete confirmation). A system playlist is
/// refused with [`FrontendError::InvalidInput`] and the UI disables the
/// entry (quirk fix 16); an unknown playlist is refused too. Publishes
/// `ListChanged`. When the active playlist is deleted, the UI switches to the
/// first one, as Iced does.
pub fn delete_playlist(ctx: &AppContext, playlist_id: i64) -> Reply<()> {
    let hub = ctx.hub.clone();
    ctx.db.write(move |db| {
        let data = db.get_playlist_by_id(playlist_id)?.ok_or_else(|| {
            FrontendError::InvalidInput(format!("there is no playlist {playlist_id}"))
        })?;
        if data.is_system {
            return Err(FrontendError::InvalidInput(format!(
                "the system playlist \"{}\" cannot be deleted",
                data.name
            )));
        }
        db.delete_playlist(playlist_id)?;
        hub.publish(Event::Playlists(PlaylistsEvent::ListChanged));
        Ok(())
    })
}

// --- Songs --------------------------------------------------------------------

/// Adds `song_ids` to playlist `playlist_id` (Iced's `AddSongToPlaylist`,
/// `PlayAlbum` and send-to) and returns how many were added (songs already
/// in it are skipped). Publishes `ContentChanged` for that playlist, unless
/// `song_ids` is empty.
pub fn add_songs(ctx: &AppContext, playlist_id: i64, song_ids: Vec<i64>) -> Reply<usize> {
    if song_ids.is_empty() {
        return Reply::ready(Ok(0));
    }
    let hub = ctx.hub.clone();
    ctx.db.write(move |db| {
        let added = db.add_songs_to_playlist(playlist_id, &song_ids)?;
        hub.publish(Event::Playlists(PlaylistsEvent::ContentChanged {
            playlist_id,
        }));
        Ok(added)
    })
}

/// Sends the songs targeted by a click on the row `clicked` (the selection
/// when `clicked` is in it; see [`PlaybackCursor::action_targets`]) to
/// playlist `target_playlist_id` (Iced's `PlaylistSendToList`). For a new
/// list, pass the same targets to [`create_playlist`].
pub fn send_to(
    ctx: &AppContext,
    cursor: &PlaybackCursor,
    selected: &HashSet<usize>,
    clicked: usize,
    target_playlist_id: i64,
) -> Reply<usize> {
    add_songs(
        ctx,
        target_playlist_id,
        cursor.action_targets(selected, clicked),
    )
}

/// Removes the songs targeted by a click on the row `clicked` (see
/// [`PlaybackCursor::action_targets`]) from the cursor's playlist (Iced's
/// `PlaylistDeleteSelection`) and returns how many were targeted. Publishes
/// `ContentChanged`; the UI reloads the rows with [`reload_content`]. As in
/// Iced, the backend memory purge runs afterwards.
pub fn remove_songs(
    ctx: &AppContext,
    cursor: &PlaybackCursor,
    selected: &HashSet<usize>,
    clicked: usize,
) -> Reply<usize> {
    let song_ids = cursor.action_targets(selected, clicked);
    if song_ids.is_empty() {
        return Reply::ready(Ok(0));
    }
    let playlist_id = cursor.playlist_id;
    let hub = ctx.hub.clone();
    let removed = ctx.db.write(move |db| {
        db.batch_remove_songs_from_playlist(playlist_id, &song_ids)?;
        hub.publish(Event::Playlists(PlaylistsEvent::ContentChanged {
            playlist_id,
        }));
        Ok(song_ids.len())
    });
    let _ = queue_backend_purge(&ctx.engine, &ctx.preload);
    removed
}

/// Empties the cursor's playlist (Iced's `ClearPlaylist`): the database rows,
/// the cursor's rows and playing song, then playback stops, the state is
/// persisted and the backend memory purge runs. Publishes `ContentChanged`.
pub fn clear_playlist(ctx: &AppContext, cursor: &mut PlaybackCursor) -> Reply<()> {
    let playlist_id = cursor.playlist_id;
    let hub = ctx.hub.clone();
    let cleared = ctx.db.write(move |db| {
        db.clear_playlist(playlist_id)?;
        hub.publish(Event::Playlists(PlaylistsEvent::ContentChanged {
            playlist_id,
        }));
        Ok(())
    });
    cursor.replace_content(Vec::new(), HashMap::new());
    cursor.playing = None;
    ctx.player.stop();
    let _ = persist_state(ctx, cursor);
    let _ = queue_backend_purge(&ctx.engine, &ctx.preload);
    cleared
}

// --- Enabled flags ---------------------------------------------------------------

/// One enabled-flag change to store.
enum FlagWrite {
    Song(i64, bool),
    Folder(String, bool),
}

/// Stores flag changes in one job and publishes `ContentChanged`. Failures
/// are logged, as in Iced; the first one is returned.
fn store_flags(ctx: &AppContext, playlist_id: i64, writes: Vec<FlagWrite>) -> Reply<()> {
    if writes.is_empty() {
        return Reply::ready(Ok(()));
    }
    let hub = ctx.hub.clone();
    ctx.db.write(move |db| {
        let mut first_error = None;
        for write in writes {
            let result = match &write {
                FlagWrite::Song(song_id, enabled) => {
                    db.set_song_enabled_in_playlist(playlist_id, *song_id, *enabled)
                }
                FlagWrite::Folder(path, enabled) => {
                    db.set_folder_enabled_in_playlist(playlist_id, path, *enabled)
                }
            };
            if let Err(e) = result {
                tracing::warn!(
                    target: "audoxidy::persistence",
                    playlist_id,
                    error = %e,
                    "the enabled flag could not be stored"
                );
                first_error.get_or_insert(e);
            }
        }
        hub.publish(Event::Playlists(PlaylistsEvent::ContentChanged {
            playlist_id,
        }));
        first_error.map_or(Ok(()), |e| Err(e.into()))
    })
}

/// Sets the enabled flag of the song row `linear_idx` in the cursor (the
/// displayed rows; with a search, mirrored into the whole playlist by song
/// id) and returns the flag write to store.
fn set_song_flag(
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
    enabled: bool,
) -> Option<FlagWrite> {
    let song_id = cursor.song_at(linear_idx)?.song_id;
    nav::set_song_enabled(
        &mut cursor.groups,
        cursor.filtered.as_deref_mut(),
        linear_idx,
        enabled,
    );
    Some(FlagWrite::Song(song_id, enabled))
}

/// Sets the enabled flag of every song of the group whose separator is the
/// row `linear_idx` (with a search, mirrored into the whole group) and
/// returns the flag write to store: the whole folder, as Iced stores it.
fn set_group_flag(
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
    enabled: bool,
) -> Option<FlagWrite> {
    let Some(PlaylistItemType::Separator(group_idx, _)) =
        nav::item_at(cursor.active_groups(), linear_idx)
    else {
        return None;
    };
    let folder = cursor
        .active_groups()
        .get(group_idx)?
        .folder_path
        .to_string();
    nav::set_group_enabled(
        &mut cursor.groups,
        cursor.filtered.as_deref_mut(),
        linear_idx,
        enabled,
    );
    Some(FlagWrite::Folder(folder, enabled))
}

/// Sets the enabled flag of the song row `linear_idx` (Iced's
/// `ToggleSongEnabled` with a forced state) and stores it. A row that is not
/// a song changes nothing.
pub fn set_song_enabled(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
    enabled: bool,
) -> Reply<()> {
    let writes = set_song_flag(cursor, linear_idx, enabled)
        .into_iter()
        .collect();
    store_flags(ctx, cursor.playlist_id, writes)
}

/// Flips the enabled flag of the song row `linear_idx` (Iced's
/// `ToggleSongEnabled`), stores it, and returns the new flag; `None` for a
/// row that is not a song.
pub fn toggle_song_enabled(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
) -> Option<bool> {
    let enabled = !cursor.song_at(linear_idx)?.enabled;
    let _ = set_song_enabled(ctx, cursor, linear_idx, enabled);
    Some(enabled)
}

/// Sets the enabled flag of every song of the group whose separator is the
/// row `linear_idx` (Iced's `ToggleGroupEnabled` with a forced state) and
/// stores it for the whole folder. A row that is not a separator changes
/// nothing.
pub fn set_group_enabled(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
    enabled: bool,
) -> Reply<()> {
    let writes = set_group_flag(cursor, linear_idx, enabled)
        .into_iter()
        .collect();
    store_flags(ctx, cursor.playlist_id, writes)
}

/// Toggles the group whose separator is the row `linear_idx` (Iced's
/// `ToggleGroupEnabled`): it becomes enabled when every displayed song of the
/// group is disabled, otherwise disabled. Stores it and returns the new flag;
/// `None` for a row that is not a separator.
pub fn toggle_group_enabled(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    linear_idx: usize,
) -> Option<bool> {
    let groups = cursor.active_groups();
    let Some(PlaylistItemType::Separator(group_idx, _)) = nav::item_at(groups, linear_idx) else {
        return None;
    };
    let enabled = !groups.get(group_idx)?.songs.iter().any(|song| song.enabled);
    let _ = set_group_enabled(ctx, cursor, linear_idx, enabled);
    Some(enabled)
}

/// The enable toggle on the row `clicked` (Iced's `PlaylistToggleEnabled`,
/// the Space key and the context menu): it applies to the selection when
/// `clicked` is in it, otherwise to `clicked` alone. Every targeted song and
/// group becomes enabled if any of them has a disabled song, otherwise
/// disabled (`logic::playlist_nav::bulk_enable_target`). The flags are stored
/// in one job; returns the flag applied.
pub fn toggle_enabled(
    ctx: &AppContext,
    cursor: &mut PlaybackCursor,
    selected: &HashSet<usize>,
    clicked: usize,
) -> bool {
    let mut targets: Vec<usize> = if selected.contains(&clicked) {
        selected.iter().copied().collect()
    } else {
        vec![clicked]
    };
    targets.sort_unstable();
    let enabled = nav::bulk_enable_target(
        &cursor.groups,
        cursor.active_groups(),
        targets.iter().copied(),
    );
    let mut writes = Vec::new();
    for idx in targets {
        let write = match nav::item_at(cursor.active_groups(), idx) {
            Some(PlaylistItemType::Song(..)) => set_song_flag(cursor, idx, enabled),
            Some(PlaylistItemType::Separator(..)) => set_group_flag(cursor, idx, enabled),
            None => None,
        };
        writes.extend(write);
    }
    let _ = store_flags(ctx, cursor.playlist_id, writes);
    enabled
}

// --- Persistence ------------------------------------------------------------------

/// The coalescing key of a playlist's persisted state.
fn persistence_key(playlist_id: i64) -> String {
    format!("playlist_persistence:{playlist_id}")
}

/// A reply that resolves once `reply`'s job has run, counting a job replaced
/// by a newer one with the same key ([`FrontendError::Superseded`]) as a
/// success.
fn settled(db: &DbWorker, reply: Reply<()>) -> Reply<()> {
    db.read(move |_| match reply.try_recv() {
        Some(Err(FrontendError::Superseded)) | Some(Ok(())) | None => Ok(()),
        Some(Err(e)) => Err(e),
    })
}

/// One reply for several: resolves once every job behind `replies` has run,
/// with the first error.
pub(crate) fn all_settled(db: &DbWorker, replies: Vec<Reply<()>>) -> Reply<()> {
    db.read(move |_| {
        for reply in replies {
            if let Some(Err(e)) = reply.try_recv() {
                return Err(e);
            }
        }
        Ok(())
    })
}

/// Persists the cursor's playback state for its playlist (Iced's
/// `persist_playlist_state`): the playing song, the player's current position
/// and play state, shuffle, repeat, and the shuffle position and session id.
///
/// The player is read now; the write is queued and coalesced per playlist
/// (the last state wins). The reply resolves once it is stored. The UI calls
/// it on `Session::AutosaveDue` (quirk fix 13); the playback commands call it
/// where Iced does.
pub fn persist_state(ctx: &AppContext, cursor: &PlaybackCursor) -> Reply<()> {
    let state = ctx.player.snapshot();
    persist_state_at(ctx, cursor, state.current_pos_sec, state.is_playing)
}

/// [`persist_state`] with an explicit position and play state.
pub(crate) fn persist_state_at(
    ctx: &AppContext,
    cursor: &PlaybackCursor,
    position_secs: f64,
    is_playing: bool,
) -> Reply<()> {
    let playlist_id = cursor.playlist_id;
    let song_id = cursor.playing;
    let shuffle = cursor.shuffle;
    let repeat = i32::from(cursor.repeat.as_u8());
    let (shuffle_pos, shuffle_id) = cursor.session.as_ref().map_or((0, None), |session| {
        (session.current_position, Some(session.session_id.clone()))
    });
    let write = ctx
        .db
        .write_coalesced(persistence_key(playlist_id), move |db| {
            db.update_playlist_persistence(
                playlist_id,
                song_id,
                position_secs,
                is_playing,
                shuffle,
                repeat,
                shuffle_pos,
                shuffle_id,
            )
            .map_err(|e| {
                tracing::warn!(
                    target: "audoxidy::persistence",
                    playlist_id,
                    error = %e,
                    "the playlist state could not be persisted"
                );
                FrontendError::from(e)
            })
        });
    settled(&ctx.db, write)
}

/// Saves the shuffle session of `playlist_id` (its order and history).
pub fn save_shuffle_session(
    ctx: &AppContext,
    playlist_id: i64,
    session: &ShuffleSession,
) -> Reply<()> {
    let session = session.clone();
    ctx.db.write(move |db| {
        db.save_shuffle_session(playlist_id, &session).map_err(|e| {
            tracing::warn!(
                target: "audoxidy::persistence",
                playlist_id,
                error = %e,
                "the shuffle session could not be saved"
            );
            FrontendError::from(e)
        })
    })
}

/// Clears the saved shuffle session of `playlist_id`.
pub fn clear_shuffle_session(ctx: &AppContext, playlist_id: i64) -> Reply<()> {
    ctx.db.write(move |db| {
        db.clear_shuffle_session(playlist_id).map_err(|e| {
            tracing::warn!(
                target: "audoxidy::persistence",
                playlist_id,
                error = %e,
                "the shuffle session could not be cleared"
            );
            FrontendError::from(e)
        })
    })
}

// --- Export -----------------------------------------------------------------------

/// A portable-export copy that failed; the export goes on without it, as in
/// Iced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedCopy {
    /// The song's file.
    pub source: String,
    /// Where it was to be copied.
    pub target: PathBuf,
    /// Why it failed.
    pub error: String,
}

/// What [`export`] wrote; taken with
/// `ctx.ops.take_result::<ExportReport>(op_id)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReport {
    /// The playlist file written.
    pub playlist_path: PathBuf,
    /// The number of entries in it.
    pub entries: usize,
    /// The number of song files copied (portable export).
    pub copied: usize,
    /// The copies that failed (portable export). The UI can warn about them.
    pub failed_copies: Vec<FailedCopy>,
}

/// Exports playlist `playlist_id` as a long operation
/// ([`LongOpKind::ExportPlaylist`]; Iced's `ExportPlaylist`).
///
/// The songs are read on the database worker; the plan is
/// `logic::playlist_export::plan_export` (portable copies de-duplicated,
/// Iced defect D3). Teksilo-only (quirk fix 25, Iced defect D19):
///
/// - durations are rounded to the nearest second, not truncated;
/// - an [`ExportFormat::M3U`] file is written in Latin-1 (ISO-8859-1) and an
///   [`ExportFormat::M3U8`] file in UTF-8. When an entry has a character
///   Latin-1 cannot store, the export fails before writing or copying
///   anything, with a [`FrontendError::InvalidInput`] naming the entry and
///   suggesting M3U8: a lossy file is never written.
///
/// A portable export copies each song (a failed copy is logged and reported
/// in [`ExportReport::failed_copies`], and the export goes on, as in Iced),
/// then writes the list. Failing to write the list fails the operation (quirk
/// fix 8: Iced only logs it). The operation can be cancelled between copies.
pub fn export(
    ctx: &AppContext,
    playlist_id: i64,
    path: PathBuf,
    format: ExportFormat,
    mode: ExportMode,
) -> OpId {
    let db = ctx.db.clone();
    ctx.ops.start(LongOpKind::ExportPlaylist, move |op| {
        op.check_cancelled()?;
        let (songs, name) = db
            .read(move |db| {
                let songs = db.search_playlist_songs(playlist_id, "")?;
                let name = db
                    .get_playlist_by_id(playlist_id)?
                    .map_or_else(|| FALLBACK_PLAYLIST_NAME.to_string(), |p| p.name);
                Ok((songs, name))
            })
            .recv_blocking()?;
        write_export(op, &songs, &name, &path, format, mode)
    })
}

/// A duration as an `#EXTINF` entry shows it: rounded to the nearest second
/// (quirk fix 25), never negative.
fn rounded_duration(secs: f64) -> f64 {
    if secs.is_finite() {
        secs.max(0.0).round()
    } else {
        0.0
    }
}

/// Plans and writes an export; see [`export`].
fn write_export(
    op: &OpContext,
    songs: &[PlaylistSongRef],
    name: &str,
    target: &Path,
    format: ExportFormat,
    mode: ExportMode,
) -> Result<ExportReport, FrontendError> {
    let songs: Vec<PlaylistSongRef> = songs
        .iter()
        .map(|song| PlaylistSongRef {
            duration: rounded_duration(song.duration),
            ..song.clone()
        })
        .collect();
    let plan = plan_export(&songs, name, target, format, mode);
    let bytes = encode_playlist(&plan.content, format, &songs)?;

    let steps = plan.copies.len() + 1;
    let mut copied = 0;
    let mut failed_copies = Vec::new();
    for (done, copy) in plan.copies.iter().enumerate() {
        op.check_cancelled()?;
        if let Err(e) = std::fs::create_dir_all(&copy.target_dir) {
            tracing::warn!(
                "No se pudo crear el directorio {:?}: {}",
                copy.target_dir,
                e
            );
        }
        match std::fs::copy(&copy.source, &copy.target_file) {
            Ok(_) => copied += 1,
            Err(e) => {
                tracing::warn!(
                    "No se pudo copiar {:?} a {:?}: {}",
                    copy.source,
                    copy.target_file,
                    e
                );
                failed_copies.push(FailedCopy {
                    source: copy.source.clone(),
                    target: copy.target_file.clone(),
                    error: e.to_string(),
                });
            }
        }
        op.report_progress((done + 1) as f32 / steps as f32);
    }
    op.check_cancelled()?;
    std::fs::write(&plan.playlist_path, bytes).map_err(|e| {
        tracing::warn!(
            "No se pudo escribir la lista M3U en {:?}: {}",
            plan.playlist_path,
            e
        );
        FrontendError::Io(e)
    })?;
    Ok(ExportReport {
        playlist_path: plan.playlist_path,
        entries: songs.len(),
        copied,
        failed_copies,
    })
}

/// The bytes of the list: UTF-8 for M3U8, Latin-1 for M3U. A character
/// outside Latin-1 fails the export, naming the entry it is in (`songs` in
/// plan order, one entry per song).
fn encode_playlist(
    content: &str,
    format: ExportFormat,
    songs: &[PlaylistSongRef],
) -> Result<Vec<u8>, FrontendError> {
    if format == ExportFormat::M3U8 {
        return Ok(content.as_bytes().to_vec());
    }
    let mut bytes = Vec::with_capacity(content.len());
    for (offset, c) in content.char_indices() {
        match u8::try_from(u32::from(c)) {
            Ok(byte) => bytes.push(byte),
            Err(_) => {
                // Every entry starts on a new line with "#EXTINF:".
                let entry = content[..offset].matches("\n#EXTINF:").count();
                let what = entry
                    .checked_sub(1)
                    .and_then(|index| songs.get(index))
                    .map_or_else(
                        || "the playlist header".to_string(),
                        |song| format!("the entry \"{}\" ({})", song.title, song.file_path),
                    );
                return Err(FrontendError::InvalidInput(format!(
                    "{what} contains '{c}', which an M3U playlist (Latin-1) cannot store; \
                     export the playlist as M3U8 instead"
                )));
            }
        }
    }
    Ok(bytes)
}

// --- Import and external files --------------------------------------------------

/// Imports an M3U/M3U8 file as a new playlist, as a long operation
/// ([`LongOpKind::ImportPlaylist`]; Iced's `ImportPlaylistFile`), and returns
/// the new playlist's id (`ctx.ops.take_result::<i64>(op_id)`).
///
/// It runs `Scanner::import_m3u` on the database connection: the playlist is
/// named after the file, and listed files missing from the library are added
/// as external songs. A file that cannot be read fails with
/// [`FrontendError::Io`]; a list with no existing file, or whose name is
/// already a playlist's, fails with [`FrontendError::InvalidInput`] (quirk
/// fix 8: Iced does nothing). Publishes `ListChanged`; the UI then switches
/// to the new playlist with `auto_play`, as Iced does.
pub fn import_m3u(ctx: &AppContext, path: PathBuf) -> OpId {
    let connection = ctx.db.connection();
    let hub = ctx.hub.clone();
    ctx.ops.start(LongOpKind::ImportPlaylist, move |op| {
        op.check_cancelled()?;
        std::fs::File::open(&path)?;
        let path_str = path.to_string_lossy().into_owned();
        let playlist_id = Scanner::import_m3u(&connection, &path_str).ok_or_else(|| {
            FrontendError::InvalidInput(format!(
                "{path_str} could not be imported: it lists no existing file, or a playlist \
                 with its name already exists"
            ))
        })?;
        hub.publish(Event::Playlists(PlaylistsEvent::ListChanged));
        Ok(playlist_id)
    })
}

/// Whether `path` is an M3U/M3U8 file, which the external batch imports as a
/// new playlist.
fn is_playlist_file(path: &Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("m3u") || e.eq_ignore_ascii_case("m3u8"))
}

/// Adds files and folders to playlist `playlist_id` as a long operation
/// ([`LongOpKind::ExternalBatch`]; Iced's `PlaylistProcessExternalFiles`, from
/// "add files", "add folder" and drops).
///
/// It runs `Scanner::process_external_batch` on the database connection:
/// audio files are read and added (folders recursively), and M3U/M3U8 files
/// are imported as new playlists. Publishes `ContentChanged` for the playlist,
/// and `ListChanged` when a playlist file was among `paths`.
pub fn add_external(ctx: &AppContext, paths: Vec<PathBuf>, playlist_id: i64) -> OpId {
    let connection = ctx.db.connection();
    let hub = ctx.hub.clone();
    ctx.ops.start(LongOpKind::ExternalBatch, move |op| {
        op.check_cancelled()?;
        let imports_lists = paths.iter().any(|path| is_playlist_file(path));
        Scanner::process_external_batch(&connection, paths, playlist_id);
        hub.publish(Event::Playlists(PlaylistsEvent::ContentChanged {
            playlist_id,
        }));
        if imports_lists {
            hub.publish(Event::Playlists(PlaylistsEvent::ListChanged));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
