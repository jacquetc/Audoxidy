//! Custom widgets shared by the panels.

pub mod album_art;
pub mod brand_logo;
pub mod dialog_frame;
pub mod icon_action;
pub mod pressable;
pub mod search_field;
pub mod stock_styles;

pub use album_art::{AlbumArt, CoverImage, Placeholder, RgbaCover, SharedCover};
pub use brand_logo::BrandLogo;
pub use dialog_frame::{DialogFrame, Frame, ModalText};
pub use icon_action::IconAction;
pub use pressable::Pressable;
pub use search_field::SearchField;
