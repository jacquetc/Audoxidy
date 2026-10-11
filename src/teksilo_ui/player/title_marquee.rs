//! The title and artist marquee: which window of the text is shown, from the
//! time the text has been scrolling.
//!
//! Iced counts `UITick`s (`PlayerUiState::tick_count`) and shows
//! `logic::marquee::player_marquee(text, limit, tick)` (`src/gui/app.rs:1665`).
//! Here the tick comes from elapsed time ([`MarqueeClock`]): a frame tick can
//! fire at any rate, so the view-model reads the clock and never counts.
//!
//! The clock runs only while the text scrolls: a text wider than its window
//! ([`needs_scroll`]) and a track that plays. Pausing keeps the position, so
//! playing again continues where it stopped, as Iced's tick count does; a new
//! title starts from the beginning ([`MarqueeClock::reset`]).

use std::time::{Duration, Instant};

use crate::logic::marquee::{
    ARTIST_MARQUEE_WIDTH, MARQUEE_TICK_PERIOD, TITLE_MARQUEE_WIDTH, needs_marquee, player_marquee,
    tick_from_elapsed,
};

/// The period of the marquee's frame subscription (500 ms, as Iced's tick).
pub const PERIOD: Duration = MARQUEE_TICK_PERIOD;

/// Whether the title or the artist is wider than its window and so scrolls
/// (`src/gui/app.rs:5916`).
pub fn needs_scroll(title: &str, artist: &str) -> bool {
    needs_marquee(title.chars().count(), TITLE_MARQUEE_WIDTH)
        || needs_marquee(artist.chars().count(), ARTIST_MARQUEE_WIDTH)
}

/// The title as shown at `tick`.
pub fn title_window(title: &str, tick: u64) -> String {
    let chars: Vec<char> = title.chars().collect();
    player_marquee(title, &chars, TITLE_MARQUEE_WIDTH, tick)
}

/// The artist as shown at `tick`.
pub fn artist_window(artist: &str, tick: u64) -> String {
    let chars: Vec<char> = artist.chars().collect();
    player_marquee(artist, &chars, ARTIST_MARQUEE_WIDTH, tick)
}

/// The marquee's tick: whole periods scrolled so far.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct MarqueeClock {
    base: u64,
    since: Option<Instant>,
}

impl MarqueeClock {
    /// The tick at `now`.
    pub fn tick(&self, now: Instant) -> u64 {
        let running = self.since.map_or(0, |since| {
            tick_from_elapsed(now.saturating_duration_since(since), PERIOD)
        });
        self.base.saturating_add(running)
    }

    /// Whether the clock is running.
    pub fn is_running(&self) -> bool {
        self.since.is_some()
    }

    /// Starts running at `now`; a clock that runs already is left alone.
    pub fn start(&mut self, now: Instant) {
        if self.since.is_none() {
            self.since = Some(now);
        }
    }

    /// Stops at `now`, keeping the tick it reached.
    pub fn stop(&mut self, now: Instant) {
        self.base = self.tick(now);
        self.since = None;
    }

    /// Back to tick 0, running or not as it was, counting from `now`.
    pub fn reset(&mut self, now: Instant) {
        self.base = 0;
        if self.since.is_some() {
            self.since = Some(now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LONG: &str = "A title that is far too long to fit in the player slot";

    fn at(ms: u64, origin: Instant) -> Instant {
        origin + Duration::from_millis(ms)
    }

    #[test]
    fn text_scrolls_from_forty_characters_for_the_title_and_forty_one_for_the_artist() {
        let thirty_nine = "a".repeat(39);
        let forty = "a".repeat(40);
        let forty_one = "a".repeat(41);
        assert!(!needs_scroll(&thirty_nine, &forty));
        assert!(needs_scroll(&forty, ""));
        assert!(!needs_scroll("", &forty));
        assert!(needs_scroll("", &forty_one));
    }

    #[test]
    fn the_length_is_counted_in_characters() {
        // 39 two-byte characters fit even though they are 78 bytes.
        assert!(!needs_scroll(&"é".repeat(39), ""));
        assert!(needs_scroll(&"é".repeat(40), ""));
    }

    #[test]
    fn a_short_text_is_shown_whole_at_any_tick() {
        assert_eq!(title_window("Short", 17), "Short");
        assert_eq!(artist_window("Short", 17), "Short");
    }

    #[test]
    fn a_long_title_scrolls_one_character_per_tick() {
        let chars: Vec<char> = LONG.chars().collect();
        let window = |tick| title_window(LONG, tick);
        assert_eq!(window(0), chars[0..39].iter().collect::<String>());
        assert_eq!(window(2), chars[2..41].iter().collect::<String>());
    }

    #[test]
    fn the_artist_window_is_forty_characters() {
        let long = "x".repeat(60);
        assert_eq!(artist_window(&long, 0).chars().count(), 40);
        assert_eq!(title_window(&long, 0).chars().count(), 39);
    }

    #[test]
    fn the_tick_comes_from_the_time_scrolled() {
        let t0 = Instant::now();
        let mut clock = MarqueeClock::default();
        assert_eq!(clock.tick(at(5_000, t0)), 0, "a stopped clock stays at 0");
        clock.start(t0);
        assert_eq!(clock.tick(at(499, t0)), 0);
        assert_eq!(clock.tick(at(500, t0)), 1);
        assert_eq!(clock.tick(at(5_250, t0)), 10);
    }

    #[test]
    fn a_frame_rate_of_sixty_hertz_does_not_speed_it_up() {
        let t0 = Instant::now();
        let mut clock = MarqueeClock::default();
        clock.start(t0);
        let mut last = 0;
        for frame in 0..=120u64 {
            last = clock.tick(at(frame * 16, t0));
        }
        assert_eq!(last, 3, "1920 ms is three periods however many frames");
    }

    #[test]
    fn pausing_keeps_the_position_and_playing_continues_from_it() {
        let t0 = Instant::now();
        let mut clock = MarqueeClock::default();
        clock.start(t0);
        clock.stop(at(2_000, t0));
        assert_eq!(clock.tick(at(9_000, t0)), 4);
        clock.start(at(10_000, t0));
        assert_eq!(clock.tick(at(10_500, t0)), 5);
    }

    #[test]
    fn starting_a_running_clock_does_not_restart_it() {
        let t0 = Instant::now();
        let mut clock = MarqueeClock::default();
        clock.start(t0);
        clock.start(at(1_000, t0));
        assert_eq!(clock.tick(at(1_000, t0)), 2);
    }

    #[test]
    fn a_new_title_starts_from_the_beginning() {
        let t0 = Instant::now();
        let mut clock = MarqueeClock::default();
        clock.start(t0);
        clock.reset(at(3_000, t0));
        assert_eq!(clock.tick(at(3_000, t0)), 0);
        assert_eq!(clock.tick(at(3_500, t0)), 1);
        assert!(clock.is_running());
        clock.stop(at(3_500, t0));
        clock.reset(at(4_000, t0));
        assert_eq!(clock.tick(at(4_000, t0)), 0);
        assert!(!clock.is_running());
    }
}
