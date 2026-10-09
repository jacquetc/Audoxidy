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
