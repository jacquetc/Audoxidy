use std::sync::atomic::{AtomicU32, Ordering};

/// Lock-free atomic meter data for real-time peak and RMS levels.
///
/// Transfer mechanism from decoder thread to GUI: 4 `AtomicU32` fields store
/// f32 dBFS values bit-reinterpreted via `f32::to_bits()` / `f32::from_bits()`.
/// The decoder thread writes with `Ordering::Relaxed` (single-writer);
/// the GUI reads with `Ordering::Relaxed` (single-reader, stale OK at ~30ms).
///
/// No allocations on the hot path — this struct is purely stack-allocated atomic stores.
///
/// Besides the four Iced-era levels it carries the linear stereo statistics
/// the Teksilo meter needs ([`StereoReading`]: phase correlation, mid and side
/// RMS and peaks), written by [`MeterData::write_stereo`]. The Iced UI ignores
/// them.
pub struct MeterData {
    /// Peak level for left channel in dBFS
    peak_l: AtomicU32,
    /// RMS level for left channel in dBFS
    rms_l: AtomicU32,
    /// Peak level for right channel in dBFS
    peak_r: AtomicU32,
    /// RMS level for right channel in dBFS
    rms_r: AtomicU32,
    /// Phase correlation of the RMS window, `-1.0..=1.0` (`0.0` for silence)
    correlation: AtomicU32,
    /// RMS level of Mid `(L+R)/2` in dBFS
    mid_rms: AtomicU32,
    /// RMS level of Side `(L-R)/2` in dBFS
    side_rms: AtomicU32,
    /// Peak level of Mid in dBFS
    mid_peak: AtomicU32,
    /// Peak level of Side in dBFS
    side_peak: AtomicU32,
}

/// Lowest level the decoder publishes (dBFS), the floor of the meter.
const FLOOR_DB: f32 = -60.0;
/// Highest level the decoder publishes (dBFS).
const CEILING_DB: f32 = 6.0;

/// A linear amplitude (`0.0..`) as dBFS, clamped to the published range;
/// silence is the floor. The conversion the decoder applies to the peaks.
pub fn linear_to_db(linear: f64) -> f32 {
    if linear > 0.0 {
        (20.0 * linear.log10()).clamp(FLOOR_DB as f64, CEILING_DB as f64) as f32
    } else {
        FLOOR_DB
    }
}

/// The stereo statistics of one batch, computed from linear samples.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StereoReading {
    /// Phase correlation of the RMS window, `-1.0..=1.0`.
    pub correlation: f32,
    /// RMS of Mid `(L+R)/2` over the window (dBFS).
    pub mid_rms_db: f32,
    /// RMS of Side `(L-R)/2` over the window (dBFS).
    pub side_rms_db: f32,
    /// Peak of Mid in the batch (dBFS).
    pub mid_peak_db: f32,
    /// Peak of Side in the batch (dBFS).
    pub side_peak_db: f32,
}

impl StereoReading {
    /// Silence: no correlation, every level at the floor.
    pub const SILENCE: Self = Self {
        correlation: 0.0,
        mid_rms_db: FLOOR_DB,
        side_rms_db: FLOOR_DB,
        mid_peak_db: FLOOR_DB,
        side_peak_db: FLOOR_DB,
    };
}

/// Accumulator of the linear stereo statistics, filled by the decoder's
/// metering loop one frame at a time.
///
/// It holds a handful of `f64` sums: no allocation, no lock, nothing but
/// arithmetic, so it is safe on the decoder's hot path. The sums of `l²`,
/// `r²` and `l·r` over the RMS window give the correlation and the mid and
/// side RMS (`crate::logic::meter`); the mid and side peaks are per batch.
///
/// Lifecycle, as the decoder uses it: [`add_frame`](Self::add_frame) for every
/// frame of a batch, [`reading`](Self::reading) after the batch,
/// [`end_batch`](Self::end_batch) to start the next batch's peaks, and
/// [`reset_window`](Self::reset_window) when the RMS window expires.
#[derive(Debug, Clone, Copy, Default)]
pub struct StereoStats {
    sum_ll: f64,
    sum_rr: f64,
    sum_lr: f64,
    frames: u64,
    mid_peak: f64,
    side_peak: f64,
}

impl StereoStats {
    /// An empty accumulator.
    pub const fn new() -> Self {
        Self {
            sum_ll: 0.0,
            sum_rr: 0.0,
            sum_lr: 0.0,
            frames: 0,
            mid_peak: 0.0,
            side_peak: 0.0,
        }
    }

    /// Accumulates one stereo frame of linear samples.
    #[inline]
    pub fn add_frame(&mut self, l: f64, r: f64) {
        self.sum_ll += l * l;
        self.sum_rr += r * r;
        self.sum_lr += l * r;
        self.frames += 1;
        let mid = ((l + r) * 0.5).abs();
        let side = ((l - r) * 0.5).abs();
        if mid > self.mid_peak {
            self.mid_peak = mid;
        }
        if side > self.side_peak {
            self.side_peak = side;
        }
    }

    /// The statistics so far: the window's correlation and RMS, the batch's
    /// peaks.
    pub fn reading(&self) -> StereoReading {
        use crate::logic::meter::{
            correlation_from_sums, mean_square_to_db, mid_side_mean_squares,
        };
        let (mid, side) = mid_side_mean_squares(self.sum_ll, self.sum_rr, self.sum_lr, self.frames);
        StereoReading {
            correlation: correlation_from_sums(self.sum_ll, self.sum_rr, self.sum_lr),
            mid_rms_db: mean_square_to_db(mid),
            side_rms_db: mean_square_to_db(side),
            mid_peak_db: linear_to_db(self.mid_peak),
            side_peak_db: linear_to_db(self.side_peak),
        }
    }

    /// Starts a new batch: the peaks are cleared, the window's sums stay.
    pub fn end_batch(&mut self) {
        self.mid_peak = 0.0;
        self.side_peak = 0.0;
    }

    /// Starts a new RMS window: the sums are cleared.
    pub fn reset_window(&mut self) {
        self.sum_ll = 0.0;
        self.sum_rr = 0.0;
        self.sum_lr = 0.0;
        self.frames = 0;
    }
}

impl MeterData {
    /// Creates a new `MeterData` with all fields initialized to silence (`-inf` dBFS).
    pub fn new() -> Self {
        Self {
            peak_l: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            rms_l: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            peak_r: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            rms_r: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            correlation: AtomicU32::new(0.0_f32.to_bits()),
            mid_rms: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            side_rms: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            mid_peak: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
            side_peak: AtomicU32::new(f32::NEG_INFINITY.to_bits()),
        }
    }

    /// Stores peak and RMS values for both channels (dBFS, f32 via `to_bits`).
    ///
    /// This is the ONLY write method — called by the decoder thread after DSP
    /// processing, once per batch. Uses `Ordering::Relaxed` (no cross-field
    /// consistency needed — best-effort visual data).
    pub fn write_peak_rms(&self, peak_l_db: f32, rms_l_db: f32, peak_r_db: f32, rms_r_db: f32) {
        self.peak_l.store(peak_l_db.to_bits(), Ordering::Relaxed);
        self.rms_l.store(rms_l_db.to_bits(), Ordering::Relaxed);
        self.peak_r.store(peak_r_db.to_bits(), Ordering::Relaxed);
        self.rms_r.store(rms_r_db.to_bits(), Ordering::Relaxed);
    }

    /// Stores the linear stereo statistics of a batch, written by the decoder
    /// thread beside [`write_peak_rms`](Self::write_peak_rms) (same ordering,
    /// same single writer).
    pub fn write_stereo(&self, stats: &StereoReading) {
        self.correlation
            .store(stats.correlation.to_bits(), Ordering::Relaxed);
        self.mid_rms
            .store(stats.mid_rms_db.to_bits(), Ordering::Relaxed);
        self.side_rms
            .store(stats.side_rms_db.to_bits(), Ordering::Relaxed);
        self.mid_peak
            .store(stats.mid_peak_db.to_bits(), Ordering::Relaxed);
        self.side_peak
            .store(stats.side_peak_db.to_bits(), Ordering::Relaxed);
    }

    /// Reads the phase correlation, `-1.0..=1.0` (`0.0` before any write).
    pub fn read_correlation(&self) -> f32 {
        f32::from_bits(self.correlation.load(Ordering::Relaxed))
    }

    /// Reads the RMS level of Mid in dBFS.
    pub fn read_mid_rms(&self) -> f32 {
        f32::from_bits(self.mid_rms.load(Ordering::Relaxed))
    }

    /// Reads the RMS level of Side in dBFS.
    pub fn read_side_rms(&self) -> f32 {
        f32::from_bits(self.side_rms.load(Ordering::Relaxed))
    }

    /// Reads the peak level of Mid in dBFS.
    pub fn read_mid_peak(&self) -> f32 {
        f32::from_bits(self.mid_peak.load(Ordering::Relaxed))
    }

    /// Reads the peak level of Side in dBFS.
    pub fn read_side_peak(&self) -> f32 {
        f32::from_bits(self.side_peak.load(Ordering::Relaxed))
    }

    /// Reads the peak level for the left channel in dBFS.
    pub fn read_peak_l(&self) -> f32 {
        f32::from_bits(self.peak_l.load(Ordering::Relaxed))
    }

    /// Reads the RMS level for the left channel in dBFS.
    pub fn read_rms_l(&self) -> f32 {
        f32::from_bits(self.rms_l.load(Ordering::Relaxed))
    }

    /// Reads the peak level for the right channel in dBFS.
    pub fn read_peak_r(&self) -> f32 {
        f32::from_bits(self.peak_r.load(Ordering::Relaxed))
    }

    /// Reads the RMS level for the right channel in dBFS.
    pub fn read_rms_r(&self) -> f32 {
        f32::from_bits(self.rms_r.load(Ordering::Relaxed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_silence_initial_value() {
        let m = MeterData::new();
        assert_eq!(m.read_peak_l(), f32::NEG_INFINITY);
        assert_eq!(m.read_rms_l(), f32::NEG_INFINITY);
        assert_eq!(m.read_peak_r(), f32::NEG_INFINITY);
        assert_eq!(m.read_rms_r(), f32::NEG_INFINITY);
    }

    #[test]
    fn test_atomic_roundtrip() {
        let m = MeterData::new();
        m.write_peak_rms(
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
        );
        assert_eq!(m.read_peak_l(), f32::NEG_INFINITY);
    }

    #[test]
    fn test_positive_value_roundtrip() {
        let m = MeterData::new();
        m.write_peak_rms(0.5, -10.0, -20.0, -30.0);
        assert_eq!(m.read_peak_l(), 0.5);
    }

    #[test]
    fn test_zero_roundtrip() {
        let m = MeterData::new();
        m.write_peak_rms(0.0, 0.0, 0.0, 0.0);
        assert_eq!(m.read_peak_l(), 0.0);
        assert_eq!(m.read_rms_r(), 0.0);
    }

    #[test]
    fn test_clipping_value_roundtrip() {
        let m = MeterData::new();
        m.write_peak_rms(-6.0, -3.0, 0.0, 6.0);
        assert_eq!(m.read_peak_l(), -6.0);
        assert_eq!(m.read_rms_l(), -3.0);
        assert_eq!(m.read_peak_r(), 0.0);
        assert_eq!(m.read_rms_r(), 6.0);
    }

    // --- the linear stereo statistics ---

    /// Counts the allocations made by the current thread.
    struct CountingAlloc;

    thread_local! {
        static ALLOCATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    // SAFETY: forwards to the system allocator; the counter is a const-init
    // thread-local `Cell`, which neither allocates nor needs a destructor.
    unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            unsafe { std::alloc::System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
            unsafe { std::alloc::System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, size: usize) -> *mut u8 {
            let _ = ALLOCATIONS.try_with(|count| count.set(count.get() + 1));
            unsafe { std::alloc::System.realloc(ptr, layout, size) }
        }
    }

    #[global_allocator]
    static COUNTING: CountingAlloc = CountingAlloc;

    fn allocations() -> usize {
        ALLOCATIONS.with(|count| count.get())
    }

    /// The stats of `frames` accumulated one by one, the way the decoder does.
    fn stats_of(frames: &[(f64, f64)]) -> StereoStats {
        let mut stats = StereoStats::new();
        for &(l, r) in frames {
            stats.add_frame(l, r);
        }
        stats
    }

    fn sine(n: usize, phase_r: f64) -> Vec<(f64, f64)> {
        (0..n)
            .map(|i| {
                let t = i as f64 * 2.0 * std::f64::consts::PI * 440.0 / 48_000.0;
                (0.5 * t.sin(), 0.5 * (t + phase_r).sin())
            })
            .collect()
    }

    #[test]
    fn the_stereo_atomics_start_silent() {
        let m = MeterData::new();
        assert_eq!(m.read_correlation(), 0.0);
        assert_eq!(m.read_mid_rms(), f32::NEG_INFINITY);
        assert_eq!(m.read_side_rms(), f32::NEG_INFINITY);
        assert_eq!(m.read_mid_peak(), f32::NEG_INFINITY);
        assert_eq!(m.read_side_peak(), f32::NEG_INFINITY);
    }

    #[test]
    fn the_stereo_atomics_round_trip_and_leave_the_four_levels_alone() {
        let m = MeterData::new();
        m.write_peak_rms(-6.0, -12.0, -7.0, -13.0);
        m.write_stereo(&StereoReading {
            correlation: -0.5,
            mid_rms_db: -20.0,
            side_rms_db: -30.0,
            mid_peak_db: -10.0,
            side_peak_db: -15.0,
        });
        assert_eq!(m.read_correlation(), -0.5);
        assert_eq!(m.read_mid_rms(), -20.0);
        assert_eq!(m.read_side_rms(), -30.0);
        assert_eq!(m.read_mid_peak(), -10.0);
        assert_eq!(m.read_side_peak(), -15.0);
        assert_eq!(m.read_peak_l(), -6.0);
        assert_eq!(m.read_rms_r(), -13.0);
    }

    #[test]
    fn a_mono_signal_correlates_fully_and_is_all_mid() {
        let reading = stats_of(&sine(4800, 0.0)).reading();
        assert!((reading.correlation - 1.0).abs() < 1e-5);
        // 0.5 amplitude sine: RMS 0.3536 = -9.03 dBFS, all of it mid.
        assert!((reading.mid_rms_db - -9.03).abs() < 0.05, "{reading:?}");
        assert_eq!(reading.side_rms_db, -60.0);
        assert!((reading.mid_peak_db - -6.02).abs() < 0.05);
        assert_eq!(reading.side_peak_db, -60.0);
    }

    #[test]
    fn an_out_of_phase_signal_correlates_at_minus_one_and_is_all_side() {
        let reading = stats_of(&sine(4800, std::f64::consts::PI)).reading();
        assert!((reading.correlation + 1.0).abs() < 1e-5);
        assert_eq!(reading.mid_rms_db, -60.0);
        assert!((reading.side_rms_db - -9.03).abs() < 0.05);
    }

    #[test]
    fn a_quarter_period_offset_is_uncorrelated_and_splits_evenly() {
        let reading = stats_of(&sine(48_000, std::f64::consts::FRAC_PI_2)).reading();
        assert!(reading.correlation.abs() < 0.01, "{}", reading.correlation);
        assert!((reading.mid_rms_db - reading.side_rms_db).abs() < 0.05);
    }

    #[test]
    fn one_silent_channel_has_no_correlation_and_is_half_mid_half_side() {
        let frames: Vec<(f64, f64)> = sine(4800, 0.0).into_iter().map(|(l, _)| (l, 0.0)).collect();
        let reading = stats_of(&frames).reading();
        assert_eq!(reading.correlation, 0.0);
        assert!((reading.mid_rms_db - reading.side_rms_db).abs() < 0.01);
    }

    #[test]
    fn silence_is_the_floor_with_no_correlation() {
        let reading = stats_of(&[(0.0, 0.0); 100]).reading();
        assert_eq!(reading, StereoReading::SILENCE);
        assert_eq!(StereoStats::new().reading(), StereoReading::SILENCE);
    }

    #[test]
    fn peaks_are_per_batch_and_the_window_sums_survive_a_batch() {
        let mut stats = stats_of(&[(1.0, 1.0); 10]);
        assert!(stats.reading().mid_peak_db > -0.1);
        stats.end_batch();
        let after = stats.reading();
        assert_eq!(after.mid_peak_db, -60.0);
        assert!((after.correlation - 1.0).abs() < 1e-6);
        assert!(after.mid_rms_db > -0.1);
        stats.reset_window();
        assert_eq!(stats.reading(), StereoReading::SILENCE);
    }

    #[test]
    fn a_full_scale_overdrive_is_clamped_to_the_ceiling() {
        let reading = stats_of(&[(4.0, 4.0)]).reading();
        assert_eq!(reading.mid_peak_db, 6.0);
        assert_eq!(linear_to_db(0.0), -60.0);
        assert_eq!(linear_to_db(1.0), 0.0);
    }

    #[test]
    fn accumulating_and_reading_allocate_nothing() {
        let frames = sine(512, 0.3);
        let meter = MeterData::new();
        let mut stats = StereoStats::new();
        let before = allocations();
        for _ in 0..20 {
            for &(l, r) in &frames {
                stats.add_frame(l, r);
            }
            meter.write_stereo(&stats.reading());
            stats.end_batch();
        }
        stats.reset_window();
        assert_eq!(allocations(), before, "the metering loop allocated");
        // The counter does count: a Vec is seen.
        let probe = allocations();
        let v: Vec<u8> = Vec::with_capacity(64);
        assert!(allocations() > probe);
        drop(v);
    }
}
