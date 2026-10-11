//! `SingleNowPlaying`: the track that is playing.
//!
//! Signals for the current track (title, artist, album, cover path, file
//! path), the play state, the position and duration, and the engine's
//! non-modal notice. It starts from the player's state at construction
//! (`ctx.player.snapshot()`), so a widget built after playback began shows the
//! right track, and follows the playback monitor's events:
//!
//! - [`PlaybackEvent::TrackChanged`]: the track, its duration, the play state;
//! - [`PlaybackEvent::StateChanged`]: the play state;
//! - [`PlaybackEvent::PositionChanged`]: the position and the duration;
//! - [`PlaybackEvent::NoticeChanged`]: the notice.
//!
//! A signal is only written when its value changed, so an event that repeats
//! what the widgets show costs them nothing.

use std::rc::Rc;

use teksilo::prelude::{BuildContext, Signal};

use crate::frontend::events::{PlaybackEvent, PlaybackSnapshot};
use crate::frontend::{AppContext, Event, Origin};

struct Inner {
    title: Signal<String>,
    artist: Signal<String>,
    album: Signal<String>,
    cover_path: Signal<Option<String>>,
    path: Signal<String>,
    is_playing: Signal<bool>,
    position_secs: Signal<f64>,
    duration_secs: Signal<f64>,
    notice: Signal<Option<String>>,
}

/// The track that is playing. UI thread only; clones share one state.
#[derive(Clone)]
pub struct SingleNowPlaying {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for SingleNowPlaying {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SingleNowPlaying")
            .field("title", &self.inner.title.get())
            .field("is_playing", &self.inner.is_playing.get())
            .finish_non_exhaustive()
    }
}

impl SingleNowPlaying {
    /// A single initialised from the player's current state.
    pub fn new(ctx: &AppContext) -> Self {
        let state = PlaybackSnapshot::from_state(&ctx.player.snapshot());
        let single = Self {
            inner: Rc::new(Inner {
                title: Signal::new(String::new()),
                artist: Signal::new(String::new()),
                album: Signal::new(String::new()),
                cover_path: Signal::new(None),
                path: Signal::new(String::new()),
                is_playing: Signal::new(false),
                position_secs: Signal::new(0.0),
                duration_secs: Signal::new(0.0),
                notice: Signal::new(None),
            }),
        };
        single.apply_snapshot(&state);
        single
    }

    /// Subscribes the single to the playback events. Call it from a
    /// long-lived widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let single = self.clone();
        ctx.subscribe_event(Origin::Playback, move |event: &Event| {
            single.on_event(event);
        });
    }

    /// The title of the current track (empty when nothing is loaded).
    pub fn title(&self) -> Signal<String> {
        self.inner.title.clone()
    }

    /// The artist of the current track.
    pub fn artist(&self) -> Signal<String> {
        self.inner.artist.clone()
    }

    /// The album of the current track.
    pub fn album(&self) -> Signal<String> {
        self.inner.album.clone()
    }

    /// The cached cover of the current track, if any.
    pub fn cover_path(&self) -> Signal<Option<String>> {
        self.inner.cover_path.clone()
    }

    /// The file path of the current track (empty when nothing is loaded).
    pub fn path(&self) -> Signal<String> {
        self.inner.path.clone()
    }

    /// Whether the engine is playing.
    pub fn is_playing(&self) -> Signal<bool> {
        self.inner.is_playing.clone()
    }

    /// The position, in seconds.
    pub fn position_secs(&self) -> Signal<f64> {
        self.inner.position_secs.clone()
    }

    /// The duration of the current track, in seconds (0 while unknown).
    pub fn duration_secs(&self) -> Signal<f64> {
        self.inner.duration_secs.clone()
    }

    /// The engine's non-modal notice, if any.
    pub fn notice(&self) -> Signal<Option<String>> {
        self.inner.notice.clone()
    }

    /// Applies a playback event; the events of other kinds are ignored.
    pub fn on_event(&self, event: &Event) {
        let Event::Playback(event) = event else {
            return;
        };
        let inner = &self.inner;
        match event {
            PlaybackEvent::StateChanged(state) => {
                inner.is_playing.set_if_changed(state.is_playing);
            }
            PlaybackEvent::TrackChanged(state) => self.apply_track(state),
            PlaybackEvent::PositionChanged {
                position_secs,
                duration_secs,
            } => {
                inner.position_secs.set_if_changed(*position_secs);
                inner.duration_secs.set_if_changed(*duration_secs);
            }
            PlaybackEvent::NoticeChanged(notice) => {
                inner.notice.set_if_changed(notice.clone());
            }
            PlaybackEvent::EndOfTrack(_) | PlaybackEvent::DeviceChanged(_) => {}
        }
    }

    /// Sets every signal from `state`.
    pub fn apply_snapshot(&self, state: &PlaybackSnapshot) {
        self.apply_track(state);
        self.inner.notice.set_if_changed(state.audio_notice.clone());
    }

    fn apply_track(&self, state: &PlaybackSnapshot) {
        let inner = &self.inner;
        inner.title.set_if_changed(state.title.clone());
        inner.artist.set_if_changed(state.artist.clone());
        inner.album.set_if_changed(state.album.clone());
        inner.cover_path.set_if_changed(state.cover_path.clone());
        inner.path.set_if_changed(state.path.clone());
        inner.is_playing.set_if_changed(state.is_playing);
        inner.position_secs.set_if_changed(state.position_secs);
        inner.duration_secs.set_if_changed(state.duration_secs);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;

    use super::*;
    use crate::frontend::events::DeviceSnapshot;

    fn snapshot(title: &str, playing: bool) -> PlaybackSnapshot {
        PlaybackSnapshot {
            is_playing: playing,
            volume: 1.0,
            position_secs: 12.0,
            duration_secs: 240.0,
            path: format!("/music/{title}.flac"),
            title: title.to_string(),
            artist: "Artist".into(),
            album: "Album".into(),
            cover_path: Some("cache/covers/a.avif".into()),
            eof_reached: false,
            source_sample_rate: 44_100,
            replay_gain_track: None,
            replay_gain_album: None,
            device: DeviceSnapshot {
                device_id: None,
                sample_rate: 44_100,
                channels: 2,
                bit_depth_display: String::new(),
                buffer_size: 0,
            },
            audio_notice: None,
        }
    }

    fn single() -> (AppContext, SingleNowPlaying) {
        let ctx = AppContext::in_memory().unwrap();
        let single = SingleNowPlaying::new(&ctx);
        (ctx, single)
    }

    #[test]
    fn it_starts_from_the_players_state() {
        let (ctx, single) = single();
        let state = ctx.player.snapshot();
        assert_eq!(single.title().get(), state.title);
        assert_eq!(single.path().get(), state.path);
        assert_eq!(single.is_playing().get(), state.is_playing);
        assert_eq!(single.position_secs().get(), state.current_pos_sec);
        assert_eq!(single.duration_secs().get(), state.total_duration_sec);
        assert_eq!(single.notice().get(), state.audio_notice);
    }

    #[test]
    fn a_track_change_sets_the_track_and_the_play_state() {
        let (_ctx, single) = single();
        let state = Arc::new(snapshot("Aladdin Sane", true));
        single.on_event(&Event::Playback(PlaybackEvent::TrackChanged(state)));
        assert_eq!(single.title().get(), "Aladdin Sane");
        assert_eq!(single.artist().get(), "Artist");
        assert_eq!(single.album().get(), "Album");
        assert_eq!(
            single.cover_path().get().as_deref(),
            Some("cache/covers/a.avif")
        );
        assert_eq!(single.path().get(), "/music/Aladdin Sane.flac");
        assert!(single.is_playing().get());
        assert_eq!(single.duration_secs().get(), 240.0);
        assert_eq!(single.position_secs().get(), 12.0);
    }

    #[test]
    fn a_state_change_only_moves_the_play_state() {
        let (_ctx, single) = single();
        single.on_event(&Event::Playback(PlaybackEvent::TrackChanged(Arc::new(
            snapshot("One", true),
        ))));
        single.on_event(&Event::Playback(PlaybackEvent::StateChanged(Arc::new(
            snapshot("Another title", false),
        ))));
        assert!(!single.is_playing().get());
        assert_eq!(
            single.title().get(),
            "One",
            "the track follows TrackChanged"
        );
    }

    #[test]
    fn a_position_change_moves_the_position_and_the_duration() {
        let (_ctx, single) = single();
        single.on_event(&Event::Playback(PlaybackEvent::PositionChanged {
            position_secs: 61.5,
            duration_secs: 300.0,
        }));
        assert_eq!(single.position_secs().get(), 61.5);
        assert_eq!(single.duration_secs().get(), 300.0);
    }

    #[test]
    fn the_notice_appears_and_goes_away() {
        let (_ctx, single) = single();
        single.on_event(&Event::Playback(PlaybackEvent::NoticeChanged(Some(
            "device lost".into(),
        ))));
        assert_eq!(single.notice().get().as_deref(), Some("device lost"));
        single.on_event(&Event::Playback(PlaybackEvent::NoticeChanged(None)));
        assert_eq!(single.notice().get(), None);
    }

    #[test]
    fn an_event_that_repeats_the_value_does_not_notify() {
        let (_ctx, single) = single();
        let notified = Rc::new(Cell::new(0));
        let counter = notified.clone();
        let _handle = single
            .position_secs()
            .observe(move |_| counter.set(counter.get() + 1));
        let event = Event::Playback(PlaybackEvent::PositionChanged {
            position_secs: 5.0,
            duration_secs: 10.0,
        });
        single.on_event(&event);
        single.on_event(&event);
        assert_eq!(notified.get(), 1);
    }

    #[test]
    fn events_of_other_origins_are_ignored() {
        let (_ctx, single) = single();
        let before = single.title().get();
        single.on_event(&Event::Library(
            crate::frontend::events::LibraryEvent::Changed,
        ));
        assert_eq!(single.title().get(), before);
    }

    #[test]
    fn the_player_state_after_a_play_is_picked_up_by_a_new_single() {
        let (ctx, _single) = single();
        let state = ctx.player.state();
        state.write().is_playing = true;
        state.write().title = "Already playing".into();
        let late = SingleNowPlaying::new(&ctx);
        assert!(late.is_playing().get());
        assert_eq!(late.title().get(), "Already playing");
    }
}
