//! Layer A: reactive handles on the few things the UI shows once: the track
//! that is playing, the library totals, the output device and the active
//! playlist.
//!
//! A *single* owns a few `Signal`s that widgets bind to, loads its value
//! through the facade (or reads it from the player at construction), and
//! keeps it current from the facade's events. It sits beside the
//! [`models`](super::models), which adapt collections. Like a model it is
//! built once, subscribes from a long-lived widget's `build` with `wire`
//! (which subscribes afresh on every call, since a subscription lasts as long
//! as the build that made it), and has no real/mock seam: the mocks build runs
//! the same facade over an in-memory database and a fake player.
//!
//! The event handler of each single is a public method (`on_event`), so a test
//! can deliver an event directly: a headless tree drops the hub's deliveries.

pub mod single_active_playlist;
pub mod single_library_stats;
pub mod single_now_playing;
pub mod single_output_device;

pub use single_active_playlist::SingleActivePlaylist;
pub use single_library_stats::SingleLibraryStats;
pub use single_now_playing::SingleNowPlaying;
pub use single_output_device::SingleOutputDevice;
