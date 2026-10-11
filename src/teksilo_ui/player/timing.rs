//! The player's deadlines, as pure state: when the volume overlay hides and
//! when the transport controls go quiet.
//!
//! The Iced UI runs these as `tokio::time::sleep` tasks, one per wheel step or
//! mouse move, each carrying a tick id so a stale one is ignored
//! (`src/gui/app.rs:1342-1356`, `4606-4638`). Here each is one deadline, a
//! plain [`Instant`] the view-model hands to the window's one-shot wake-up
//! (`BuildContext::wake_at_handle`) and checks from a frame-tick effect. The
//! state machines below take the time as an argument and never read the
//! clock, so the arithmetic is tested without waiting.
//!
//! - [`VolumeOverlay`]: the overlay shows the volume in percent after a wheel
//!   step. It hides 1500 ms after the last step, 1 s after the first mouse
//!   move that follows a step (a later move does not push it back), and at
//!   once on a click (`PlayerScroll`, `wake_up_controls`).
//! - [`Activity`]: the hover feedback of the three control zones and the seek
//!   tooltip are shown only while the user has been active
//!   (`PlayerUiState::is_active`). Iced counts 3 half-second ticks; this is
//!   1500 ms after the last activity.

use std::time::{Duration, Instant};

/// How long the volume overlay stays after a wheel step (`app.rs:4630`).
pub const VOLUME_AFTER_WHEEL: Duration = Duration::from_millis(1500);
/// How long it stays after a mouse move that follows a step (`app.rs:1157`).
pub const VOLUME_AFTER_MOVE: Duration = Duration::from_millis(1000);
/// How long the controls stay active after the last activity (`app.rs:1134`:
/// three ticks of 500 ms).
pub const CONTROLS_ACTIVE_FOR: Duration = Duration::from_millis(1500);

/// The earlier of `current` (if any) and `candidate`: what to keep when two
/// parties want the window's single wake-up.
pub fn earlier(current: Option<Instant>, candidate: Instant) -> Instant {
    match current {
        Some(current) if current <= candidate => current,
        _ => candidate,
    }
}

/// The earlier of two optional deadlines.
pub fn earliest(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Whether `deadline` has come at `now`.
pub fn is_due(deadline: Option<Instant>, now: Instant) -> bool {
    deadline.is_some_and(|at| at <= now)
}

/// The volume overlay's state; see the module documentation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct VolumeOverlay {
    shown: Option<f32>,
    clearing: bool,
    deadline: Option<Instant>,
}

impl VolumeOverlay {
    /// The volume being shown, in percent, if the overlay is up.
    pub fn shown(&self) -> Option<f32> {
        self.shown
    }

    /// When the overlay hides, if it is up.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// A wheel step set the volume to `percent` at `now`: the overlay shows it
    /// and its timeout restarts from here, which also cancels the shorter one
    /// a mouse move may have started (Iced's new tick id).
    pub fn wheel(&mut self, now: Instant, percent: f32) {
        self.shown = Some(percent);
        self.clearing = false;
        self.deadline = Some(now + VOLUME_AFTER_WHEEL);
    }

    /// The mouse moved at `now`. While the overlay is up and no move has yet
    /// started its short timeout, one starts; the earlier of it and the wheel
    /// timeout holds. Returns whether the deadline changed.
    pub fn pointer_moved(&mut self, now: Instant) -> bool {
        if self.shown.is_none() || self.clearing {
            return false;
        }
        self.clearing = true;
        let at = earlier(self.deadline, now + VOLUME_AFTER_MOVE);
        let changed = self.deadline != Some(at);
        self.deadline = Some(at);
        changed
    }

    /// A click or a transport action: the overlay goes at once. Returns
    /// whether it was up.
    pub fn dismiss(&mut self) -> bool {
        let was_up = self.shown.is_some();
        *self = Self::default();
        was_up
    }

    /// Hides the overlay if its deadline has come. Returns whether it did.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self.shown.is_some() && is_due(self.deadline, now) {
            *self = Self::default();
            true
        } else {
            false
        }
    }
}

/// Whether the user has been active lately; see the module documentation.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Activity {
    active: bool,
    deadline: Option<Instant>,
}

impl Activity {
    /// Whether the controls are active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// When the controls go quiet, if they are active.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// The user did something at `now`: active until [`CONTROLS_ACTIVE_FOR`]
    /// after it. Returns whether it was inactive before.
    pub fn touch(&mut self, now: Instant) -> bool {
        let woke = !self.active;
        self.active = true;
        self.deadline = Some(now + CONTROLS_ACTIVE_FOR);
        woke
    }

    /// Goes quiet if the deadline has come. Returns whether it did.
    pub fn expire(&mut self, now: Instant) -> bool {
        if self.active && is_due(self.deadline, now) {
            *self = Self::default();
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64, origin: Instant) -> Instant {
        origin + Duration::from_millis(ms)
    }

    #[test]
    fn a_wheel_step_shows_the_volume_for_1500_ms() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        overlay.wheel(t0, 55.0);
        assert_eq!(overlay.shown(), Some(55.0));
        assert_eq!(overlay.deadline(), Some(at(1500, t0)));
        assert!(!overlay.expire(at(1499, t0)));
        assert!(overlay.expire(at(1500, t0)));
        assert_eq!(overlay.shown(), None);
        assert_eq!(overlay.deadline(), None);
    }

    #[test]
    fn each_wheel_step_restarts_the_timeout() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        overlay.wheel(t0, 55.0);
        overlay.wheel(at(1000, t0), 60.0);
        assert_eq!(overlay.shown(), Some(60.0));
        assert!(
            !overlay.expire(at(2000, t0)),
            "the first step's timeout is void"
        );
        assert!(overlay.expire(at(2500, t0)));
    }

    #[test]
    fn a_mouse_move_shortens_the_timeout_to_one_second_from_the_move() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        overlay.wheel(t0, 55.0);
        assert!(overlay.pointer_moved(at(100, t0)));
        assert_eq!(overlay.deadline(), Some(at(1100, t0)));
        assert!(overlay.expire(at(1100, t0)));
    }

    #[test]
    fn only_the_first_move_after_a_step_starts_the_short_timeout() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        overlay.wheel(t0, 55.0);
        overlay.pointer_moved(at(100, t0));
        assert!(
            !overlay.pointer_moved(at(600, t0)),
            "a later move changes nothing"
        );
        assert_eq!(overlay.deadline(), Some(at(1100, t0)));
    }

    #[test]
    fn a_move_late_in_the_wheel_timeout_keeps_the_earlier_wheel_deadline() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        overlay.wheel(t0, 55.0);
        assert!(
            !overlay.pointer_moved(at(900, t0)),
            "1900 ms is after 1500 ms"
        );
        assert_eq!(overlay.deadline(), Some(at(1500, t0)));
    }

    #[test]
    fn a_wheel_step_after_a_move_starts_over() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        overlay.wheel(t0, 55.0);
        overlay.pointer_moved(at(100, t0));
        overlay.wheel(at(200, t0), 50.0);
        assert_eq!(overlay.deadline(), Some(at(1700, t0)));
        assert!(
            overlay.pointer_moved(at(300, t0)),
            "the move may shorten it again"
        );
        assert_eq!(overlay.deadline(), Some(at(1300, t0)));
    }

    #[test]
    fn a_move_with_no_overlay_does_nothing() {
        let mut overlay = VolumeOverlay::default();
        assert!(!overlay.pointer_moved(Instant::now()));
        assert_eq!(overlay, VolumeOverlay::default());
    }

    #[test]
    fn a_click_hides_the_overlay_at_once() {
        let t0 = Instant::now();
        let mut overlay = VolumeOverlay::default();
        assert!(!overlay.dismiss());
        overlay.wheel(t0, 55.0);
        assert!(overlay.dismiss());
        assert_eq!(overlay.shown(), None);
        assert_eq!(overlay.deadline(), None);
    }

    #[test]
    fn the_controls_are_active_for_1500_ms_after_the_last_activity() {
        let t0 = Instant::now();
        let mut activity = Activity::default();
        assert!(!activity.is_active());
        assert!(activity.touch(t0), "the first touch wakes them");
        assert!(!activity.touch(at(1000, t0)), "a second one does not");
        assert_eq!(activity.deadline(), Some(at(2500, t0)));
        assert!(!activity.expire(at(2499, t0)));
        assert!(activity.expire(at(2500, t0)));
        assert!(!activity.is_active());
        assert!(!activity.expire(at(9000, t0)), "going quiet happens once");
    }

    #[test]
    fn the_earlier_of_two_deadlines_holds() {
        let t0 = Instant::now();
        assert_eq!(earlier(None, at(5, t0)), at(5, t0));
        assert_eq!(earlier(Some(at(3, t0)), at(5, t0)), at(3, t0));
        assert_eq!(earlier(Some(at(9, t0)), at(5, t0)), at(5, t0));
        assert_eq!(earliest(None, None), None);
        assert_eq!(earliest(Some(at(3, t0)), None), Some(at(3, t0)));
        assert_eq!(earliest(Some(at(3, t0)), Some(at(2, t0))), Some(at(2, t0)));
    }

    #[test]
    fn a_missing_deadline_is_never_due() {
        assert!(!is_due(None, Instant::now()));
    }
}
