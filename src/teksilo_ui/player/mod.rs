//! The player panel: the cover, the transport, the seek bar, the volume bubble
//! and the player menu, at the top of the left column (400 x 400).
//!
//! - [`PlayerViewModel`]: the track, the transport on the shared playback
//!   cursor, the volume wheel with its bubble, the deadlines and the marquee;
//! - [`PlayerPanel`]: the panel the app mounts in its player region;
//! - [`menu`]: the menu button and its entries.
//!
//! Pure rules live apart so they are tested without a window:
//! [`geometry`] (Iced's sizes, the zones, the tooltip rules), [`timing`] (the
//! volume bubble's and the controls' deadlines) and [`title_marquee`] (the
//! window of a long title at a time).

pub mod geometry;
pub mod menu;
pub mod panel;
pub mod player_vm;
pub mod seek_bar;
pub mod timing;
pub mod title_marquee;

pub use self::geometry::{HoverZone, VolumeIcon};
pub use self::panel::PlayerPanel;
pub use self::player_vm::PlayerViewModel;
