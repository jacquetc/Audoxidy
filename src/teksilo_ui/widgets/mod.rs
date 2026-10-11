//! Custom widgets shared by the panels.

pub mod effect_slider;
pub mod eq_slider;
pub mod pressable;
pub mod stepper;
pub mod vu_meter;

pub use effect_slider::EffectSlider;
pub use eq_slider::EqSlider;
pub use pressable::Pressable;
pub use stepper::Stepper;
pub use vu_meter::{VuMeter, VuMeterFrame};
