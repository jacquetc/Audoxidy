//! The hold-to-repeat schedule of the stepper's arrows.
//!
//! The Iced `NumberStepper` steps once on the press, waits 0.4 s, then steps
//! every 0.1 s while the arrow is held (`src/gui/widgets.rs:273-277`,
//! `602-633`). Iced measures each repeat from the previous one on whatever
//! redraw comes next, so its cadence drifts with the frame rate. Here the
//! number of steps owed is a function of the time since the press alone, so a
//! late frame catches up instead of slipping, and the widget only needs to be
//! woken at [`HoldRepeat::next_deadline`].

use std::time::{Duration, Instant};

/// How long an arrow must be held before it repeats (`HOLD_ACTIVATE_SECS`).
pub const HOLD_DELAY: Duration = Duration::from_millis(400);
/// The time between two repeats (`HOLD_REPEAT_SECS`).
pub const HOLD_INTERVAL: Duration = Duration::from_millis(100);

/// How many repeat steps are due `now` for an arrow pressed at `press_at`,
/// not counting the step the press itself made: none before 0.4 s, then one
/// more every 0.1 s.
pub fn steps_due(press_at: Instant, now: Instant) -> u32 {
    let held = now.saturating_duration_since(press_at);
    if held < HOLD_DELAY {
        return 0;
    }
    let repeats = (held - HOLD_DELAY).as_nanos() / HOLD_INTERVAL.as_nanos();
    u32::try_from(repeats).map_or(u32::MAX, |n| n.saturating_add(1))
}

/// An arrow being held.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HoldRepeat {
    /// The direction: `-1` for the minus arrow, `1` for the plus arrow.
    pub dir: i8,
    /// When the arrow was pressed.
    pub press_at: Instant,
    /// How many repeat steps have been made so far.
    pub repeats_done: u32,
}

impl HoldRepeat {
    /// An arrow pressed at `press_at` (the press's own step already made).
    pub fn new(dir: i8, press_at: Instant) -> Self {
        Self {
            dir,
            press_at,
            repeats_done: 0,
        }
    }

    /// The number of steps to make `now`, which are then counted as done.
    pub fn advance(&mut self, now: Instant) -> u32 {
        let due = steps_due(self.press_at, now);
        let owed = due.saturating_sub(self.repeats_done);
        self.repeats_done = self.repeats_done.max(due);
        owed
    }

    /// When the next repeat falls due.
    pub fn next_deadline(&self) -> Instant {
        self.press_at + HOLD_DELAY + HOLD_INTERVAL * self.repeats_done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn nothing_repeats_during_the_first_four_tenths_of_a_second() {
        let t0 = Instant::now();
        assert_eq!(steps_due(t0, t0), 0);
        assert_eq!(steps_due(t0, t0 + ms(399)), 0);
    }

    #[test]
    fn then_one_step_falls_due_every_tenth_of_a_second() {
        let t0 = Instant::now();
        assert_eq!(steps_due(t0, t0 + ms(400)), 1);
        assert_eq!(steps_due(t0, t0 + ms(499)), 1);
        assert_eq!(steps_due(t0, t0 + ms(500)), 2);
        assert_eq!(steps_due(t0, t0 + ms(1400)), 11);
    }

    #[test]
    fn a_clock_before_the_press_owes_nothing() {
        let t0 = Instant::now() + ms(1000);
        assert_eq!(steps_due(t0, t0 - ms(500)), 0);
    }

    #[test]
    fn advancing_counts_the_steps_owed_since_the_last_advance() {
        let t0 = Instant::now();
        let mut hold = HoldRepeat::new(1, t0);
        assert_eq!(hold.next_deadline(), t0 + ms(400));
        assert_eq!(hold.advance(t0 + ms(200)), 0);
        assert_eq!(hold.advance(t0 + ms(410)), 1);
        assert_eq!(hold.next_deadline(), t0 + ms(500));
        assert_eq!(hold.advance(t0 + ms(450)), 0);
        // A late frame catches up rather than slipping.
        assert_eq!(hold.advance(t0 + ms(760)), 3);
        assert_eq!(hold.next_deadline(), t0 + ms(800));
    }
}
