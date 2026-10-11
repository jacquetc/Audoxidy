//! Where the preset dialogs open, and the layer that holds them.
//!
//! Iced shows the Load and Save dialogs at a position computed from the Audio
//! Center's window position, not at the centre of the window or at the
//! pointer (`src/gui/app.rs:5717-5765`):
//!
//! | Dialog | Size | Top-leading corner |
//! | --- | --- | --- |
//! | Load | 330 × 352 | panel + ((940 − 330) / 2, (480 − 352) / 2 + 15) |
//! | Save | 190 × 162 | panel + (940 − 194 − 15, 120) |
//!
//! where "panel" is the Audio Center's corner: its dragged position, or the
//! one that centres it in the window (`app.rs:5719-5725`). The Save dialog
//! uses a width of 194 for its offset though it is 190 wide, as Iced does.
//!
//! A Teksilo modal is centred by the framework, so the dialog is put in a
//! layer ([`DialogLayer`]) as large as the window, centred, with the dialog
//! at the computed offset inside it. The layer is Iced's invisible click
//! blocker (`app.rs:5766-5773`): a press anywhere outside the dialog
//! dismisses the modal. Escape and the scrim are the framework's
//! (`ModalCloseBehavior::EscapeOrClickOutside`), and every route ends in the
//! `on_dismiss` callback, which is where the Load dialog restores its backup.
//!
//! Unlike Iced, the dialog is kept inside the window: an Audio Center dragged
//! towards an edge (it may leave the window by up to 840 × 440 px,
//! `app.rs:5124-5127`) would put the dialog where it cannot be reached.

use std::rc::Rc;

use teksilo::core::modal::{ModalCloseBehavior, ModalRequest};
use teksilo::prelude::*;
use teksilo::tokens::Alignment;
use teksilo::widgets::{FixedSize, Padding, RectWidget, ZStack};

/// The Audio Center's size (`audio_center.rs:2007-2008`).
pub const PANEL_SIZE: (f32, f32) = (940.0, 480.0);
/// The window size assumed when the context has no window (a headless
/// tree); the size the input helpers of the tests lay out in.
pub const FALLBACK_WINDOW: (f32, f32) = (1360.0, 880.0);

/// The Load dialog's size (`app.rs:5545-5546`).
pub const LOAD_SIZE: (f32, f32) = (330.0, 352.0);
/// The Save dialog's size (`app.rs:5666-5667`).
pub const SAVE_SIZE: (f32, f32) = (190.0, 162.0);

/// The width Iced uses to place the Save dialog (`app.rs:5736`).
const SAVE_PLACEMENT_WIDTH: f32 = 194.0;

/// Which dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogKind {
    /// "Cargar Preset".
    Load,
    /// "Guardar Preset".
    Save,
}

impl DialogKind {
    /// The dialog's size.
    pub fn size(self) -> (f32, f32) {
        match self {
            Self::Load => LOAD_SIZE,
            Self::Save => SAVE_SIZE,
        }
    }
}

/// The Audio Center's top-leading corner in a window of `window` px: where
/// the user dragged it, or the corner that centres it (`app.rs:5719`).
pub fn panel_origin(position: Option<(f32, f32)>, window: (f32, f32)) -> (f32, f32) {
    position.unwrap_or((
        (window.0 - PANEL_SIZE.0) / 2.0,
        (window.1 - PANEL_SIZE.1) / 2.0,
    ))
}

/// The dialog's top-leading corner for an Audio Center at `panel`
/// (`app.rs:5729-5742`), before it is kept inside the window.
pub fn dialog_origin(kind: DialogKind, panel: (f32, f32)) -> (f32, f32) {
    match kind {
        DialogKind::Load => (
            panel.0 + (PANEL_SIZE.0 - LOAD_SIZE.0) / 2.0,
            panel.1 + (PANEL_SIZE.1 - LOAD_SIZE.1) / 2.0 + 15.0,
        ),
        DialogKind::Save => (
            panel.0 + PANEL_SIZE.0 - SAVE_PLACEMENT_WIDTH - 15.0,
            panel.1 + 120.0,
        ),
    }
}

/// [`dialog_origin`] moved the least that puts the whole dialog inside a
/// window of `window` px (a dialog larger than the window sits at the
/// origin).
pub fn keep_inside(origin: (f32, f32), size: (f32, f32), window: (f32, f32)) -> (f32, f32) {
    let clamp = |value: f32, extent: f32, limit: f32| value.min(limit - extent).max(0.0);
    (
        clamp(origin.0, size.0, window.0),
        clamp(origin.1, size.1, window.1),
    )
}

/// The full position of `kind` for an Audio Center at `position` in `window`.
pub fn place(kind: DialogKind, position: Option<(f32, f32)>, window: (f32, f32)) -> (f32, f32) {
    keep_inside(
        dialog_origin(kind, panel_origin(position, window)),
        kind.size(),
        window,
    )
}

/// The window's size as the context knows it, or [`FALLBACK_WINDOW`].
pub fn window_size(ctx: &EventContext) -> (f32, f32) {
    ctx.window().map_or(FALLBACK_WINDOW, |window| {
        let (width, height) = window.size().get();
        (width as f32, height as f32)
    })
}

/// The layer that holds a dialog: window-sized, with the dialog at `origin`,
/// and a press outside the dialog dismisses the modal.
pub struct DialogLayer {
    dialog: Option<Box<dyn Widget>>,
    origin: (f32, f32),
    window: (f32, f32),
    root: Option<WidgetId>,
}

impl std::fmt::Debug for DialogLayer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DialogLayer")
            .field("origin", &self.origin)
            .field("window", &self.window)
            .finish_non_exhaustive()
    }
}

impl DialogLayer {
    /// A layer of `window` px with `dialog` at `origin`.
    pub fn new(dialog: Box<dyn Widget>, origin: (f32, f32), window: (f32, f32)) -> Self {
        Self {
            dialog: Some(dialog),
            origin,
            window,
            root: None,
        }
    }
}

impl Widget for DialogLayer {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let Some(dialog) = self.dialog.take() else {
            return self.root.into_iter().collect();
        };
        let dialog = ctx.add_boxed(dialog);
        let blocker = RectWidget::new()
            .background(SurfaceRole::Transparent)
            .on_tap(|_, ctx| ctx.dismiss_modal());
        let root = ctx.add(
            FixedSize::new()
                .width(self.window.0)
                .height(self.window.1)
                .child(
                    ZStack::new()
                        .alignment(Alignment::TOP_LEADING)
                        .child(blocker)
                        .child(Padding::new(self.origin.1, 0.0, 0.0, self.origin.0).child(dialog)),
                ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, _proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, SizeProposal::unspecified()))
            .unwrap_or(Size::new(self.window.0, self.window.1))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// What runs when the modal goes away by any route (Escape, a press outside,
/// a button, the code).
pub type DismissFn = Rc<dyn Fn(&mut EventContext)>;

/// Presents `dialog` as a modal at its Iced position, titled `title`.
/// `on_dismiss` runs however it goes away; a dialog's own buttons settle it
/// first so that the dismissal that follows them does nothing.
pub fn present(
    ctx: &mut EventContext,
    kind: DialogKind,
    panel_position: Option<(f32, f32)>,
    title: LocalizedString,
    dialog: Box<dyn Widget>,
    on_dismiss: DismissFn,
) {
    let window = window_size(ctx);
    let origin = place(kind, panel_position, window);
    ctx.present_modal(
        ModalRequest::deferred(move |tree| tree.add(DialogLayer::new(dialog, origin, window)))
            .title(title.resolve_now())
            .close_behavior(ModalCloseBehavior::EscapeOrClickOutside)
            .on_dismiss(Rc::new(move |_reason, ctx| on_dismiss(ctx))),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_load_dialog_opens_below_the_panels_centre_line_by_15_px() {
        let panel = panel_origin(None, (1360.0, 880.0));
        assert_eq!(panel, (210.0, 200.0));
        assert_eq!(
            dialog_origin(DialogKind::Load, panel),
            (210.0 + 305.0, 200.0 + 64.0 + 15.0),
            "((940 - 330) / 2, (480 - 352) / 2 + 15) from the panel"
        );
    }

    #[test]
    fn the_save_dialog_opens_at_the_panels_right_edge_using_a_width_of_194() {
        assert_eq!(
            dialog_origin(DialogKind::Save, (100.0, 50.0)),
            (100.0 + 940.0 - 194.0 - 15.0, 50.0 + 120.0)
        );
    }

    #[test]
    fn a_dragged_panel_moves_its_dialogs_with_it() {
        let dragged = panel_origin(Some((30.0, 20.0)), (1360.0, 880.0));
        assert_eq!(dragged, (30.0, 20.0));
        assert_eq!(
            place(DialogKind::Load, Some((30.0, 20.0)), (1360.0, 880.0)),
            (335.0, 99.0)
        );
    }

    #[test]
    fn a_dialog_is_kept_inside_the_window() {
        let window = (1360.0, 880.0);
        // A panel dragged to the far corner (Iced allows it).
        let (x, y) = place(DialogKind::Save, Some((1260.0, 840.0)), window);
        assert_eq!((x, y), (1360.0 - 190.0, 880.0 - 162.0));
        let (x, y) = place(DialogKind::Load, Some((-800.0, -400.0)), window);
        assert_eq!((x, y), (0.0, 0.0));
        assert_eq!(
            keep_inside((5.0, 5.0), (100.0, 100.0), (50.0, 50.0)),
            (0.0, 0.0)
        );
    }
}
