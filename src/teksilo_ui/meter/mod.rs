//! The VU meter of the library's bottom bar, its settings popup, and the
//! view-model behind both.
//!
//! - [`MeterViewModel`]: runs the correct clock-based meter
//!   (`crate::logic::meter::LiveMeter`) over the engine's `MeterData` while
//!   playing or fading, and owns the five meter settings;
//! - [`MeterSlot`]: what the library mounts in the bottom bar's 400 px meter
//!   slot (`LibraryPanel::meter_slot`): the meter when it is enabled, an empty
//!   slot when it is not, and in both cases the popup on a right-click;
//! - [`MeterPopup`]: the settings, anchored to the slot.

mod meter_vm;
mod popup;
mod slot;

pub use meter_vm::{MeterViewModel, frame_of};
pub use popup::{MeterPopup, POPUP_WIDTH};
pub use slot::{MeterSlot, SLOT_HEIGHT, SLOT_WIDTH};

#[cfg(test)]
mod tests;
