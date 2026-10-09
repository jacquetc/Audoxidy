//! What the UI has preloaded, shared between the commands and the monitor.
//!
//! The playback commands preload the next track about 15 s before the end and
//! record it here; when the track advances they record when the preload
//! finished. The playback monitor reads the same state to keep the memory
//! purge away from a track transition: no purge while a preload is pending,
//! nor in the 20 s after it finished (`logic::transport::gc_blocked_by_preload`).
//! In the Iced UI these are the `preloaded_next_path` and
//! `preload_finished_at` fields of `PlayerUiState`.

#![warn(missing_docs)]

use parking_lot::Mutex;

use crate::logic::transport::gc_blocked_by_preload;

/// A copy of [`PreloadState`] at one moment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PreloadSnapshot {
    /// The preloaded next track, if a preload is pending.
    pub next_path: Option<String>,
    /// When the last preload finished (the track advanced), in UNIX seconds.
    pub finished_at: Option<u64>,
}

impl PreloadSnapshot {
    /// Whether a preload is pending.
    pub fn is_active(&self) -> bool {
        self.next_path.is_some()
    }

    /// Whether the memory purge must wait: a preload is pending, or one
    /// finished less than 20 s before `now_unix_secs`.
    pub fn blocks_gc(&self, now_unix_secs: u64) -> bool {
        gc_blocked_by_preload(now_unix_secs, self.is_active(), self.finished_at)
    }
}

/// The preload bookkeeping, shared through an `Arc` by the commands (which
/// write it) and the playback monitor (which reads it).
#[derive(Debug, Default)]
pub struct PreloadState {
    inner: Mutex<PreloadSnapshot>,
}

impl PreloadState {
    /// No preload, none finished.
    pub fn new() -> Self {
        Self::default()
    }

    /// The preloaded next track, if any.
    pub fn preloaded_next_path(&self) -> Option<String> {
        self.inner.lock().next_path.clone()
    }

    /// Whether `path` is the pending preload (the UI preloads each next track
    /// once).
    pub fn is_preloaded(&self, path: &str) -> bool {
        self.inner.lock().next_path.as_deref() == Some(path)
    }

    /// When the last preload finished, in UNIX seconds.
    pub fn finished_at(&self) -> Option<u64> {
        self.inner.lock().finished_at
    }

    /// Records that `path` was preloaded. The purge is blocked until the track
    /// advances, and the previous "finished" time is forgotten.
    pub fn mark_preloaded(&self, path: impl Into<String>) {
        let mut inner = self.inner.lock();
        inner.next_path = Some(path.into());
        inner.finished_at = None;
    }

    /// Records that the track advanced at `now_unix_secs`: the preload is
    /// consumed, and the purge stays blocked for 20 s more.
    pub fn mark_track_advanced(&self, now_unix_secs: u64) {
        let mut inner = self.inner.lock();
        inner.next_path = None;
        inner.finished_at = Some(now_unix_secs);
    }

    /// Forgets the pending preload and the finished time.
    pub fn clear(&self) {
        *self.inner.lock() = PreloadSnapshot::default();
    }

    /// A copy of the current state.
    pub fn snapshot(&self) -> PreloadSnapshot {
        self.inner.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pending_preload_blocks_the_purge_until_twenty_seconds_after_it_finishes() {
        let state = PreloadState::new();
        assert!(!state.snapshot().blocks_gc(1_000));

        state.mark_preloaded("next.flac");
        assert!(state.is_preloaded("next.flac"));
        assert!(state.snapshot().blocks_gc(1_000));

        state.mark_track_advanced(1_000);
        assert_eq!(state.preloaded_next_path(), None);
        assert_eq!(state.finished_at(), Some(1_000));
        assert!(state.snapshot().blocks_gc(1_019));
        assert!(!state.snapshot().blocks_gc(1_020));
    }

    #[test]
    fn a_new_preload_forgets_the_previous_finished_time() {
        let state = PreloadState::new();
        state.mark_track_advanced(500);
        state.mark_preloaded("b.flac");
        assert_eq!(state.finished_at(), None);
        state.clear();
        assert_eq!(state.snapshot(), PreloadSnapshot::default());
    }
}
