//! Custom widgets shared by the panels.

pub mod album_art;
pub mod brand_logo;
pub mod column_header;
pub mod dialog_frame;
pub mod effect_slider;
pub mod eq_slider;
pub mod icon_action;
pub mod menu;
pub mod pressable;
pub mod search_field;
pub mod stepper;
pub mod stock_styles;
pub mod vu_meter;

pub use album_art::{AlbumArt, CoverImage, Placeholder, RgbaCover, SharedCover};
pub use brand_logo::BrandLogo;
pub use column_header::ColumnHeader;
pub use dialog_frame::{DialogFrame, Frame, ModalText};
pub use effect_slider::EffectSlider;
pub use eq_slider::EqSlider;
pub use icon_action::IconAction;
pub use menu::{MenuEntry, PlayerMenu};
pub use pressable::Pressable;
pub use search_field::SearchField;
pub use stepper::Stepper;
pub use vu_meter::{VuMeter, VuMeterFrame};
