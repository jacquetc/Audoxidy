//! VU-meter ballistics: the per-tick state machine that turns raw peak and RMS
//! readings (dBFS) into the values the meter draws. It resets on track change,
//! holds peaks (optionally forever), smooths with a fast attack and a slow
//! release, derives Mid/Side, phase correlation and crest factor, and fades
//! out on pause. Time is injected, so every behaviour is testable.

use std::time::{Duration, Instant};

/// Lowest level drawn by the meter (dBFS); peak reset target.
pub const METER_MIN_DB: f32 = -60.0;
/// How long a peak is held before it starts to decay (seconds).
pub const METER_HOLD_TIME_SECS: f32 = 1.5;
/// Decay rate of a held peak (dB per second).
pub const METER_DECAY_RATE_DB_PER_SEC: f32 = 1.8;
/// Tick length the decay step assumes (seconds): each update decays by
/// `METER_DECAY_RATE_DB_PER_SEC × METER_DECAY_TICK_SECS`, whatever the real
/// time between updates.
pub const METER_DECAY_TICK_SECS: f32 = 0.030;
/// Smoothing coefficient when the level rises (fast attack).
pub const METER_ATTACK_ALPHA: f32 = 0.85;
/// Smoothing coefficient when the level falls (slow release).
pub const METER_RELEASE_ALPHA: f32 = 0.05;
/// Length of the fade-out after pausing (seconds).
pub const METER_FADE_OUT_SECS: f32 = 0.4;
/// Interval at which the meter is updated while playing or fading.
pub const METER_TICK_PERIOD: Duration = Duration::from_millis(30);

/// One step of the peak-hold state machine (Holding → Decaying → Idle).
///
/// Returns `(new_hold_value, new_hold_start, new_decay_value)`.
pub fn peak_hold_step(
    hold_value: f32,
    hold_start: Option<Instant>,
    decay_value: f32,
    raw_peak: f32,
    now: Instant,
) -> (f32, Option<Instant>, f32) {
    let (mut new_hold, mut new_start, mut new_decay) = (hold_value, hold_start, decay_value);

    if let Some(start) = new_start {
        // Holding: once the hold time is over, start decaying from the hold.
        if now.saturating_duration_since(start).as_secs_f32() > METER_HOLD_TIME_SECS {
            new_decay = new_hold;
            new_start = None;
        }
    }

    if new_start.is_none() && new_decay > 0.0 {
        // Decaying: a fixed decrement per update.
        new_decay -= METER_DECAY_RATE_DB_PER_SEC * METER_DECAY_TICK_SECS;
        if new_decay <= raw_peak {
            // Decaying → Idle
            new_decay = 0.0;
        }
    }

    if raw_peak > new_hold {
        // A louder peak snaps the hold up and re-enters Holding.
        new_hold = raw_peak;
        new_start = Some(now);
        new_decay = 0.0;
    } else if new_start.is_none() && new_decay <= 0.0 {
        // Idle: the hold follows the signal or the decay level.
        new_hold = raw_peak.max(new_decay);
    }

    (new_hold, new_start, new_decay)
}

/// One step of the asymmetric smoothing: fast attack, slow release.
pub fn ballistic_step(current: f32, target: f32) -> f32 {
    let alpha = if target > current {
        METER_ATTACK_ALPHA
    } else {
        METER_RELEASE_ALPHA
    };
    current + alpha * (target - current)
}

/// One update's raw input: the playback state and the four dBFS readings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MeterReading<'a> {
    /// Path of the track being played (a change resets the meter).
    pub track_path: &'a str,
    /// Whether playback is running.
    pub is_playing: bool,
    /// Whether peaks are held forever.
    pub infinite_hold: bool,
    /// Left peak (dBFS).
    pub peak_l: f32,
    /// Right peak (dBFS).
    pub peak_r: f32,
    /// Left RMS (dBFS).
    pub rms_l: f32,
    /// Right RMS (dBFS).
    pub rms_r: f32,
}

/// Animation state of the stereo VU meter, updated once per tick.
#[derive(Debug, Clone)]
pub struct MeterState {
    /// Left (or Mid) bar level after ballistics.
    pub display_l: f32,
    /// Right (or Side) bar level after ballistics.
    pub display_r: f32,

    /// Held left peak.
    pub peak_hold_l: f32,
    /// Held right peak.
    pub peak_hold_r: f32,
    /// When the left hold started (`None` while decaying or idle).
    pub peak_hold_start_l: Option<Instant>,
    /// When the right hold started (`None` while decaying or idle).
    pub peak_hold_start_r: Option<Instant>,
    /// Left decay level (`0.0` when not decaying).
    pub peak_decay_l: f32,
    /// Right decay level (`0.0` when not decaying).
    pub peak_decay_r: f32,

    /// Smoothed left peak (kept for the widget; not updated by the ballistics).
    pub smooth_peak_l: f32,
    /// Smoothed right peak (kept for the widget; not updated by the ballistics).
    pub smooth_peak_r: f32,
    /// Smoothed left RMS (not updated by the ballistics).
    pub smooth_rms_l: f32,
    /// Smoothed right RMS (not updated by the ballistics).
    pub smooth_rms_r: f32,

    /// Left level captured when playback paused (fade start).
    pub last_playing_l: f32,
    /// Right level captured when playback paused (fade start).
    pub last_playing_r: f32,
    /// Whether the pause fade-out is running.
    pub is_fading_out: bool,
    /// When the pause fade-out started.
    pub fade_out_start: Option<Instant>,

    /// Track the state belongs to; a different path resets the meter.
    pub last_track_path: String,

    /// M/S display mode: the bars show Mid/Side instead of L/R.
    pub ms_mode: bool,
    /// Phase correlation: -1.0 to +1.0 (`0.0` for silence).
    pub phase_corr: f32,
    /// Whether the numeric dBFS readout is shown.
    pub show_numeric: bool,
    /// Left crest factor: peak minus RMS (dB).
    pub crest_l: f32,
    /// Right crest factor: peak minus RMS (dB).
    pub crest_r: f32,
}

impl Default for MeterState {
    fn default() -> Self {
        Self {
            display_l: 0.0,
            display_r: 0.0,
            peak_hold_l: 0.0,
            peak_hold_r: 0.0,
            peak_hold_start_l: None,
            peak_hold_start_r: None,
            peak_decay_l: 0.0,
            peak_decay_r: 0.0,
            smooth_peak_l: 0.0,
            smooth_peak_r: 0.0,
            smooth_rms_l: 0.0,
            smooth_rms_r: 0.0,
            last_playing_l: 0.0,
            last_playing_r: 0.0,
            is_fading_out: false,
            fade_out_start: None,
            last_track_path: String::new(),
            ms_mode: false,
            phase_corr: 0.0,
            show_numeric: true,
            crest_l: 0.0,
            crest_r: 0.0,
        }
    }
}

impl MeterState {
    /// Whether the meter still animates (it must keep receiving updates).
    pub fn is_animating(&self, is_playing: bool) -> bool {
        is_playing || self.is_fading_out
    }

    /// Clears both held peaks to [`METER_MIN_DB`] (the "reset peak" action).
    pub fn reset_peaks(&mut self) {
        self.peak_hold_l = METER_MIN_DB;
        self.peak_hold_r = METER_MIN_DB;
        self.peak_hold_start_l = None;
        self.peak_hold_start_r = None;
        self.peak_decay_l = 0.0;
        self.peak_decay_r = 0.0;
    }

    /// Advances the meter by one tick at instant `now`.
    ///
    /// - A new track resets everything except the M/S mode and the numeric
    ///   readout, and skips this tick.
    /// - In M/S mode the bars show Mid `(L+R)/2` and Side `(L−R)/2`.
    /// - While playing: peak hold (or infinite hold), ballistics towards the
    ///   max of the reading and the hold, phase correlation and crest factor.
    /// - On pause: the levels are captured and fade linearly to zero over
    ///   [`METER_FADE_OUT_SECS`].
    pub fn update(&mut self, reading: &MeterReading<'_>, now: Instant) {
        if reading.track_path != self.last_track_path {
            let ms_mode = self.ms_mode;
            let show_numeric = self.show_numeric;
            *self = MeterState::default();
            self.last_track_path = reading.track_path.to_string();
            self.ms_mode = ms_mode;
            self.show_numeric = show_numeric;
            return;
        }

        let (raw_peak_l, raw_peak_r, raw_rms_l, raw_rms_r) =
            (reading.peak_l, reading.peak_r, reading.rms_l, reading.rms_r);

        let (display_peak_l, display_peak_r) = if self.ms_mode {
            let mid_peak = (raw_peak_l + raw_peak_r) / 2.0;
            let side_peak = (raw_peak_l - raw_peak_r) / 2.0;
            (mid_peak, side_peak)
        } else {
            (raw_peak_l, raw_peak_r)
        };

        if reading.is_playing {
            self.is_fading_out = false;

            if reading.infinite_hold {
                // Infinite hold: the held values only ever rise.
                self.peak_hold_l = self.peak_hold_l.max(display_peak_l);
                self.peak_hold_r = self.peak_hold_r.max(display_peak_r);
                self.peak_hold_start_l = None;
                self.peak_hold_start_r = None;
                self.peak_decay_l = 0.0;
                self.peak_decay_r = 0.0;
            } else {
                let (hold_l, start_l, decay_l) = peak_hold_step(
                    self.peak_hold_l,
                    self.peak_hold_start_l,
                    self.peak_decay_l,
                    display_peak_l,
                    now,
                );
                self.peak_hold_l = hold_l;
                self.peak_hold_start_l = start_l;
                self.peak_decay_l = decay_l;

                let (hold_r, start_r, decay_r) = peak_hold_step(
                    self.peak_hold_r,
                    self.peak_hold_start_r,
                    self.peak_decay_r,
                    display_peak_r,
                    now,
                );
                self.peak_hold_r = hold_r;
                self.peak_hold_start_r = start_r;
                self.peak_decay_r = decay_r;
            }

            // Ballistics towards the max of the reading and the held peak.
            let target_l = display_peak_l.max(self.peak_hold_l);
            let target_r = display_peak_r.max(self.peak_hold_r);
            self.display_l = ballistic_step(self.display_l, target_l);
            self.display_r = ballistic_step(self.display_r, target_r);

            // Phase correlation from the RMS readings; undefined for silence.
            self.phase_corr = if raw_rms_l.abs() > 0.001 && raw_rms_r.abs() > 0.001 {
                let corr = (raw_rms_l * raw_rms_r) / (raw_rms_l.abs() * raw_rms_r.abs());
                corr.clamp(-1.0, 1.0)
            } else {
                0.0
            };

            // Crest factor: peak minus RMS, when the RMS is above the floor.
            self.crest_l = if raw_rms_l > METER_MIN_DB + 1.0 {
                raw_peak_l - raw_rms_l
            } else {
                0.0
            };
            self.crest_r = if raw_rms_r > METER_MIN_DB + 1.0 {
                raw_peak_r - raw_rms_r
            } else {
                0.0
            };
        } else if !self.is_fading_out {
            // Playing → paused: capture the levels and start the fade.
            self.last_playing_l = self.display_l;
            self.last_playing_r = self.display_r;
            self.is_fading_out = true;
            self.fade_out_start = Some(now);
        }

        if self.is_fading_out
            && let Some(start) = self.fade_out_start
        {
            let elapsed = now.saturating_duration_since(start).as_secs_f32();
            let factor = (1.0 - elapsed / METER_FADE_OUT_SECS).clamp(0.0, 1.0);
            self.display_l = self.last_playing_l * factor;
            self.display_r = self.last_playing_r * factor;
            if factor <= 0.0 {
                self.is_fading_out = false;
                self.display_l = 0.0;
                self.display_r = 0.0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The correct meter (Teksilo UI only)
//
// Everything above is the Iced UI's meter and keeps its documented defects
// (`docs/teksilo-port-iced-defects.md` D5-D8): the held peak never decays, the
// RMS bar is always 0, the pause fade rises to full scale, and the phase
// correlation and Mid/Side come from dB values. What follows replaces it for
// the Teksilo UI. It works on the clock (so the tick rate does not matter),
// on RMS for the bars, on falling levels for the fade, and on the linear
// statistics the decoder publishes for the correlation and Mid/Side.
// ---------------------------------------------------------------------------

/// Highest level the decoder publishes (dBFS); a louder one is clamped to it.
pub const METER_MAX_DB: f32 = 6.0;
/// Level at which a peak counts as clipping (dBFS): a sample at full scale.
pub const METER_CLIP_DB: f32 = 0.0;
/// How long the clip LED stays lit after the last clipped batch (seconds).
pub const METER_CLIP_HOLD_SECS: f32 = METER_HOLD_TIME_SECS;
/// The longest gap between two updates that counts as elapsed time (seconds):
/// a stalled UI must not jump the held peaks or the ballistics.
pub const METER_MAX_STEP_SECS: f32 = 0.5;

/// Brings a reading into the drawn range: silence (`-inf`, NaN, below the
/// floor) becomes [`METER_MIN_DB`], and a level above [`METER_MAX_DB`] is
/// capped.
pub fn sanitize_db(db: f32) -> f32 {
    if db.is_nan() || db < METER_MIN_DB {
        METER_MIN_DB
    } else {
        db.min(METER_MAX_DB)
    }
}

/// A mean square (linear power) in dBFS: `10 log10(ms)`, floored at
/// [`METER_MIN_DB`] (zero, negative and NaN give the floor).
pub fn mean_square_to_db(mean_square: f64) -> f32 {
    if mean_square > 0.0 {
        sanitize_db((10.0 * mean_square.log10()) as f32)
    } else {
        METER_MIN_DB
    }
}

/// The phase correlation, `-1.0..=1.0`, from the sums over a window of
/// linear samples: `Σlr / sqrt(Σll · Σrr)`. `0.0` when either channel is
/// silent (the correlation is undefined) or the sums are not finite.
///
/// Identical channels give `+1`, opposite channels `-1`, unrelated ones about
/// `0`. (The Iced UI takes the sign product of two dB values, which is
/// always `+1`: defect D8.)
pub fn correlation_from_sums(sum_ll: f64, sum_rr: f64, sum_lr: f64) -> f32 {
    let denominator = (sum_ll * sum_rr).sqrt();
    if !denominator.is_finite() || denominator <= 1e-18 {
        return 0.0;
    }
    ((sum_lr / denominator) as f32).clamp(-1.0, 1.0)
}

/// The mean squares of Mid `(L+R)/2` and Side `(L-R)/2` over `count` linear
/// sample pairs, from the sums of `l²`, `r²` and `l·r`. `(0.0, 0.0)` for an
/// empty window.
pub fn mid_side_mean_squares(sum_ll: f64, sum_rr: f64, sum_lr: f64, count: u64) -> (f64, f64) {
    if count == 0 {
        return (0.0, 0.0);
    }
    let n = count as f64;
    let mid = (sum_ll + 2.0 * sum_lr + sum_rr) / (4.0 * n);
    let side = (sum_ll - 2.0 * sum_lr + sum_rr) / (4.0 * n);
    (mid.max(0.0), side.max(0.0))
}

/// The level a held peak has fallen to `age_secs` after it was raised: it
/// stays for [`METER_HOLD_TIME_SECS`], then falls by
/// [`METER_DECAY_RATE_DB_PER_SEC`] dB per second, down to [`METER_MIN_DB`].
/// (The Iced hold never falls, defect D5.)
pub fn held_peak_db(raised_db: f32, age_secs: f32) -> f32 {
    let falling = (age_secs - METER_HOLD_TIME_SECS).max(0.0);
    (raised_db - METER_DECAY_RATE_DB_PER_SEC * falling).max(METER_MIN_DB)
}

/// [`ballistic_step`] for `dt_secs` of elapsed time instead of one 30 ms
/// tick: the per-tick coefficient is compounded over `dt / 30 ms` ticks, so
/// the same signal gives the same bar at any update rate.
pub fn ballistic_step_dt(current: f32, target: f32, dt_secs: f32) -> f32 {
    let alpha = if target > current {
        METER_ATTACK_ALPHA
    } else {
        METER_RELEASE_ALPHA
    };
    let ticks = (dt_secs / METER_DECAY_TICK_SECS).max(0.0);
    let compounded = 1.0 - (1.0 - alpha).powf(ticks);
    current + compounded * (target - current)
}

/// The level `elapsed_secs` into a pause fade that started at `from_db`: it
/// falls linearly in dB to [`METER_MIN_DB`] over [`METER_FADE_OUT_SECS`].
/// (The Iced fade multiplies the dB value by a factor going to 0, so the bar
/// climbs to full scale, defect D7.)
pub fn faded_db(from_db: f32, elapsed_secs: f32) -> f32 {
    let remaining = (1.0 - elapsed_secs / METER_FADE_OUT_SECS).clamp(0.0, 1.0);
    let from = from_db.max(METER_MIN_DB);
    METER_MIN_DB + (from - METER_MIN_DB) * remaining
}

/// One held peak: the level it was raised to and how long ago (playing time).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PeakHold {
    /// The level the hold was last raised to (dBFS).
    pub raised_db: f32,
    /// Playing time since it was raised (seconds).
    pub age_secs: f32,
}

impl Default for PeakHold {
    fn default() -> Self {
        Self {
            raised_db: METER_MIN_DB,
            age_secs: 0.0,
        }
    }
}

impl PeakHold {
    /// The held level now (see [`held_peak_db`]).
    pub fn value_db(&self) -> f32 {
        held_peak_db(self.raised_db, self.age_secs)
    }

    /// Advances by `dt_secs` of playing time and takes `peak_db` if it is at
    /// least the held level. With `infinite` the hold never ages.
    pub fn update(&mut self, peak_db: f32, infinite: bool, dt_secs: f32) {
        if !infinite {
            self.age_secs += dt_secs;
        }
        let held = self.value_db();
        if peak_db >= held {
            self.raised_db = peak_db;
            self.age_secs = 0.0;
        } else if infinite {
            // Infinite hold keeps the level it had, even if it had started to
            // fall before the option was switched on.
            self.raised_db = held;
            self.age_secs = 0.0;
        }
    }

    /// Drops the hold to the floor.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// One update's input of the correct meter: the playback state and what the
/// decoder published (peaks and RMS per channel, and the linear statistics).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveReading<'a> {
    /// Path of the track being played (a change resets the meter).
    pub track_path: &'a str,
    /// Whether playback is running.
    pub is_playing: bool,
    /// Whether peaks are held forever.
    pub infinite_hold: bool,
    /// Whether the bars show Mid and Side instead of Left and Right.
    pub mid_side: bool,
    /// Left and right peaks (dBFS).
    pub peak_db: [f32; 2],
    /// Left and right RMS (dBFS).
    pub rms_db: [f32; 2],
    /// Mid and side peaks (dBFS).
    pub mid_side_peak_db: [f32; 2],
    /// Mid and side RMS (dBFS).
    pub mid_side_rms_db: [f32; 2],
    /// The phase correlation of the window, `-1.0..=1.0`.
    pub correlation: f32,
}

impl Default for LiveReading<'_> {
    /// Silence on no track, paused.
    fn default() -> Self {
        Self {
            track_path: "",
            is_playing: false,
            infinite_hold: false,
            mid_side: false,
            peak_db: [METER_MIN_DB; 2],
            rms_db: [METER_MIN_DB; 2],
            mid_side_peak_db: [METER_MIN_DB; 2],
            mid_side_rms_db: [METER_MIN_DB; 2],
            correlation: 0.0,
        }
    }
}

/// The pause fade in progress.
#[derive(Debug, Clone, Copy)]
struct Fade {
    start: Instant,
    from_peak_db: [f32; 2],
    from_level_db: [f32; 2],
}

/// The stereo VU meter the Teksilo UI draws, advanced by the clock.
///
/// Index 0 is the top bar (left, or mid in Mid/Side mode), index 1 the bottom
/// bar (right, or side). Unlike [`MeterState`] it:
///
/// - holds each peak for [`METER_HOLD_TIME_SECS`], then lets it fall by
///   [`METER_DECAY_RATE_DB_PER_SEC`] dB per second (or never, with infinite
///   hold), counting playing time only (D5);
/// - draws the RMS as the bar level, and the peak as the readout (D6);
/// - fades to silence on pause, falling to [`METER_MIN_DB`] (D7);
/// - takes the correlation and Mid/Side from linear statistics (D8);
/// - lights the clip LED while a peak reaches [`METER_CLIP_DB`] and for
///   [`METER_CLIP_HOLD_SECS`] after.
#[derive(Debug, Clone)]
pub struct LiveMeter {
    /// Smoothed peak of each bar (dBFS): the "Pk" readout.
    pub peak_db: [f32; 2],
    /// Smoothed RMS of each bar (dBFS): the bar level and the "Rms" readout.
    pub level_db: [f32; 2],
    /// The peak-hold marker of each bar.
    pub hold: [PeakHold; 2],
    /// Crest factor of each bar (peak minus RMS, dB).
    pub crest_db: [f32; 2],
    /// Phase correlation, `-1.0..=1.0`.
    pub correlation: f32,
    /// Whether the clip LED is lit.
    pub clipping: bool,
    /// Whether the pause fade is running.
    pub is_fading_out: bool,
    /// Track the state belongs to; a different path resets the meter.
    pub last_track_path: String,
    /// The mode of the last update (a change drops the holds: Mid/Side
    /// levels are not Left/Right levels).
    mid_side: bool,
    clip_until: Option<Instant>,
    fade: Option<Fade>,
    last_update: Option<Instant>,
    /// Whether the last update was playing or fading (so the next one has a
    /// meaningful `dt`).
    was_running: bool,
}

impl Default for LiveMeter {
    fn default() -> Self {
        Self {
            peak_db: [METER_MIN_DB; 2],
            level_db: [METER_MIN_DB; 2],
            hold: [PeakHold::default(); 2],
            crest_db: [0.0; 2],
            correlation: 0.0,
            clipping: false,
            is_fading_out: false,
            last_track_path: String::new(),
            mid_side: false,
            clip_until: None,
            fade: None,
            last_update: None,
            was_running: false,
        }
    }
}

impl LiveMeter {
    /// The held level of each bar (dBFS); at or below [`METER_MIN_DB`] the
    /// marker is not drawn.
    pub fn hold_db(&self) -> [f32; 2] {
        [self.hold[0].value_db(), self.hold[1].value_db()]
    }

    /// Whether the meter still animates (it must keep receiving updates).
    pub fn is_animating(&self, is_playing: bool) -> bool {
        is_playing || self.is_fading_out
    }

    /// Clears both held peaks and the clip LED ("reset peak").
    pub fn reset_peaks(&mut self) {
        self.hold[0].reset();
        self.hold[1].reset();
        self.clip_until = None;
        self.clipping = false;
    }

    /// Advances the meter to instant `now`.
    ///
    /// A new track resets everything and takes no reading. While playing the
    /// holds, the ballistics, the crest factors, the correlation and the clip
    /// LED follow the reading; on pause the bars fall to silence over
    /// [`METER_FADE_OUT_SECS`] and the holds stay where they were.
    pub fn update(&mut self, reading: &LiveReading<'_>, now: Instant) {
        if reading.track_path != self.last_track_path {
            *self = Self {
                last_track_path: reading.track_path.to_string(),
                mid_side: reading.mid_side,
                last_update: Some(now),
                ..Self::default()
            };
            return;
        }

        let dt = match self.last_update {
            Some(last) if self.was_running => now
                .saturating_duration_since(last)
                .as_secs_f32()
                .min(METER_MAX_STEP_SECS),
            _ => 0.0,
        };
        self.last_update = Some(now);

        if reading.mid_side != self.mid_side {
            self.mid_side = reading.mid_side;
            self.hold[0].reset();
            self.hold[1].reset();
        }

        if reading.is_playing {
            self.play(reading, now, dt);
        } else {
            self.pause(now);
        }

        if !reading.is_playing && !self.is_fading_out {
            // Nothing will update the LED once the meter is idle.
            self.clip_until = None;
        }
        self.clipping = self.clip_until.is_some_and(|until| now < until);
        self.was_running = reading.is_playing || self.is_fading_out;
    }

    fn play(&mut self, reading: &LiveReading<'_>, now: Instant, dt: f32) {
        self.is_fading_out = false;
        self.fade = None;

        let (peaks, rms) = if reading.mid_side {
            (reading.mid_side_peak_db, reading.mid_side_rms_db)
        } else {
            (reading.peak_db, reading.rms_db)
        };
        let peaks = peaks.map(sanitize_db);
        let rms = rms.map(sanitize_db);

        for bar in 0..2 {
            self.hold[bar].update(peaks[bar], reading.infinite_hold, dt);
            self.peak_db[bar] = ballistic_step_dt(self.peak_db[bar], peaks[bar], dt);
            self.level_db[bar] = ballistic_step_dt(self.level_db[bar], rms[bar], dt);
            self.crest_db[bar] = if rms[bar] > METER_MIN_DB + 1.0 {
                peaks[bar] - rms[bar]
            } else {
                0.0
            };
        }

        self.correlation = if reading.correlation.is_finite() {
            reading.correlation.clamp(-1.0, 1.0)
        } else {
            0.0
        };

        // The LED follows the channels, whatever the bars show.
        if reading.peak_db.iter().any(|&p| p >= METER_CLIP_DB) {
            self.clip_until = Some(now + Duration::from_secs_f32(METER_CLIP_HOLD_SECS));
        }
    }

    fn pause(&mut self, now: Instant) {
        self.crest_db = [0.0; 2];
        self.correlation = 0.0;
        if self.fade.is_none() && !self.is_fading_out {
            let audible = self
                .peak_db
                .iter()
                .chain(self.level_db.iter())
                .any(|&db| db > METER_MIN_DB);
            if !audible {
                return;
            }
            self.fade = Some(Fade {
                start: now,
                from_peak_db: self.peak_db,
                from_level_db: self.level_db,
            });
            self.is_fading_out = true;
        }
        let Some(fade) = self.fade else { return };
        let elapsed = now.saturating_duration_since(fade.start).as_secs_f32();
        for bar in 0..2 {
            self.peak_db[bar] = faded_db(fade.from_peak_db[bar], elapsed);
            self.level_db[bar] = faded_db(fade.from_level_db[bar], elapsed);
        }
        if elapsed >= METER_FADE_OUT_SECS {
            self.is_fading_out = false;
            self.fade = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TRACK: &str = "/music/a.flac";

    fn reading(peak: f32, rms: f32) -> MeterReading<'static> {
        MeterReading {
            track_path: TRACK,
            is_playing: true,
            infinite_hold: false,
            peak_l: peak,
            peak_r: peak,
            rms_l: rms,
            rms_r: rms,
        }
    }

    fn started(t0: Instant) -> MeterState {
        let mut m = MeterState::default();
        m.update(&reading(-20.0, -30.0), t0);
        m
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_new_track_resets_the_state_but_keeps_the_display_options() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.update(&reading(-6.0, -12.0), t0 + ms(30));
        m.ms_mode = true;
        m.show_numeric = false;

        let other = MeterReading {
            track_path: "/music/b.flac",
            ..reading(-3.0, -9.0)
        };
        m.update(&other, t0 + ms(60));
        assert_eq!(m.last_track_path, "/music/b.flac");
        assert!(m.ms_mode);
        assert!(!m.show_numeric);
        assert_eq!(m.display_l, 0.0);
        assert_eq!(m.peak_hold_l, 0.0);
        assert_eq!(m.crest_l, 0.0);
    }

    #[test]
    fn the_first_reading_of_a_track_only_resets() {
        let t0 = Instant::now();
        let m = started(t0);
        assert_eq!(m.last_track_path, TRACK);
        assert_eq!(m.display_l, 0.0);
        assert_eq!(m.phase_corr, 0.0);
    }

    #[test]
    fn a_louder_peak_snaps_the_hold_and_starts_holding() {
        let t0 = Instant::now();
        let (hold, start, decay) = peak_hold_step(-20.0, None, 0.0, -10.0, t0);
        assert_eq!(hold, -10.0);
        assert_eq!(start, Some(t0));
        assert_eq!(decay, 0.0);
    }

    #[test]
    fn a_hold_is_kept_during_the_hold_time() {
        let t0 = Instant::now();
        let (hold, start, decay) = peak_hold_step(-10.0, Some(t0), 0.0, -30.0, t0 + ms(1500));
        assert_eq!(hold, -10.0);
        assert_eq!(start, Some(t0));
        assert_eq!(decay, 0.0);
    }

    #[test]
    fn after_the_hold_time_a_positive_hold_decays_by_a_fixed_step() {
        let t0 = Instant::now();
        let (hold, start, decay) = peak_hold_step(2.0, Some(t0), 0.0, -30.0, t0 + ms(1600));
        assert_eq!(start, None);
        assert!((decay - (2.0 - 1.8 * 0.030)).abs() < 1e-6);
        // Still decaying: the hold value itself is left alone.
        assert_eq!(hold, 2.0);
    }

    #[test]
    fn a_decay_that_reaches_the_signal_returns_to_idle() {
        let (hold, start, decay) = peak_hold_step(0.5, None, 0.03, 0.0, Instant::now());
        assert_eq!(start, None);
        assert_eq!(decay, 0.0);
        assert_eq!(hold, 0.0);
    }

    #[test]
    fn a_negative_dbfs_hold_never_decays_after_the_hold_time() {
        // The decay branch is gated on a positive level, so a dBFS hold stays.
        let t0 = Instant::now();
        let (hold, start, decay) = peak_hold_step(-10.0, Some(t0), 0.0, -30.0, t0 + ms(2000));
        assert_eq!(start, None);
        assert_eq!(decay, -10.0);
        assert_eq!(hold, -10.0);
    }

    #[test]
    fn idle_hold_follows_the_signal_or_the_decay_level() {
        // Idle with no decay level: max(signal, 0.0), so a dBFS signal yields 0.
        let (hold, _, _) = peak_hold_step(-5.0, None, 0.0, -20.0, Instant::now());
        assert_eq!(hold, 0.0);
        let (hold, _, _) = peak_hold_step(5.0, None, 0.0, 3.0, Instant::now());
        assert_eq!(hold, 3.0);
    }

    #[test]
    fn ballistics_rise_fast_and_fall_slowly() {
        assert!((ballistic_step(0.0, 10.0) - 8.5).abs() < 1e-6);
        assert!((ballistic_step(10.0, 0.0) - 9.5).abs() < 1e-6);
        assert_eq!(ballistic_step(4.0, 4.0), 4.0);
    }

    #[test]
    fn playing_moves_the_bars_towards_the_held_peak() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.reset_peaks();
        m.update(&reading(-12.0, -20.0), t0 + ms(30));
        assert_eq!(m.peak_hold_l, -12.0);
        assert!((m.display_l - (0.0 + 0.05 * (-12.0 - 0.0))).abs() < 1e-6);
        assert_eq!(m.display_l, m.display_r);
    }

    #[test]
    fn infinite_hold_only_rises() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.reset_peaks();
        let mut r = reading(-12.0, -20.0);
        r.infinite_hold = true;
        m.update(&r, t0 + ms(30));
        assert_eq!(m.peak_hold_l, -12.0);
        r.peak_l = -40.0;
        m.update(&r, t0 + ms(5000));
        assert_eq!(m.peak_hold_l, -12.0);
        assert_eq!(m.peak_hold_start_l, None);
        assert_eq!(m.peak_decay_l, 0.0);
        r.peak_l = -3.0;
        m.update(&r, t0 + ms(5030));
        assert_eq!(m.peak_hold_l, -3.0);
    }

    #[test]
    fn mid_side_mode_feeds_mid_and_side_to_the_holds() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.ms_mode = true;
        m.reset_peaks();
        let r = MeterReading {
            peak_l: -10.0,
            peak_r: -20.0,
            ..reading(0.0, -30.0)
        };
        m.update(&r, t0 + ms(30));
        assert_eq!(m.peak_hold_l, -15.0); // Mid
        assert_eq!(m.peak_hold_r, 5.0); // Side
    }

    #[test]
    fn phase_correlation_is_the_sign_product_and_zero_for_silence() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.update(&reading(-6.0, -12.0), t0 + ms(30));
        assert_eq!(m.phase_corr, 1.0);
        let r = MeterReading {
            rms_l: 3.0,
            ..reading(-6.0, -12.0)
        };
        m.update(&r, t0 + ms(60));
        assert_eq!(m.phase_corr, -1.0);
        m.update(&reading(-6.0, 0.0), t0 + ms(90));
        assert_eq!(m.phase_corr, 0.0);
    }

    #[test]
    fn crest_factor_is_peak_minus_rms_above_the_floor() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.update(&reading(-6.0, -18.0), t0 + ms(30));
        assert_eq!(m.crest_l, 12.0);
        assert_eq!(m.crest_r, 12.0);
        m.update(&reading(-50.0, -59.5), t0 + ms(60));
        assert_eq!(m.crest_l, 0.0);
    }

    #[test]
    fn pausing_captures_the_levels_and_fades_them_out() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.display_l = -20.0;
        m.display_r = -10.0;
        let mut r = reading(-6.0, -12.0);
        r.is_playing = false;

        let pause = t0 + ms(100);
        m.update(&r, pause);
        assert!(m.is_fading_out);
        assert_eq!(m.fade_out_start, Some(pause));
        assert_eq!(m.last_playing_l, -20.0);
        assert_eq!(m.display_l, -20.0);

        m.update(&r, pause + ms(200));
        assert!((m.display_l - -10.0).abs() < 1e-4);
        assert!((m.display_r - -5.0).abs() < 1e-4);
        assert!(m.is_animating(false));

        m.update(&r, pause + ms(400));
        assert!(!m.is_fading_out);
        assert_eq!(m.display_l, 0.0);
        assert_eq!(m.display_r, 0.0);
        assert!(!m.is_animating(false));
    }

    #[test]
    fn resuming_stops_the_fade() {
        let t0 = Instant::now();
        let mut m = started(t0);
        let mut r = reading(-6.0, -12.0);
        r.is_playing = false;
        m.update(&r, t0 + ms(30));
        assert!(m.is_fading_out);
        r.is_playing = true;
        m.update(&r, t0 + ms(60));
        assert!(!m.is_fading_out);
    }

    #[test]
    fn resetting_peaks_drops_them_to_the_floor() {
        let t0 = Instant::now();
        let mut m = started(t0);
        m.peak_hold_start_l = Some(t0);
        m.peak_decay_r = 3.0;
        m.reset_peaks();
        assert_eq!(m.peak_hold_l, METER_MIN_DB);
        assert_eq!(m.peak_hold_r, METER_MIN_DB);
        assert_eq!(m.peak_hold_start_l, None);
        assert_eq!(m.peak_decay_r, 0.0);
    }
}

#[cfg(test)]
mod live_tests {
    use super::*;

    const TRACK: &str = "/music/a.flac";

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn playing(peak: f32, rms: f32) -> LiveReading<'static> {
        LiveReading {
            track_path: TRACK,
            is_playing: true,
            peak_db: [peak; 2],
            rms_db: [rms; 2],
            mid_side_peak_db: [peak; 2],
            mid_side_rms_db: [rms; 2],
            ..LiveReading::default()
        }
    }

    fn paused() -> LiveReading<'static> {
        LiveReading {
            is_playing: false,
            ..playing(METER_MIN_DB, METER_MIN_DB)
        }
    }

    /// A meter that has seen its track and been running for `settle` ticks of
    /// 30 ms on `reading`; returns it with the instant of its last update.
    fn running(reading: &LiveReading<'_>, ticks: u64) -> (LiveMeter, Instant) {
        let t0 = Instant::now();
        let mut meter = LiveMeter::default();
        meter.update(reading, t0);
        for tick in 1..=ticks {
            meter.update(reading, t0 + ms(30 * tick));
        }
        (meter, t0 + ms(30 * ticks))
    }

    // --- the pure functions ---

    #[test]
    fn silence_and_nonsense_readings_become_the_floor() {
        assert_eq!(sanitize_db(f32::NEG_INFINITY), METER_MIN_DB);
        assert_eq!(sanitize_db(f32::NAN), METER_MIN_DB);
        assert_eq!(sanitize_db(-200.0), METER_MIN_DB);
        assert_eq!(sanitize_db(-12.0), -12.0);
        assert_eq!(sanitize_db(30.0), METER_MAX_DB);
    }

    #[test]
    fn a_held_peak_stays_for_the_hold_time_then_falls_in_db_per_second() {
        assert_eq!(held_peak_db(-10.0, 0.0), -10.0);
        assert_eq!(held_peak_db(-10.0, 1.5), -10.0);
        // 10 s after the hold time: 18 dB lower. The Iced hold stays at -10.
        assert!((held_peak_db(-10.0, 11.5) - -28.0).abs() < 1e-4);
        assert_eq!(held_peak_db(-10.0, 1000.0), METER_MIN_DB);
    }

    #[test]
    fn the_iced_hold_never_falls_where_the_correct_one_does() {
        let t0 = Instant::now();
        let (old, ..) = peak_hold_step(-10.0, Some(t0), 0.0, -50.0, t0 + ms(11_500));
        let (old, ..) = peak_hold_step(old, None, -10.0, -50.0, t0 + ms(11_530));
        assert_eq!(old, -10.0);
        assert!(held_peak_db(-10.0, 11.5) < -20.0);
    }

    #[test]
    fn the_ballistics_give_the_same_bar_at_any_update_rate() {
        let mut fine = 0.0_f32;
        for _ in 0..10 {
            fine = ballistic_step_dt(fine, -30.0, 0.030);
        }
        let coarse = ballistic_step_dt(0.0, -30.0, 0.300);
        assert!((fine - coarse).abs() < 1e-3, "{fine} {coarse}");
        assert_eq!(ballistic_step_dt(-5.0, -30.0, 0.0), -5.0);
        // One 30 ms step is the Iced step.
        assert!((ballistic_step_dt(0.0, 10.0, 0.030) - ballistic_step(0.0, 10.0)).abs() < 1e-5);
        assert!((ballistic_step_dt(10.0, 0.0, 0.030) - ballistic_step(10.0, 0.0)).abs() < 1e-5);
    }

    #[test]
    fn the_fade_falls_to_the_floor_where_the_iced_fade_rises_to_full_scale() {
        assert_eq!(faded_db(-20.0, 0.0), -20.0);
        assert!((faded_db(-20.0, 0.2) - -40.0).abs() < 1e-4);
        assert_eq!(faded_db(-20.0, 0.4), METER_MIN_DB);
        assert_eq!(faded_db(-20.0, 5.0), METER_MIN_DB);
        // Iced: -20 dB × factor → 0 dBFS, the loudest the bar can be.
        let iced = -20.0_f32 * (1.0 - 0.3 / METER_FADE_OUT_SECS);
        assert!(iced > -20.0);
        assert!(faded_db(-20.0, 0.3) < -20.0);
    }

    #[test]
    fn identical_channels_correlate_at_plus_one_and_opposite_ones_at_minus_one() {
        // l == r: Σll = Σrr = Σlr.
        assert!((correlation_from_sums(2.0, 2.0, 2.0) - 1.0).abs() < 1e-6);
        assert!((correlation_from_sums(2.0, 2.0, -2.0) + 1.0).abs() < 1e-6);
        assert!(correlation_from_sums(2.0, 2.0, 0.0).abs() < 1e-6);
        assert_eq!(correlation_from_sums(0.0, 2.0, 0.0), 0.0);
        assert_eq!(correlation_from_sums(f64::NAN, 2.0, 1.0), 0.0);
    }

    #[test]
    fn a_hard_panned_signal_is_half_mid_and_half_side() {
        // l = 1, r = 0 for 4 pairs: mid = side = 0.5, mean square 0.25.
        let (mid, side) = mid_side_mean_squares(4.0, 0.0, 0.0, 4);
        assert!((mid - 0.25).abs() < 1e-12 && (side - 0.25).abs() < 1e-12);
        // Mono (l == r == 1): all mid.
        let (mid, side) = mid_side_mean_squares(4.0, 4.0, 4.0, 4);
        assert!((mid - 1.0).abs() < 1e-12 && side.abs() < 1e-12);
        // Out of phase: all side.
        let (mid, side) = mid_side_mean_squares(4.0, 4.0, -4.0, 4);
        assert!(mid.abs() < 1e-12 && (side - 1.0).abs() < 1e-12);
        assert_eq!(mid_side_mean_squares(1.0, 1.0, 1.0, 0), (0.0, 0.0));
        assert!((mean_square_to_db(0.25) - -6.0206).abs() < 1e-3);
        assert_eq!(mean_square_to_db(0.0), METER_MIN_DB);
    }

    // --- the peak hold ---

    #[test]
    fn a_louder_peak_raises_the_hold_and_restarts_its_age() {
        let mut hold = PeakHold::default();
        hold.update(-20.0, false, 0.03);
        assert_eq!(hold.value_db(), -20.0);
        hold.update(-30.0, false, 1.0);
        assert_eq!(hold.age_secs, 1.0);
        hold.update(-10.0, false, 0.03);
        assert_eq!((hold.raised_db, hold.age_secs), (-10.0, 0.0));
    }

    #[test]
    fn the_hold_follows_the_signal_once_it_has_fallen_below_it() {
        let mut hold = PeakHold {
            raised_db: -10.0,
            age_secs: 11.5,
        };
        assert!((hold.value_db() - -28.0).abs() < 1e-4);
        hold.update(-25.0, false, 0.0);
        assert_eq!((hold.raised_db, hold.age_secs), (-25.0, 0.0));
    }

    #[test]
    fn an_infinite_hold_never_ages_and_never_falls() {
        let mut hold = PeakHold::default();
        hold.update(-12.0, true, 0.03);
        for _ in 0..1000 {
            hold.update(-50.0, true, 0.5);
        }
        assert_eq!(hold.value_db(), -12.0);
        hold.update(-3.0, true, 0.03);
        assert_eq!(hold.value_db(), -3.0);
    }

    #[test]
    fn switching_infinite_hold_on_freezes_the_level_the_hold_had_reached() {
        let mut hold = PeakHold {
            raised_db: -10.0,
            age_secs: 11.5,
        };
        hold.update(-50.0, true, 0.5);
        assert!((hold.value_db() - -28.0).abs() < 1e-4);
        hold.update(-50.0, true, 5.0);
        assert!((hold.value_db() - -28.0).abs() < 1e-4);
    }

    // --- the meter ---

    #[test]
    fn the_first_reading_of_a_track_only_resets() {
        let t0 = Instant::now();
        let mut meter = LiveMeter::default();
        meter.update(&playing(-6.0, -12.0), t0);
        assert_eq!(meter.last_track_path, TRACK);
        assert_eq!(meter.level_db, [METER_MIN_DB; 2]);
        assert_eq!(meter.hold_db(), [METER_MIN_DB; 2]);
    }

    #[test]
    fn the_bar_level_is_the_rms_and_the_readout_the_peak() {
        let (meter, _) = running(&playing(-6.0, -18.0), 100);
        assert!((meter.level_db[0] - -18.0).abs() < 0.01, "{:?}", meter);
        assert!((meter.peak_db[0] - -6.0).abs() < 0.01);
        // The Iced UI passes `smooth_peak` (never updated, 0) as the RMS.
        assert!(meter.level_db[0] < -10.0);
        assert!((meter.crest_db[0] - 12.0).abs() < 0.01);
    }

    #[test]
    fn the_hold_marker_sits_at_the_peak_then_falls_after_the_hold_time() {
        let t0 = Instant::now();
        let mut meter = LiveMeter::default();
        meter.update(&playing(-6.0, -18.0), t0);
        meter.update(&playing(-6.0, -18.0), t0 + ms(30));
        assert_eq!(meter.hold_db()[0], -6.0);

        // The signal drops to -40 dB; 11.5 s later (in 0.25 s steps) the
        // marker is 18 dB lower than it was.
        let quiet = playing(-40.0, -50.0);
        let mut now = t0 + ms(30);
        for _ in 0..46 {
            now += ms(250);
            meter.update(&quiet, now);
        }
        let hold = meter.hold_db()[0];
        assert!(hold < -6.0 - 15.0 && hold > -6.0 - 20.0, "{hold}");
    }

    #[test]
    fn the_hold_marker_does_not_fall_while_paused() {
        let (mut meter, now) = running(&playing(-6.0, -18.0), 3);
        meter.update(&paused(), now + ms(30));
        meter.update(&paused(), now + ms(500));
        // A long pause, then playback resumes with a quiet signal.
        meter.update(&playing(-60.0, -60.0), now + ms(60_000));
        assert_eq!(meter.hold_db()[0], -6.0);
    }

    #[test]
    fn the_result_does_not_depend_on_the_tick_rate() {
        let t0 = Instant::now();
        let reading = playing(-6.0, -18.0);
        let (mut fast, mut slow) = (LiveMeter::default(), LiveMeter::default());
        fast.update(&reading, t0);
        slow.update(&reading, t0);
        for tick in 1..=100 {
            fast.update(&reading, t0 + ms(10 * tick));
        }
        for tick in 1..=10 {
            slow.update(&reading, t0 + ms(100 * tick));
        }
        assert!((fast.level_db[0] - slow.level_db[0]).abs() < 0.05);
        assert!((fast.peak_db[1] - slow.peak_db[1]).abs() < 0.05);
    }

    #[test]
    fn infinite_hold_keeps_the_marker_for_ever() {
        let reading = LiveReading {
            infinite_hold: true,
            ..playing(-6.0, -18.0)
        };
        let (mut meter, mut now) = running(&reading, 3);
        let quiet = LiveReading {
            infinite_hold: true,
            ..playing(-50.0, -60.0)
        };
        for _ in 0..200 {
            now += ms(250);
            meter.update(&quiet, now);
        }
        assert_eq!(meter.hold_db()[0], -6.0);
    }

    #[test]
    fn resetting_the_peaks_clears_the_markers_and_the_clip_led() {
        let (mut meter, _) = running(&playing(1.0, -18.0), 3);
        assert!(meter.clipping);
        meter.reset_peaks();
        assert_eq!(meter.hold_db(), [METER_MIN_DB; 2]);
        assert!(!meter.clipping);
    }

    #[test]
    fn pausing_fades_the_bars_down_to_silence_and_never_up() {
        let (mut meter, now) = running(&playing(-6.0, -18.0), 100);
        let (peak, level) = (meter.peak_db[0], meter.level_db[0]);
        let mut previous = (peak, level);
        meter.update(&paused(), now + ms(30));
        assert!(meter.is_fading_out && meter.is_animating(false));
        for step in 2..=14 {
            meter.update(&paused(), now + ms(30 * step));
            assert!(meter.peak_db[0] <= previous.0 + 1e-4, "peak rose");
            assert!(meter.level_db[0] <= previous.1 + 1e-4, "level rose");
            assert!(meter.peak_db[0] <= peak && meter.level_db[0] <= level);
            previous = (meter.peak_db[0], meter.level_db[0]);
        }
        meter.update(&paused(), now + ms(500));
        assert!(!meter.is_fading_out && !meter.is_animating(false));
        assert_eq!(meter.peak_db, [METER_MIN_DB; 2]);
        assert_eq!(meter.level_db, [METER_MIN_DB; 2]);
        assert_eq!(meter.crest_db, [0.0; 2]);
    }

    #[test]
    fn pausing_a_silent_meter_does_not_start_a_fade() {
        let t0 = Instant::now();
        let mut meter = LiveMeter::default();
        meter.update(&paused(), t0);
        meter.update(&paused(), t0 + ms(30));
        assert!(!meter.is_fading_out);
    }

    #[test]
    fn resuming_stops_the_fade() {
        let (mut meter, now) = running(&playing(-6.0, -18.0), 20);
        meter.update(&paused(), now + ms(30));
        assert!(meter.is_fading_out);
        meter.update(&playing(-6.0, -18.0), now + ms(60));
        assert!(!meter.is_fading_out);
    }

    #[test]
    fn a_new_track_resets_the_meter() {
        let (mut meter, now) = running(&playing(-6.0, -18.0), 20);
        let other = LiveReading {
            track_path: "/music/b.flac",
            ..playing(-3.0, -9.0)
        };
        meter.update(&other, now + ms(30));
        assert_eq!(meter.last_track_path, "/music/b.flac");
        assert_eq!(meter.level_db, [METER_MIN_DB; 2]);
        assert_eq!(meter.hold_db(), [METER_MIN_DB; 2]);
    }

    #[test]
    fn mid_side_mode_draws_the_linear_mid_and_side_levels() {
        let reading = LiveReading {
            mid_side: true,
            peak_db: [-6.0, -6.0],
            rms_db: [-9.0, -9.0],
            // A mono signal: all mid, no side.
            mid_side_peak_db: [-6.0, METER_MIN_DB],
            mid_side_rms_db: [-9.0, METER_MIN_DB],
            ..playing(0.0, 0.0)
        };
        let (meter, _) = running(&reading, 100);
        assert!((meter.level_db[0] - -9.0).abs() < 0.01);
        assert!(meter.level_db[1] < -59.0);
        assert!((meter.hold_db()[0] - -6.0).abs() < 1e-4);
        assert!(meter.hold_db()[1] <= METER_MIN_DB);
    }

    #[test]
    fn changing_the_mode_drops_the_holds() {
        let (mut meter, now) = running(&playing(-6.0, -18.0), 5);
        assert_eq!(meter.hold_db()[0], -6.0);
        let switched = LiveReading {
            mid_side: true,
            ..playing(-40.0, -50.0)
        };
        meter.update(&switched, now + ms(30));
        assert!(meter.hold_db()[0] <= -40.0);
    }

    #[test]
    fn the_correlation_is_the_decoders_linear_value() {
        let reading = LiveReading {
            correlation: -0.75,
            ..playing(-6.0, -18.0)
        };
        let (meter, _) = running(&reading, 3);
        // The Iced UI computes +1 or -1 from the sign of two dB values.
        assert_eq!(meter.correlation, -0.75);
        let nan = LiveReading {
            correlation: f32::NAN,
            ..reading
        };
        let (meter, _) = running(&nan, 3);
        assert_eq!(meter.correlation, 0.0);
    }

    #[test]
    fn the_clip_led_lights_on_a_full_scale_peak_and_stays_for_the_hold_time() {
        let t0 = Instant::now();
        let mut meter = LiveMeter::default();
        meter.update(&playing(-6.0, -18.0), t0);
        meter.update(&playing(0.0, -18.0), t0 + ms(30));
        assert!(meter.clipping);
        meter.update(&playing(-6.0, -18.0), t0 + ms(1000));
        assert!(meter.clipping, "still lit within the hold time");
        meter.update(&playing(-6.0, -18.0), t0 + ms(1600));
        assert!(!meter.clipping);
    }

    #[test]
    fn a_clip_in_one_channel_lights_the_led_even_in_mid_side_mode() {
        let reading = LiveReading {
            mid_side: true,
            peak_db: [3.0, -20.0],
            mid_side_peak_db: [-6.0, -6.0],
            ..playing(0.0, -18.0)
        };
        let (meter, _) = running(&reading, 2);
        assert!(meter.clipping);
    }

    #[test]
    fn silence_readings_from_an_unwritten_meter_are_the_floor() {
        let reading = LiveReading {
            peak_db: [f32::NEG_INFINITY; 2],
            rms_db: [f32::NEG_INFINITY; 2],
            ..playing(0.0, 0.0)
        };
        let (meter, _) = running(&reading, 10);
        assert_eq!(meter.level_db, [METER_MIN_DB; 2]);
        assert_eq!(meter.hold_db(), [METER_MIN_DB; 2]);
        assert!(!meter.clipping);
    }

    #[test]
    fn a_stalled_ui_does_not_jump_the_hold() {
        let (mut meter, now) = running(&playing(-6.0, -18.0), 3);
        // Ten minutes without an update, then the same signal: at most the
        // maximum step counts as elapsed time.
        meter.update(&playing(-60.0, -60.0), now + Duration::from_secs(600));
        assert_eq!(meter.hold_db()[0], -6.0);
    }
}
