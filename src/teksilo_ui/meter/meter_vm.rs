//! `MeterViewModel`: the meter the bottom bar draws and the settings behind
//! its popup.
//!
//! A `Clone` handle of `Signal`s. While the engine plays, and while the bars
//! fall after a pause, [`MeterViewModel::tick`] reads the engine's
//! `MeterData` and advances a [`LiveMeter`] to the instant it is given, then
//! publishes a [`VuMeterFrame`]. The widget calls it on its frame ticks (which
//! may come more often than the Iced UI's 30 ms, or when something else asked
//! for a frame), so everything is computed from elapsed time.
//!
//! The Iced meter had four defects the Teksilo UI does not copy (D5-D8,
//! `docs/teksilo-port-iced-defects.md`; quirk fixes 43-46): its held peak
//! never decayed and pinned the bars at 0 dBFS, its RMS bar was always 0, its
//! pause fade rose to full scale, and its correlation and Mid/Side came from
//! dB values. The clip LED, which Iced never lights (`app.rs:5147`), follows
//! the peaks. See `crate::logic::meter::LiveMeter` for the rules.
//!
//! **Activity.** `active` is true while playing or fading and nothing else:
//! the widget holds its frame-tick subscription only then, so an idle meter
//! costs no frames. It turns true when [`MeterViewModel::wire`]'s effect sees
//! playback start, and false from a tick, once the fade is over.
//!
//! **Settings.** `enabled`, the hold time, the RMS window, the Mid/Side mode
//! and the infinite hold are read from the engine at construction and from
//! the database (`commands::meter::load`, the Mid/Side mode lives only there).
//! Every change goes to the facade (`commands::meter`), which applies it to
//! the engine and persists the key. The hold time and the RMS window are
//! Iced stubs, kept: the popup's steppers store them, and the meter keeps the
//! fixed 1.5 s hold and the 300 ms window of the decoder (the facade marks
//! the stub; `docs/teksilo-port-stubs.md`). A failed write is logged and, where
//! the control gives an event context, shown as an error toast (quirk fix 8).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use teksilo::prelude::*;

use crate::audio::meter::MeterData;
use crate::frontend::Reply;
use crate::frontend::commands::meter::{self, HOLD_TIME_RANGE_MS, RMS_WINDOW_RANGE_MS};
use crate::logic::meter::{LiveMeter, LiveReading};
use crate::teksilo_ui::app::AppServices;
use crate::teksilo_ui::widgets::VuMeterFrame;

struct Inner {
    services: AppServices,
    data: Arc<MeterData>,
    enabled: Signal<bool>,
    hold_time_ms: Signal<f64>,
    rms_window_ms: Signal<f64>,
    ms_mode: Signal<bool>,
    infinite_hold: Signal<bool>,
    frame: Signal<VuMeterFrame>,
    active: Signal<bool>,
    live: RefCell<LiveMeter>,
    /// Whether the user changed a setting: the late answer of the database
    /// read must not undo it.
    touched: Cell<bool>,
}

/// The meter's view-model; see the module documentation.
#[derive(Clone)]
pub struct MeterViewModel {
    inner: Rc<Inner>,
}

impl std::fmt::Debug for MeterViewModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MeterViewModel")
            .field("enabled", &self.inner.enabled.get())
            .field("active", &self.inner.active.get())
            .finish_non_exhaustive()
    }
}

/// The frame the widget draws for `live`, with the display options.
pub fn frame_of(live: &LiveMeter, mid_side: bool) -> VuMeterFrame {
    VuMeterFrame {
        peak_db: live.peak_db,
        level_db: live.level_db,
        hold_db: live.hold_db(),
        clipping: live.clipping,
        mid_side,
        correlation: live.correlation,
        show_numeric: true,
        crest_db: live.crest_db,
    }
}

impl MeterViewModel {
    /// The view-model over `services`, showing the engine's settings now and
    /// the database's (the Mid/Side mode) once they are read.
    pub fn new(services: &AppServices) -> Self {
        let state = services.ctx.player.snapshot();
        let live = LiveMeter::default();
        let ms_mode = false;
        let vm = Self {
            inner: Rc::new(Inner {
                services: services.clone(),
                data: services.ctx.player.meter(),
                enabled: Signal::new(state.meter_enabled),
                hold_time_ms: Signal::new(f64::from(state.meter_hold_time_ms)),
                rms_window_ms: Signal::new(f64::from(state.meter_rms_window_ms)),
                ms_mode: Signal::new(ms_mode),
                infinite_hold: Signal::new(state.meter_infinite_hold),
                frame: Signal::new(frame_of(&live, ms_mode)),
                active: Signal::new(services.now_playing.is_playing().get()),
                live: RefCell::new(live),
                touched: Cell::new(false),
            }),
        };
        vm.load();
        vm
    }

    /// Reads the persisted settings and shows them, unless the user changed
    /// one in the meantime.
    fn load(&self) {
        let reply = meter::load(&self.inner.services.ctx);
        let vm = self.clone();
        self.inner
            .services
            .runtime
            .spawn_local(async move {
                match reply.recv().await {
                    Ok(settings) if !vm.inner.touched.get() => {
                        let inner = &vm.inner;
                        inner.enabled.set_if_changed(settings.enabled);
                        inner
                            .hold_time_ms
                            .set_if_changed(f64::from(settings.hold_time_ms));
                        inner
                            .rms_window_ms
                            .set_if_changed(f64::from(settings.rms_window_ms));
                        inner.infinite_hold.set_if_changed(settings.infinite_hold);
                        inner.ms_mode.set_if_changed(settings.ms_mode);
                        vm.publish();
                    }
                    Ok(_) => {}
                    Err(error) => {
                        tracing::error!("the meter settings could not be read: {error}");
                    }
                }
            })
            .detach();
    }

    /// Follows the play state: playback starting makes the meter active, so
    /// its widget starts ticking. Call it from the widget's `build`.
    pub fn wire(&self, ctx: &mut BuildContext) {
        let vm = self.clone();
        ctx.effect(
            &self.inner.services.now_playing.is_playing(),
            move |playing| {
                if *playing {
                    vm.inner.active.set_if_changed(true);
                }
            },
        );
    }

    /// Whether the meter is shown ("Mostrar medidor").
    pub fn enabled(&self) -> Signal<bool> {
        self.inner.enabled.clone()
    }

    /// The peak hold time stepper's value, in ms. (An Iced stub: stored, not
    /// read by the meter.)
    pub fn hold_time_ms(&self) -> Signal<f64> {
        self.inner.hold_time_ms.clone()
    }

    /// The RMS window stepper's value, in ms. (An Iced stub: stored, not read
    /// by the meter.)
    pub fn rms_window_ms(&self) -> Signal<f64> {
        self.inner.rms_window_ms.clone()
    }

    /// Whether the bars show Mid and Side.
    pub fn ms_mode(&self) -> Signal<bool> {
        self.inner.ms_mode.clone()
    }

    /// Whether peaks are held for ever.
    pub fn infinite_hold(&self) -> Signal<bool> {
        self.inner.infinite_hold.clone()
    }

    /// What the meter draws.
    pub fn frame(&self) -> Signal<VuMeterFrame> {
        self.inner.frame.clone()
    }

    /// Whether the meter is playing or fading: true is the only time it needs
    /// frame ticks.
    pub fn active(&self) -> Signal<bool> {
        self.inner.active.clone()
    }

    /// Advances the meter to `now`, reading the engine's levels and the play
    /// state, and publishes the frame and the activity.
    pub fn tick(&self, now: Instant) {
        let services = &self.inner.services;
        let playing = services.now_playing.is_playing().get();
        let path = services.now_playing.path().get();
        let data = &self.inner.data;
        let reading = LiveReading {
            track_path: &path,
            is_playing: playing,
            infinite_hold: self.inner.infinite_hold.get(),
            mid_side: self.inner.ms_mode.get(),
            peak_db: [data.read_peak_l(), data.read_peak_r()],
            rms_db: [data.read_rms_l(), data.read_rms_r()],
            mid_side_peak_db: [data.read_mid_peak(), data.read_side_peak()],
            mid_side_rms_db: [data.read_mid_rms(), data.read_side_rms()],
            correlation: data.read_correlation(),
        };
        let animating = {
            let mut live = self.inner.live.borrow_mut();
            live.update(&reading, now);
            live.is_animating(playing)
        };
        self.publish();
        self.inner.active.set_if_changed(animating);
    }

    /// Clears the held peaks and the clip LED ("reset peak"). Shown at once,
    /// even while nothing plays.
    pub fn reset_peaks(&self) {
        self.inner.live.borrow_mut().reset_peaks();
        self.publish();
    }

    /// Publishes the meter's frame.
    fn publish(&self) {
        let frame = frame_of(&self.inner.live.borrow(), self.inner.ms_mode.get());
        self.inner.frame.set_if_changed(frame);
    }

    /// The user showed or hid the meter (`MeterToggleEnabled`,
    /// `app.rs:4983`).
    pub fn set_enabled(&self, enabled: bool, ctx: &mut EventContext) {
        self.inner.touched.set(true);
        self.inner.enabled.set_if_changed(enabled);
        let reply = meter::set_enabled(&self.inner.services.ctx, enabled);
        self.watch(reply, Some(ctx));
    }

    /// The user chose a peak hold time (`MeterHoldTimeChanged`,
    /// `app.rs:4909`), clamped to 500-5000 ms. The Iced meter ignores it.
    pub fn set_hold_time_ms(&self, ms: f64) {
        self.inner.touched.set(true);
        let range = f64::from(*HOLD_TIME_RANGE_MS.start())..=f64::from(*HOLD_TIME_RANGE_MS.end());
        let ms = ms.clamp(*range.start(), *range.end());
        self.inner.hold_time_ms.set_if_changed(ms);
        let reply = meter::set_hold_time_ms(&self.inner.services.ctx, ms as f32);
        self.watch(reply, None);
    }

    /// The user chose an RMS window (`MeterRmsWindowChanged`, `app.rs:4928`),
    /// clamped to 50-1000 ms. Nothing reads it.
    pub fn set_rms_window_ms(&self, ms: f64) {
        self.inner.touched.set(true);
        let range = f64::from(*RMS_WINDOW_RANGE_MS.start())..=f64::from(*RMS_WINDOW_RANGE_MS.end());
        let ms = ms.clamp(*range.start(), *range.end());
        self.inner.rms_window_ms.set_if_changed(ms);
        let reply = meter::set_rms_window_ms(&self.inner.services.ctx, ms as f32);
        self.watch(reply, None);
    }

    /// The user switched Mid/Side on or off (`MeterMsModeToggle`,
    /// `app.rs:4947`).
    pub fn set_ms_mode(&self, ms_mode: bool, ctx: &mut EventContext) {
        self.inner.touched.set(true);
        self.inner.ms_mode.set_if_changed(ms_mode);
        self.publish();
        let reply = meter::set_ms_mode(&self.inner.services.ctx, ms_mode);
        self.watch(reply, Some(ctx));
    }

    /// The user switched the infinite hold on or off
    /// (`MeterInfiniteHoldToggle`, `app.rs:4961`).
    pub fn set_infinite_hold(&self, infinite: bool, ctx: &mut EventContext) {
        self.inner.touched.set(true);
        self.inner.infinite_hold.set_if_changed(infinite);
        let reply = meter::set_infinite_hold(&self.inner.services.ctx, infinite);
        self.watch(reply, Some(ctx));
    }

    /// Awaits the database write of a change; a failure is logged and, given
    /// a window's `EventContext`, shown as an error toast (quirk fix 8). A
    /// stepper's change callback has no context: it logs only. A tree without
    /// a window (a headless test) cannot present a toast either.
    fn watch(&self, reply: Reply<()>, ctx: Option<&mut EventContext>) {
        match ctx {
            Some(ctx) if ctx.window().is_some() => {
                ctx.spawn_local_with(async move { reply.recv().await }, |result, ctx| {
                    if let Err(error) = result {
                        tracing::error!("the meter setting could not be saved: {error}");
                        ctx.show_toast(
                            Toast::error(tr!(meter_save_failed())).body(lit!(error.to_string())),
                        );
                    }
                })
                .detach();
            }
            _ => {
                self.inner
                    .services
                    .runtime
                    .spawn_local(async move {
                        if let Err(error) = reply.recv().await {
                            tracing::error!("the meter setting could not be saved: {error}");
                        }
                    })
                    .detach();
            }
        }
    }
}

#[cfg(test)]
mod tests;
