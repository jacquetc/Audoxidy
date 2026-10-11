//! [`HShift`]: content shifted sideways by the library's shared horizontal
//! offset (quirk fix 36).
//!
//! The Iced library scrolls only its column header sideways
//! (`src/gui/widgets.rs:1832-1837`); the rows below are clipped where the
//! viewport ends, so once the header has scrolled its columns no longer stand
//! over their values. In the Teksilo UI the header and every row with columns
//! sit in an `HShift` bound to one offset signal
//! ([`LibraryViewModel::h_offset`](super::LibraryViewModel::h_offset)), so they
//! move together.
//!
//! An `HShift` fills the width it is given, `W`. It lays its child out
//! `max(W + offset, natural width)` wide and places it `offset` px towards
//! the leading edge, clipping what leaves its bounds. The child's trailing
//! edge therefore stays at (or beyond) the viewport's trailing edge: a
//! full-width background still covers the visible row, and a control pinned
//! to the end of a row (the chevron of a Detailed album header) stays on
//! screen while the columns before it scroll away.

use teksilo::core::binding::BindingLevel;
use teksilo::core::widget::WidgetPlacement;
use teksilo::prelude::*;

/// Content shifted by a shared horizontal offset; see the module
/// documentation.
pub struct HShift {
    offset: Signal<f32>,
    min_width: Option<Signal<f32>>,
    child: Option<Box<dyn Widget>>,
    child_id: Option<WidgetId>,
}

impl std::fmt::Debug for HShift {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HShift")
            .field("offset", &self.offset.get())
            .finish_non_exhaustive()
    }
}

impl HShift {
    /// `child`, shifted by `offset` px.
    pub fn new(offset: Signal<f32>, child: impl Widget + 'static) -> Self {
        Self {
            offset,
            min_width: None,
            child: Some(Box::new(child)),
            child_id: None,
        }
    }

    /// Lays the child out at least `width` px wide, whatever it reports (the
    /// column header, whose own scroll area must never overflow).
    pub fn min_width(mut self, width: Signal<f32>) -> Self {
        self.min_width = Some(width);
        self
    }

    /// The offset, never negative.
    fn offset(&self) -> f32 {
        self.offset.get().max(0.0)
    }

    /// The width the child is laid out at inside a `width`-wide shift.
    fn child_width(&self, width: f32, ctx: &LayoutContext) -> f32 {
        let natural = self
            .child_id
            .and_then(|id| ctx.child_size(id, SizeProposal::unspecified()))
            .map_or(0.0, |size| size.width);
        let floor = self.min_width.as_ref().map_or(0.0, Signal::get);
        (width + self.offset()).max(natural).max(floor)
    }
}

impl Widget for HShift {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.offset.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Relayout,
        );
        if let Some(width) = &self.min_width {
            width.bind_to(
                ctx.self_id(),
                ctx.binding_registry(),
                BindingLevel::Relayout,
            );
        }
        if let Some(child) = self.child.take() {
            self.child_id = Some(ctx.add_boxed(child));
        }
        self.child_id.into_iter().collect()
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let Some(id) = self.child_id else {
            return proposal.resolve(0.0, 0.0).into();
        };
        let natural = ctx
            .child_size(id, SizeProposal::unspecified())
            .unwrap_or(Size::ZERO);
        let width = proposal.width.unwrap_or(natural.width);
        let child_width = self.child_width(width, ctx);
        let height = ctx
            .child_size(
                id,
                SizeProposal {
                    width: Some(child_width),
                    height: proposal.height,
                },
            )
            .map_or(natural.height, |size| size.height);
        Size::new(width, proposal.height.unwrap_or(height)).into()
    }

    fn place_children(
        &self,
        bounds: Rect,
        _proposal: SizeProposal,
        children: &mut [WidgetPlacement],
        ctx: &LayoutContext,
    ) {
        let child_width = self.child_width(bounds.width, ctx);
        for child in children.iter_mut() {
            child.origin = Point::new(bounds.x - self.offset(), bounds.y);
            child.size = Size::new(child_width, bounds.height);
        }
    }

    fn children(&self) -> Vec<WidgetId> {
        self.child_id.into_iter().collect()
    }

    fn clips_children(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use teksilo::core::WidgetTree;
    use teksilo::widgets::{FixedSize, HStack, Spacer, VStack};

    use super::*;
    use crate::teksilo_ui::test_support::layout;

    fn block(width: f32) -> FixedSize {
        FixedSize::new()
            .width(width)
            .height(20.0)
            .child(Spacer::new())
    }

    /// A 300 px wide shift holding a 1000 px row, at the top of the window.
    fn mounted(offset: &Signal<f32>) -> (WidgetTree, WidgetId, WidgetId) {
        let mut tree = WidgetTree::new();
        let content = tree.add(HStack::new().spacing(0.0).child(block(1000.0)));
        let shift = tree.add(
            FixedSize::new()
                .width(300.0)
                .height(20.0)
                .child(HShift::new(offset.clone(), HStack::new().child(content))),
        );
        tree.add(
            VStack::new()
                .spacing(0.0)
                .alignment(teksilo::tokens::HAlignment::Leading)
                .child(shift)
                .child(Spacer::new()),
        );
        layout(&mut tree);
        (tree, shift, content)
    }

    #[test]
    fn the_child_moves_by_the_offset_and_keeps_its_natural_width() {
        let offset = Signal::new(0.0);
        let (mut tree, shift, content) = mounted(&offset);
        assert_eq!(tree.bounds(shift).width, 300.0);
        assert_eq!(tree.bounds(content).x, 0.0);
        assert_eq!(tree.bounds(content).width, 1000.0);
        offset.set(120.0);
        layout(&mut tree);
        assert_eq!(tree.bounds(content).x, -120.0);
        assert_eq!(tree.bounds(shift).x, 0.0, "the shift itself stays");
    }

    #[test]
    fn a_narrow_child_is_stretched_to_reach_the_trailing_edge() {
        let offset = Signal::new(50.0);
        let mut tree = WidgetTree::new();
        let content = tree.add(HStack::new().spacing(0.0).child(block(100.0)));
        let shift = tree.add(
            FixedSize::new()
                .width(300.0)
                .height(20.0)
                .child(HShift::new(offset.clone(), HStack::new().child(content))),
        );
        tree.add(VStack::new().spacing(0.0).child(shift).child(Spacer::new()));
        layout(&mut tree);
        let inner = tree.children(tree.children(shift)[0])[0];
        let bounds = tree.bounds(inner);
        assert_eq!(bounds.x - tree.bounds(shift).x, -50.0);
        assert_eq!(
            bounds.width, 350.0,
            "300 + 50: it still covers the viewport"
        );
    }
}
