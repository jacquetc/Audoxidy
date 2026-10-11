//! `PlayerViewModel`: what the player panel shows and what its controls do.
//!
//! It is a `Clone` handle of signals over the shared playback session
//! ([`AppServices`]). The track (title, artist, cover, position, play state)
//! comes from [`SingleNowPlaying`](crate::teksilo_ui::singles::SingleNowPlaying);
//! the transport acts on the shared [`playback::PlaybackCursor`] through the facade's
//! playback commands, and nothing is sent before the session restore settles
//! (quirk fix 6, [`PlayerViewModel::transport_enabled`]).
//!
//! # What the panel binds
//!
//! - the cover, the title and artist as the marquee shows them
//!   ([`title_marquee`]), their tooltips, the elapsed time, the seek position
//!   and whether the duration is known;
//! - the volume bubble ([`VolumeOverlay`]): a wheel step over the control
//!   band moves the volume by 5 % and shows it; the bubble goes after 1500 ms,
//!   1 s after a mouse move, and on a click;
//! - whether the controls are active, the zone under the pointer
//!   ([`HoverZone`]) and the seek tooltip ([`SeekHover`]), all computed from
//!   the pointer's position in the panel as Iced does (`app.rs:4672`).
//!
//! # Time
//!
//! The two timeouts and the activity window are plain deadlines
//! ([`timing`]); the view-model gives the earliest to the window's one-shot
//! wake-up and checks them from a frame-tick effect ([`PlayerViewModel::wire`],
//! [`PlayerViewModel::on_tick`]). The marquee scrolls on a
//! `subscribe_frame_tick_throttled(500 ms)` guard that the panel holds only
//! while [`PlayerViewModel::marquee_running`] is true; its tick is derived from
//! elapsed time ([`MarqueeClock`]). Every method that depends on the time has
//! an `_at(now)` form so tests drive it without waiting.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use teksilo::prelude::{BuildContext, Signal};

use super::geometry::{self, HoverZone, SeekHover, VolumeIcon};
use super::timing::{self, Activity, VolumeOverlay};
use super::title_marquee::{self, MarqueeClock};
use crate::frontend::FrontendError;
use crate::frontend::commands::playback;
use crate::logic::transport::fine_seek_enabled;
use crate::teksilo_ui::app::{AppServices, audio_center_tab};
use crate::teksilo_ui::widgets::album_art::SharedCover;
use crate::utils::format_duration;

/// A point in the panel, in logical pixels from its top-leading corner.
pub type PanelPoint = (f32, f32);

/// The shared window wake-up cell (`BuildContext::wake_at_handle`).
type WakeAt = Rc<Cell<Option<Instant>>>;

/// The deadlines and the clock: state that changes with the time.
#[derive(Default)]
struct Timers {
    overlay: VolumeOverlay,
    activity: Activity,
    clock: MarqueeClock,
    wake: Option<WakeAt>,
}

struct Inner {
    services: AppServices,
    timers: RefCell<Timers>,
    cover: Signal<Option<SharedCover>>,
    display_title: Signal<String>,
    display_artist: Signal<String>,
    title_tooltip: Signal<Option<String>>,
    artist_tooltip: Signal<Option<String>>,
    elapsed: Signal<String>,
    seek_value: Signal<f32>,
    duration_known: Signal<bool>,
    info_visible: Signal<bool>,
    marquee_running: Signal<bool>,
    controls_active: Signal<bool>,
    volume: Signal<Option<f32>>,
    pointer: Signal<Option<PanelPoint>>,
    menu_open: Signal<bool>,
    hover_zone: Signal<HoverZone>,
    seek_hover: Signal<Option<SeekHover>>,
    transport_enabled: Signal<bool>,
}

/// The player's view-model; see the module documentation.
#[derive(Clone)]
pub struct PlayerViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for PlayerViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerViewModel")
            .field("title", &self.inner.display_title.get())
            .field("controls_active", &self.inner.controls_active.get())
            .finish_non_exhaustive()
    }
}

impl PlayerViewModel {
    /// The view-model over `services`, showing the track the engine holds now.
    pub fn new(services: &AppServices) -> Self {
        let covers = services.covers.clone();
        let cover = services
            .now_playing
            .cover_path()
            .flat_map(move |path| match path {
                Some(path) => covers.request_shared(path),
                None => Signal::new(None),
            });
        let transport_enabled = services.session.status().map(|status| status.is_settled());
        let vm = Self {
            inner: Rc::new(Inner {
                services: services.clone(),
                timers: RefCell::default(),
                cover,
                display_title: Signal::new(String::new()),
                display_artist: Signal::new(String::new()),
                title_tooltip: Signal::new(None),
                artist_tooltip: Signal::new(None),
                elapsed: Signal::new(String::new()),
                seek_value: Signal::new(0.0),
                duration_known: Signal::new(false),
                info_visible: Signal::new(false),
                marquee_running: Signal::new(false),
                controls_active: Signal::new(false),
                volume: Signal::new(None),
                pointer: Signal::new(None),
                menu_open: Signal::new(false),
                hover_zone: Signal::new(HoverZone::None),
                seek_hover: Signal::new(None),
                transport_enabled,
            }),
        };
        let now = Instant::now();
        vm.on_text_changed(now);
        vm.refresh_progress();
        vm
    }

    /// Registers the view-model's effects and takes the window's wake-up cell.
    /// Call it from the panel's `build` (the panel lives as long as the
    /// window); the effects follow the track and drive the deadlines.
    pub fn wire(&self, ctx: &mut BuildContext) {
        self.inner.timers.borrow_mut().wake = Some(ctx.wake_at_handle());
        let now_playing = &self.inner.services.now_playing;

        let vm = self.clone();
        ctx.effect(&now_playing.title(), move |_| {
            vm.on_text_changed(Instant::now());
        });
        let vm = self.clone();
        ctx.effect(&now_playing.artist(), move |_| {
            vm.on_text_changed(Instant::now());
        });
        let vm = self.clone();
        ctx.effect(&now_playing.is_playing(), move |_| {
            vm.on_playing_changed(Instant::now());
        });
        let vm = self.clone();
        ctx.effect(&now_playing.position_secs(), move |_| vm.refresh_progress());
        let vm = self.clone();
        ctx.effect(&now_playing.duration_secs(), move |_| vm.refresh_progress());
        let vm = self.clone();
        ctx.effect(&ctx.frame_tick(), move |_| vm.on_tick(Instant::now()));
        self.arm_wake();
    }

    // --- What the panel binds ----------------------------------------------

    /// The cover of the playing track; `None` shows the placeholder.
    pub fn cover(&self) -> Signal<Option<SharedCover>> {
        self.inner.cover.clone()
    }

    /// The title as the marquee shows it.
    pub fn display_title(&self) -> Signal<String> {
        self.inner.display_title.clone()
    }

    /// The artist as the marquee shows it.
    pub fn display_artist(&self) -> Signal<String> {
        self.inner.display_artist.clone()
    }

    /// The full title when it is longer than 35 bytes (Iced's tooltip rule).
    pub fn title_tooltip(&self) -> Signal<Option<String>> {
        self.inner.title_tooltip.clone()
    }

    /// The full artist when it is longer than 35 bytes.
    pub fn artist_tooltip(&self) -> Signal<Option<String>> {
        self.inner.artist_tooltip.clone()
    }

    /// Whether the engine is playing: Play / Pause shows the other action.
    pub fn is_playing(&self) -> Signal<bool> {
        self.inner.services.now_playing.is_playing()
    }

    /// The elapsed time, `mm:ss`.
    pub fn elapsed(&self) -> Signal<String> {
        self.inner.elapsed.clone()
    }

    /// The seek slider's value, 0..=1. The slider writes it as the user drags;
    /// the position events write it otherwise.
    pub fn seek_value(&self) -> Signal<f32> {
        self.inner.seek_value.clone()
    }

    /// Whether the duration is known (`fine_seek_enabled`): the seek slider
    /// shows when it is, a flat rail when it is not.
    pub fn duration_known(&self) -> Signal<bool> {
        self.inner.duration_known.clone()
    }

    /// Whether the title, artist and elapsed time show: while playing or once
    /// the position has moved (`player.rs:117`).
    pub fn info_visible(&self) -> Signal<bool> {
        self.inner.info_visible.clone()
    }

    /// Whether the marquee scrolls now: a text overflows its window and the
    /// track plays. The panel holds its frame subscription exactly while this
    /// is true.
    pub fn marquee_running(&self) -> Signal<bool> {
        self.inner.marquee_running.clone()
    }

    /// Whether the user has been active in the last 1.5 s: the controls show
    /// their hover feedback and the seek tooltip only then.
    pub fn controls_active(&self) -> Signal<bool> {
        self.inner.controls_active.clone()
    }

    /// The volume the bubble shows, in percent, while it is up.
    pub fn volume(&self) -> Signal<Option<f32>> {
        self.inner.volume.clone()
    }

    /// The control zone that shows hover feedback now.
    pub fn hover_zone(&self) -> Signal<HoverZone> {
        self.inner.hover_zone.clone()
    }

    /// The seek tooltip, while it shows.
    pub fn seek_hover(&self) -> Signal<Option<SeekHover>> {
        self.inner.seek_hover.clone()
    }

    /// Whether the transport may be used: the session restore has settled
    /// (quirk fix 6).
    pub fn transport_enabled(&self) -> Signal<bool> {
        self.inner.transport_enabled.clone()
    }

    /// The speaker icon of the volume bubble, for `percent`.
    pub fn volume_icon(percent: f32) -> VolumeIcon {
        VolumeIcon::for_percent(percent)
    }

    /// Whether the transport may be used now.
    pub fn is_settled(&self) -> bool {
        self.inner.transport_enabled.get()
    }

    // --- Transport ------------------------------------------------------------

    /// Toggles play and pause (Iced's `PlayPause`). Does nothing before the
    /// restore settles or with no playlist. Returns whether it acted.
    pub fn play_pause(&self) -> bool {
        self.transport(|ctx, cursor| {
            playback::toggle_play_pause(ctx, cursor);
        })
    }

    /// Starts or resumes playback (Iced's `SetPlaying(true)`).
    pub fn play(&self) -> bool {
        self.transport(playback::play)
    }

    /// Pauses playback (Iced's `SetPlaying(false)`).
    pub fn pause(&self) -> bool {
        self.transport(playback::pause)
    }

    /// Stops playback (Iced's `Stop`). The controls are not woken, as in Iced.
    pub fn stop(&self) -> bool {
        if !self.is_settled() {
            return false;
        }
        playback::stop(&self.inner.services.ctx);
        true
    }

    /// Plays the next song (Iced's `NextTrack`). `None` before the restore
    /// settles; otherwise the command's outcome, which the panel reports as an
    /// error toast when the song could not be loaded (quirk fix 8).
    pub fn next(&self) -> Option<Result<(), FrontendError>> {
        self.change_track(playback::next)
    }

    /// Plays the previous song (Iced's `PreviousTrack`); see [`Self::next`].
    pub fn previous(&self) -> Option<Result<(), FrontendError>> {
        self.change_track(playback::previous)
    }

    /// Seeks to `seconds` (Iced's `SeekTo`): clamped to the track, ignored
    /// while the duration is unknown. Returns whether it seeked.
    pub fn seek_to(&self, seconds: f64) -> bool {
        if !self.is_settled() {
            return false;
        }
        let services = &self.inner.services;
        let seeked = services
            .session
            .read_cursor(|cursor| playback::seek(&services.ctx, cursor, seconds))
            .unwrap_or(false);
        if seeked {
            self.wake_controls_at(Instant::now(), false);
        }
        seeked
    }

    /// Seeks to `fraction` (0..=1) of the track: what the slider reports
    /// (Iced: `SeekTo(v * total_duration_sec)`, `player.rs:228`).
    pub fn seek_fraction(&self, fraction: f32) -> bool {
        let duration = self.inner.services.now_playing.duration_secs().get();
        if !fine_seek_enabled(duration) {
            return false;
        }
        self.seek_to(f64::from(fraction) * duration)
    }

    /// Seeks by `delta` seconds from the position (the keyboard and the media
    /// controls' relative seeks); returns whether it seeked.
    pub fn seek_by(&self, delta: f64) -> bool {
        if !self.is_settled() {
            return false;
        }
        let services = &self.inner.services;
        let seeked = services
            .session
            .read_cursor(|cursor| playback::seek_by(&services.ctx, cursor, delta))
            .unwrap_or(false);
        if seeked {
            self.wake_controls_at(Instant::now(), false);
        }
        seeked
    }

    /// Sets the volume, 0.0..=1.0 (Iced's `VolumeChanged`). Returns the volume
    /// applied, or `None` before the restore settles (the restore applies the
    /// saved volume, which a step must not race).
    pub fn set_volume(&self, volume: f32) -> Option<f32> {
        if !self.is_settled() {
            return None;
        }
        Some(playback::set_volume(&self.inner.services.ctx, volume))
    }

    /// A wheel step over the control band: `delta > 0` raises the volume by
    /// 5 %, anything else lowers it, and the bubble shows the new value
    /// (Iced's `PlayerScroll`). `delta` is Iced's: positive for a wheel turned
    /// up. Returns the new volume in percent, or `None` before the restore
    /// settles.
    pub fn scroll_volume(&self, delta: f32) -> Option<f32> {
        self.scroll_volume_at(Instant::now(), delta)
    }

    /// [`Self::scroll_volume`] at `now`.
    pub fn scroll_volume_at(&self, now: Instant, delta: f32) -> Option<f32> {
        if !self.is_settled() {
            return None;
        }
        let percent = playback::scroll_volume(&self.inner.services.ctx, delta);
        self.inner.timers.borrow_mut().overlay.wheel(now, percent);
        self.publish_timers();
        self.arm_wake();
        Some(percent)
    }

    fn transport(
        &self,
        act: impl FnOnce(&crate::frontend::AppContext, &playback::PlaybackCursor),
    ) -> bool {
        if !self.is_settled() {
            return false;
        }
        let services = &self.inner.services;
        let acted = services
            .session
            .read_cursor(|cursor| act(&services.ctx, cursor))
            .is_some();
        if acted {
            self.wake_controls_at(Instant::now(), false);
        }
        acted
    }

    fn change_track(
        &self,
        command: fn(
            &crate::frontend::AppContext,
            &mut playback::PlaybackCursor,
        ) -> Result<playback::TrackChange, FrontendError>,
    ) -> Option<Result<(), FrontendError>> {
        if !self.is_settled() {
            return None;
        }
        let services = &self.inner.services;
        let outcome = services
            .session
            .with_cursor(|cursor| command(&services.ctx, cursor))?;
        self.wake_controls_at(Instant::now(), false);
        Some(outcome.map(|_| ()))
    }

    // --- Pointer and time -------------------------------------------------------

    /// The pointer moved to `at` (in the panel), or left it (`None`)
    /// (Iced's `PlayerMouseMoved`, `app.rs:4733`): the controls wake, the
    /// volume bubble starts its short timeout, and the zone and seek tooltip
    /// follow the position.
    pub fn pointer_moved(&self, at: Option<PanelPoint>) {
        self.pointer_moved_at(Instant::now(), at);
    }

    /// [`Self::pointer_moved`] at `now`.
    pub fn pointer_moved_at(&self, now: Instant, at: Option<PanelPoint>) {
        self.inner.pointer.set_if_changed(at);
        if at.is_some() {
            self.wake_controls_at(now, true);
        } else {
            self.refresh_pointer_views();
        }
    }

    /// A click or a transport action woke the controls (`wake_up_controls`):
    /// they are active for another 1.5 s; a mouse move also starts the volume
    /// bubble's short timeout, anything else hides the bubble at once.
    pub fn wake_controls_at(&self, now: Instant, is_mouse_move: bool) {
        {
            let mut timers = self.inner.timers.borrow_mut();
            timers.activity.touch(now);
            if is_mouse_move {
                timers.overlay.pointer_moved(now);
            } else {
                timers.overlay.dismiss();
            }
        }
        self.publish_timers();
        self.arm_wake();
    }

    /// A click anywhere in the panel's control band or seek bar hides the
    /// volume bubble at once (Iced: the press that triggers an action).
    pub fn clicked(&self) {
        self.wake_controls_at(Instant::now(), false);
    }

    /// Runs what is due at `now`: hides the volume bubble and quiets the
    /// controls when their deadlines have come, advances the marquee, and
    /// hands the next deadline to the window's wake-up. Called from the frame
    /// tick, which also fires on frames others asked for, so it only reads the
    /// clock.
    pub fn on_tick(&self, now: Instant) {
        {
            let mut timers = self.inner.timers.borrow_mut();
            timers.overlay.expire(now);
            timers.activity.expire(now);
        }
        self.publish_timers();
        if self.inner.marquee_running.get() {
            self.update_display(now);
        }
        self.arm_wake();
    }

    /// The menu popover opened or closed: the zones show no feedback while it
    /// is open (`player.rs:332`).
    pub fn set_menu_open(&self, open: bool) {
        self.inner.menu_open.set_if_changed(open);
        self.refresh_pointer_views();
    }

    // --- The menu ------------------------------------------------------------------

    /// Starts scanning `path` (the player menu's "Abrir Carpeta"). The scan's
    /// progress arrives as library events.
    pub fn scan_folder(&self, path: &str) -> Result<(), FrontendError> {
        crate::frontend::commands::library::scan_folder(&self.inner.services.ctx, path)
    }

    /// Opens the Audio Center on its "Configuración de Audio" tab, or closes
    /// it when it shows that tab (Iced's `ToggleAudioCenter(Some(0))`).
    pub fn open_audio_config(&self) {
        self.toggle_audio_center(audio_center_tab::CONFIG);
    }

    /// The same for the "Ecualizador" tab.
    pub fn open_equalizer(&self) {
        self.toggle_audio_center(audio_center_tab::EQUALIZER);
    }

    /// The same for the "Efectos de Audio" tab.
    pub fn open_effects(&self) {
        self.toggle_audio_center(audio_center_tab::EFFECTS);
    }

    fn toggle_audio_center(&self, tab: usize) {
        self.inner.services.links.audio_center.toggle(Some(tab));
    }

    // --- Internals ---------------------------------------------------------------------

    /// Writes the timers' state to the signals the panel binds.
    fn publish_timers(&self) {
        let (shown, active) = {
            let timers = self.inner.timers.borrow();
            (timers.overlay.shown(), timers.activity.is_active())
        };
        self.inner.volume.set_if_changed(shown);
        self.inner.controls_active.set_if_changed(active);
        self.refresh_pointer_views();
    }

    /// Recomputes the zone and the seek tooltip from the pointer and the state
    /// they depend on.
    fn refresh_pointer_views(&self) {
        let inner = &self.inner;
        let pointer = inner.pointer.get();
        let active = inner.controls_active.get();
        let zone = match pointer {
            Some((x, y)) if active && inner.volume.get().is_none() && !inner.menu_open.get() => {
                HoverZone::at(x, y)
            }
            _ => HoverZone::None,
        };
        inner.hover_zone.set_if_changed(zone);
        let duration = inner.services.now_playing.duration_secs().get();
        let hover = match pointer {
            Some((x, y)) if active => geometry::seek_hover(x, y, duration),
            _ => None,
        };
        inner.seek_hover.set_if_changed(hover);
    }

    /// Gives the window's wake-up the earliest of the deadlines.
    fn arm_wake(&self) {
        let timers = self.inner.timers.borrow();
        let Some(wake) = &timers.wake else { return };
        let next = timing::earliest(timers.overlay.deadline(), timers.activity.deadline());
        if let Some(at) = next {
            wake.set(Some(timing::earlier(wake.get(), at)));
        }
    }

    /// The title or artist changed: scroll from the start, and rebuild what
    /// depends on the text.
    fn on_text_changed(&self, now: Instant) {
        let np = &self.inner.services.now_playing;
        let (title, artist) = (np.title().get(), np.artist().get());
        self.inner
            .title_tooltip
            .set_if_changed(geometry::wants_tooltip(&title).then_some(title));
        self.inner
            .artist_tooltip
            .set_if_changed(geometry::wants_tooltip(&artist).then_some(artist));
        self.inner.timers.borrow_mut().clock.reset(now);
        self.refresh_marquee(now);
    }

    fn on_playing_changed(&self, now: Instant) {
        self.refresh_marquee(now);
        self.refresh_progress();
    }

    /// Starts or stops the marquee clock for the text and the play state, and
    /// shows the window at the current tick.
    fn refresh_marquee(&self, now: Instant) {
        let np = &self.inner.services.now_playing;
        let scrolls = title_marquee::needs_scroll(&np.title().get(), &np.artist().get());
        let running = scrolls && np.is_playing().get();
        {
            let mut timers = self.inner.timers.borrow_mut();
            if running {
                timers.clock.start(now);
            } else if timers.clock.is_running() {
                timers.clock.stop(now);
            }
        }
        self.inner.marquee_running.set_if_changed(running);
        self.update_display(now);
    }

    /// Shows the title and artist at the marquee's tick at `now`.
    fn update_display(&self, now: Instant) {
        let np = &self.inner.services.now_playing;
        let tick = self.inner.timers.borrow().clock.tick(now);
        self.inner
            .display_title
            .set_if_changed(title_marquee::title_window(&np.title().get(), tick));
        self.inner
            .display_artist
            .set_if_changed(title_marquee::artist_window(&np.artist().get(), tick));
    }

    /// The position, duration and play state changed: the slider, the elapsed
    /// time and whether the info shows.
    fn refresh_progress(&self) {
        let inner = &self.inner;
        let np = &inner.services.now_playing;
        let (position, duration) = (np.position_secs().get(), np.duration_secs().get());
        let progress = if duration > 0.0 {
            (position / duration) as f32
        } else {
            0.0
        };
        inner.seek_value.set_if_changed(progress);
        inner.elapsed.set_if_changed(format_duration(position));
        inner
            .duration_known
            .set_if_changed(fine_seek_enabled(duration));
        inner
            .info_visible
            .set_if_changed(np.is_playing().get() || position > 0.0);
        self.refresh_pointer_views();
    }
}

#[cfg(test)]
mod tests;
