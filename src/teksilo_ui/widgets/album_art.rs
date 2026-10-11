//! [`AlbumArt`]: an album cover, or the Iced placeholder when there is none.
//!
//! Port of `album_art_widget` (`src/gui/widgets.rs:1436`). The cover fills a
//! square of the caller's size, cropped to cover it (Iced's
//! `ContentFit::Cover`), with the caller's corner radius. Without a cover it
//! draws one of three placeholders on the background colour:
//!
//! - [`Placeholder::Large`] (grid cards, Detailed list, `widgets.rs:1479`): the
//!   96 px `album.svg` in the secondary text colour over "AuDoxiDY" (Stage
//!   Wander 11 px, secondary), 5 px apart, centred, with the radius;
//! - [`Placeholder::Small`] (Thumbnail list, `widgets.rs:1506`): the 20 px
//!   `album.svg`, secondary, centred, with the radius;
//! - [`Placeholder::Player`] (`widgets.rs:1526`): "AuDoxiDY" in Stage Wander
//!   40 px, primary, centred, with no radius.
//!
//! The pixels are decoded off the UI thread by the cover store and handed over
//! as anything implementing [`CoverImage`]. They are drawn with
//! `ImageWidget::from_raw`, whose texture is released on the first frame that
//! does not draw it, so a scrolled-out cover gives its GPU memory back. A cover
//! bound through a signal swaps in place: the widget rebuilds when the signal
//! changes. The image is decorative: the album's name is in the text beside it.

use std::rc::Rc;

use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::tokens::HAlignment;
use teksilo::widgets::primitives::image_mask::ImageMaskShape;
use teksilo::widgets::{
    Center, FixedSize, IconWidget, ImageFit, ImageWidget, Padding, Panel, VStack,
};

use crate::teksilo_ui::widgets::BrandLogo;

/// Decoded RGBA pixels of a cover, as the cover store provides them.
pub trait CoverImage {
    /// The width in pixels.
    fn width(&self) -> u32;
    /// The height in pixels.
    fn height(&self) -> u32;
    /// The pixels: RGBA, 8 bits per channel, rows tightly packed
    /// (`width × height × 4` bytes).
    fn rgba(&self) -> &[u8];
}

/// A shared decoded cover.
pub type SharedCover = Rc<dyn CoverImage>;

/// The simplest [`CoverImage`]: shared pixels and their size.
#[derive(Debug, Clone)]
pub struct RgbaCover {
    /// The RGBA pixels.
    pub pixels: Rc<[u8]>,
    /// The width in pixels.
    pub width: u32,
    /// The height in pixels.
    pub height: u32,
}

impl CoverImage for RgbaCover {
    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn rgba(&self) -> &[u8] {
        &self.pixels
    }
}

/// The large placeholder icon's size (`widgets.rs:1482`).
pub const LARGE_ICON_SIZE: f32 = 96.0;
/// The small placeholder icon's size (`widgets.rs:1508`).
pub const SMALL_ICON_SIZE: f32 = 20.0;
/// The space between the large placeholder's icon and text
/// (`widgets.rs:1494`).
const LARGE_SPACING: f32 = 5.0;
/// The large placeholder's logo size (`widgets.rs:1490`).
pub const LARGE_LOGO_SIZE: f32 = 11.0;
/// The player placeholder's logo size (`widgets.rs:1529`).
pub const PLAYER_LOGO_SIZE: f32 = 40.0;
/// The player placeholder's padding (`widgets.rs:1535`).
const PLAYER_PADDING: f32 = 20.0;

/// What to draw when there is no cover; see the module documentation. The
/// icon factories are the caller's `album.svg` (from `icons.rs`).
#[derive(Debug, Clone, Copy)]
pub enum Placeholder {
    /// The 96 px album icon over the 11 px logo.
    Large(fn() -> IconWidget),
    /// The 20 px album icon.
    Small(fn() -> IconWidget),
    /// The 40 px logo, square corners.
    Player,
}

/// The `ImageMaskShape::RoundedSquare` fraction for `radius` on a `size`
/// square (the mask takes the radius as a fraction of the side).
pub fn mask_fraction(radius: f32, size: f32) -> f32 {
    if size <= 0.0 {
        0.0
    } else {
        (radius / size).clamp(0.0, 0.5)
    }
}

/// An album cover or its placeholder, `size` px square; see the module
/// documentation.
pub struct AlbumArt {
    cover: Prop<Option<SharedCover>>,
    placeholder: Placeholder,
    size: f32,
    radius: f32,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for AlbumArt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AlbumArt")
            .field("placeholder", &self.placeholder)
            .field("size", &self.size)
            .field("radius", &self.radius)
            .finish_non_exhaustive()
    }
}

impl AlbumArt {
    /// `cover` (a value, or a signal the cover store updates) in a `size` px
    /// square, `placeholder` when it is `None`.
    pub fn new(
        cover: impl Into<Prop<Option<SharedCover>>>,
        placeholder: Placeholder,
        size: f32,
    ) -> Self {
        Self {
            cover: cover.into(),
            placeholder,
            size,
            radius: 0.0,
            root: None,
        }
    }

    /// The corner radius of the cover and of the Large and Small placeholders
    /// (default square; the grid cards use 8, `library.rs:1557`).
    pub fn radius(mut self, radius: f32) -> Self {
        self.radius = radius;
        self
    }

    fn current(&self) -> Option<SharedCover> {
        match &self.cover {
            Prop::Static(cover) => cover.clone(),
            Prop::Bound(signal) => signal.get(),
        }
    }

    /// The cover image, cropped to the square.
    fn image(&self, cover: &dyn CoverImage) -> FixedSize {
        let mut image = ImageWidget::from_raw(cover.rgba().to_vec(), cover.width(), cover.height())
            .fit(ImageFit::Cover)
            .size(self.size, self.size)
            .a11y_hidden();
        if self.radius > 0.0 {
            image = image.mask(ImageMaskShape::RoundedSquare(mask_fraction(
                self.radius,
                self.size,
            )));
        }
        FixedSize::new()
            .width(self.size)
            .height(self.size)
            .child(image)
    }

    /// The placeholder on the background colour.
    fn placeholder(&self) -> FixedSize {
        let (content, radius): (Box<dyn Widget>, f32) = match self.placeholder {
            Placeholder::Large(icon) => (
                Box::new(
                    Center::new().child(
                        VStack::new()
                            .spacing(LARGE_SPACING)
                            .alignment(HAlignment::Center)
                            .child(icon().icon_size(LARGE_ICON_SIZE).color(TextRole::Secondary))
                            .child(BrandLogo::new(LARGE_LOGO_SIZE).color(TextRole::Secondary)),
                    ),
                ),
                self.radius,
            ),
            Placeholder::Small(icon) => (
                Box::new(
                    Center::new()
                        .child(icon().icon_size(SMALL_ICON_SIZE).color(TextRole::Secondary)),
                ),
                self.radius,
            ),
            Placeholder::Player => (
                Box::new(
                    Padding::uniform(PLAYER_PADDING)
                        .child(Center::new().child(BrandLogo::new(PLAYER_LOGO_SIZE))),
                ),
                0.0,
            ),
        };
        FixedSize::new().width(self.size).height(self.size).child(
            Panel::new()
                .padding(0.0)
                .border_width(0.0)
                .corner_radius(radius)
                .background(SurfaceRole::Main)
                .a11y_presentational()
                .child(BoxedChild(Some(content), None)),
        )
    }
}

impl Widget for AlbumArt {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        if let Prop::Bound(signal) = &self.cover {
            signal.bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        }
        let root = match self.current() {
            Some(cover) => ctx.add(self.image(cover.as_ref())),
            None => ctx.add(self.placeholder()),
        };
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| Size::new(self.size, self.size))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// A boxed widget as a child: builds it and takes its size.
#[derive(Debug)]
struct BoxedChild(Option<Box<dyn Widget>>, Option<WidgetId>);

impl Widget for BoxedChild {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        if let Some(content) = self.0.take() {
            self.1 = Some(ctx.add_boxed(content));
        }
        self.1.into_iter().collect()
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.1
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.1.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
