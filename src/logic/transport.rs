//! Transport rules shared by every UI: seek clamping, fine-seek availability,
//! volume clamping, the garbage-collector window after a preload, and the
//! mapping of external (MPRIS/SMTC) control events to transport actions.

use crate::integrations::control::ExternalControlEvent;

/// Safety window (seconds) after a preload finishes during which the global
/// garbage collector must not run: the track transition is still consuming the
/// preloaded audio (drain plus crossfade), and a purge would cut it.
pub const PRELOAD_GC_SAFETY_SECS: u64 = 20;

/// Fixed jump (seconds) applied by `Seek` events that carry no offset.
/// `SeekBy` events carry their own offset and do not use this value.
pub const SEEK_STEP_SECS: f64 = 10.0;

/// Whether the global garbage collector is blocked by the preload: either the
/// preload is active, or it finished less than [`PRELOAD_GC_SAFETY_SECS`] ago.
///
/// `now_secs` and `preload_finished_at` are UNIX timestamps in seconds.
pub fn gc_blocked_by_preload(
    now_secs: u64,
    preload_active: bool,
    preload_finished_at: Option<u64>,
) -> bool {
    preload_active
        || preload_finished_at
            .map(|t| now_secs.saturating_sub(t) < PRELOAD_GC_SAFETY_SECS)
            .unwrap_or(false)
}

/// Destination of a relative seek, clamped to the valid range.
///
/// Positions can come from the system media service, so they are untrusted: a
/// non-finite target resolves to `0.0`, and the result never leaves
/// `[0, total]` (or `[0, ∞)` while the duration is still unknown, `total <= 0`).
pub fn seek_target(current: f64, delta: f64, total: f64) -> f64 {
    let target = current + delta;
    if !target.is_finite() {
        return 0.0;
    }
    if total > 0.0 {
        target.clamp(0.0, total)
    } else {
        target.max(0.0)
    }
}

/// Whether fine seeking is available for a track of the given duration.
///
/// Fine seeking only makes sense when the duration is known: with an unknown
/// duration (`0.0`) the position is undefined and every jump is ignored. A
/// negative or non-finite (`NaN`) duration also yields `false`.
pub fn fine_seek_enabled(total_duration: f64) -> bool {
    total_duration.is_finite() && total_duration > 0.0
}

/// Clamps a volume received from the system to `[0.0, 1.0]`.
///
/// A non-finite value resolves to `0.0` so that it never reaches the audio
/// engine or the media-controls reflection.
pub fn clamp_volume(v: f64) -> f32 {
    if !v.is_finite() {
        return 0.0;
    }
    v.clamp(0.0, 1.0) as f32
}

/// A transport command requested from outside the UI, independent of any GUI
/// toolkit. Relative and absolute seeks are kept unresolved: resolve them
/// against the current position with [`TransportAction::seek_destination`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransportAction {
    /// Start or resume playback.
    Play,
    /// Pause playback.
    Pause,
    /// Toggle between playing and paused.
    Toggle,
    /// Stop playback.
    Stop,
    /// Skip to the next track.
    Next,
    /// Go back to the previous track.
    Previous,
    /// Seek by a signed offset in seconds from the current position.
    SeekBy(f64),
    /// Seek to an absolute position in seconds (not yet clamped).
    SeekTo(f64),
    /// Set the volume, already clamped to `[0.0, 1.0]`.
    SetVolume(f32),
}

impl TransportAction {
    /// The play state this action asks for, given the current one: `Play`
    /// → `true`, `Pause` → `false`, `Toggle` → `!is_playing`; `None` for
    /// every other action.
    pub fn target_playing(&self, is_playing: bool) -> Option<bool> {
        match self {
            TransportAction::Play => Some(true),
            TransportAction::Pause => Some(false),
            TransportAction::Toggle => Some(!is_playing),
            _ => None,
        }
    }

    /// The clamped seek destination of a seek action (see [`seek_target`]);
    /// `None` for every other action.
    pub fn seek_destination(&self, current_pos: f64, total_duration: f64) -> Option<f64> {
        match self {
            TransportAction::SeekBy(delta) => {
                Some(seek_target(current_pos, *delta, total_duration))
            }
            TransportAction::SeekTo(target) => Some(seek_target(*target, 0.0, total_duration)),
            _ => None,
        }
    }
}

/// Maps an external control event to its transport action.
///
/// The fixed `SeekForward`/`SeekBackward` jumps become `SeekBy(±SEEK_STEP_SECS)`
/// and the volume is clamped with [`clamp_volume`].
pub fn external_control_action(event: &ExternalControlEvent) -> TransportAction {
    match *event {
        ExternalControlEvent::Play => TransportAction::Play,
        ExternalControlEvent::Pause => TransportAction::Pause,
        ExternalControlEvent::Toggle => TransportAction::Toggle,
        ExternalControlEvent::Stop => TransportAction::Stop,
        ExternalControlEvent::Next => TransportAction::Next,
        ExternalControlEvent::Previous => TransportAction::Previous,
        ExternalControlEvent::SeekForward => TransportAction::SeekBy(SEEK_STEP_SECS),
        ExternalControlEvent::SeekBackward => TransportAction::SeekBy(-SEEK_STEP_SECS),
        ExternalControlEvent::SeekRelative(delta) => TransportAction::SeekBy(delta),
        ExternalControlEvent::SeekTo(target) => TransportAction::SeekTo(target),
        ExternalControlEvent::SetVolume(volume) => TransportAction::SetVolume(clamp_volume(volume)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gc_is_blocked_while_the_preload_is_active() {
        assert!(gc_blocked_by_preload(1000, true, None));
        assert!(gc_blocked_by_preload(1000, true, Some(995)));
    }

    #[test]
    fn gc_is_blocked_within_the_safety_window_after_the_preload() {
        assert!(gc_blocked_by_preload(1019, false, Some(1000)));
        assert!(!gc_blocked_by_preload(1020, false, Some(1000)));
        assert!(!gc_blocked_by_preload(1025, false, Some(1000)));
    }

    #[test]
    fn gc_is_not_blocked_by_a_finish_mark_in_the_future() {
        // A clock that went backwards saturates to zero elapsed: still blocked.
        assert!(gc_blocked_by_preload(990, false, Some(1000)));
    }

    #[test]
    fn gc_runs_without_any_preload_state() {
        assert!(!gc_blocked_by_preload(1000, false, None));
    }

    #[test]
    fn the_preload_safety_window_is_twenty_seconds() {
        assert_eq!(PRELOAD_GC_SAFETY_SECS, 20);
    }

    #[test]
    fn seek_target_clamps_to_the_track() {
        assert_eq!(seek_target(5.0, 10.0, 100.0), 15.0);
        assert_eq!(seek_target(95.0, 10.0, 100.0), 100.0);
        assert_eq!(seek_target(2.0, -10.0, 100.0), 0.0);
    }

    #[test]
    fn seek_target_with_unknown_duration_is_never_negative() {
        assert_eq!(seek_target(-5.0, 1.0, 0.0), 0.0);
        assert_eq!(seek_target(500.0, 10.0, 0.0), 510.0);
    }

    #[test]
    fn seek_target_resolves_non_finite_input_to_zero() {
        assert_eq!(seek_target(f64::NAN, 1.0, 100.0), 0.0);
        assert_eq!(seek_target(f64::INFINITY, 1.0, 100.0), 0.0);
    }

    #[test]
    fn fine_seek_requires_a_known_finite_duration() {
        assert!(!fine_seek_enabled(0.0));
        assert!(!fine_seek_enabled(-1.0));
        assert!(!fine_seek_enabled(f64::NAN));
        assert!(!fine_seek_enabled(f64::INFINITY));
        assert!(fine_seek_enabled(1.0));
        assert!(fine_seek_enabled(3_600.0));
    }

    #[test]
    fn clamp_volume_bounds_the_value_and_drops_nan() {
        assert_eq!(clamp_volume(0.5), 0.5);
        assert_eq!(clamp_volume(1.5), 1.0);
        assert_eq!(clamp_volume(-2.0), 0.0);
        assert_eq!(clamp_volume(f64::NAN), 0.0);
    }

    #[test]
    fn external_play_pause_and_toggle_map_to_play_state_actions() {
        let play = external_control_action(&ExternalControlEvent::Play);
        let pause = external_control_action(&ExternalControlEvent::Pause);
        let toggle = external_control_action(&ExternalControlEvent::Toggle);
        assert_eq!(play, TransportAction::Play);
        assert_eq!(pause, TransportAction::Pause);
        assert_eq!(toggle, TransportAction::Toggle);
        assert_eq!(play.target_playing(true), Some(true));
        assert_eq!(pause.target_playing(false), Some(false));
        assert_eq!(toggle.target_playing(true), Some(false));
        assert_eq!(toggle.target_playing(false), Some(true));
        assert_eq!(TransportAction::Stop.target_playing(true), None);
    }

    #[test]
    fn external_stop_next_and_previous_map_one_to_one() {
        assert_eq!(
            external_control_action(&ExternalControlEvent::Stop),
            TransportAction::Stop
        );
        assert_eq!(
            external_control_action(&ExternalControlEvent::Next),
            TransportAction::Next
        );
        assert_eq!(
            external_control_action(&ExternalControlEvent::Previous),
            TransportAction::Previous
        );
    }

    #[test]
    fn external_fixed_seeks_jump_by_the_seek_step() {
        let fwd = external_control_action(&ExternalControlEvent::SeekForward);
        let back = external_control_action(&ExternalControlEvent::SeekBackward);
        assert_eq!(fwd, TransportAction::SeekBy(SEEK_STEP_SECS));
        assert_eq!(back, TransportAction::SeekBy(-SEEK_STEP_SECS));
        assert_eq!(fwd.seek_destination(30.0, 100.0), Some(40.0));
        assert_eq!(back.seek_destination(30.0, 100.0), Some(20.0));
        assert_eq!(back.seek_destination(3.0, 100.0), Some(0.0));
    }

    #[test]
    fn external_relative_and_absolute_seeks_are_clamped_on_resolution() {
        let rel = external_control_action(&ExternalControlEvent::SeekRelative(5.0));
        assert_eq!(rel, TransportAction::SeekBy(5.0));
        assert_eq!(rel.seek_destination(30.0, 100.0), Some(35.0));

        let abs = external_control_action(&ExternalControlEvent::SeekTo(250.0));
        assert_eq!(abs, TransportAction::SeekTo(250.0));
        assert_eq!(abs.seek_destination(30.0, 100.0), Some(100.0));
        assert_eq!(TransportAction::Next.seek_destination(30.0, 100.0), None);
    }

    #[test]
    fn external_volume_is_clamped_in_the_action() {
        assert_eq!(
            external_control_action(&ExternalControlEvent::SetVolume(1.5)),
            TransportAction::SetVolume(1.0)
        );
        assert_eq!(
            external_control_action(&ExternalControlEvent::SetVolume(f64::NAN)),
            TransportAction::SetVolume(0.0)
        );
    }
}
