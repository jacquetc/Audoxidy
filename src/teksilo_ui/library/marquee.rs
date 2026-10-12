//! The rubber band of the list modes and of the grid: [`MarqueeController`]
//! and the [`Marquee`] overlay that draws it.
//!
//! Port of the Iced library marquee (`LibraryMarqueeStart`, the drag in
//! `PlayerMouseMoved`, `GlobalMouseRelease`, `DragTick` and the overlay,
//! `src/gui/app.rs:3320-3341`, `:4686-4706`, `:4790-4821`, `:1633-1660`,
//! `src/gui/library.rs:1960-2008`):
//!
//! - a primary press anywhere in the list (on a row too) arms it at the
//!   pointer, in content coordinates (the list's top, plus its scroll);
//! - once the pointer has moved more than 3 px it becomes a drag
//!   (`logic::library_geometry::MARQUEE_DRAG_THRESHOLD_SQ`), and the band is
//!   drawn from the start to the pointer: the accent at 30 % with a 1 px
//!   accent outline and a 4 px radius, clipped to the top of the list;
//! - while it drags, a pointer within 80 px of the list's top or 60 px of its
//!   bottom scrolls the list by a fifth of its distance into that zone every
//!   32 ms (Iced's 150 px from the window's top and 100 px from its bottom;
//!   the list starts 70 px down and ends 40 px up), the speed taken from the
//!   time elapsed, never from a count of frames; the frame-tick subscription
//!   exists only while a drag does;
//! - on release, the band selects the rows it crosses
//!   (`LibraryViewModel::select_marquee`): it replaces the selection, adds to
//!   it with Shift, toggles with Ctrl. The grid gives the controller its own
//!   selection ([`MarqueeController::with_select`]): the cards and the open
//!   album's songs the band crosses (G3).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;

use teksilo::core::Modifiers;
use teksilo::core::binding::BindingLevel;
use teksilo::core::event::{PointerButton, WidgetEvent};
use teksilo::core::frame_tick_scheduler::FrameTickSubscription;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::prelude::*;

use super::library_vm::LibraryViewModel;
use crate::logic::library_geometry::{ContentRect, MARQUEE_DRAG_THRESHOLD_SQ, marquee_rect};

/// The auto-scroll zone at the top of the list (`app.rs:1637`: 150 px from
/// the window's top, the list starting 70 px down).
pub const TOP_ZONE: f32 = 80.0;
/// The auto-scroll zone at the bottom of the list (`app.rs:1638`: 100 px from
/// the window's bottom, the list ending 40 px up).
pub const BOTTOM_ZONE: f32 = 60.0;
/// The period the Iced auto-scroll moves at (`app.rs:5937`).
pub const TICK_MS: f32 = 32.0;
/// The band's fill opacity (`library.rs:1989`).
pub const FILL_ALPHA: f32 = 0.3;
/// The band's outline (`library.rs:1992`).
pub const OUTLINE: f32 = 1.0;
/// The band's radius (`library.rs:1993`).
pub const RADIUS: f32 = 4.0;

/// How far one 32 ms auto-scroll step moves the list for a pointer at `y`
/// in a list `height` px tall: a fifth of its distance into a zone,
/// negative upwards (`app.rs:1640-1645`).
pub fn auto_scroll_step(y: f32, height: f32) -> f32 {
    let bottom = height - BOTTOM_ZONE;
    if y < TOP_ZONE {
        (y - TOP_ZONE) / 5.0
    } else if y > bottom {
        (y - bottom) / 5.0
    } else {
        0.0
    }
}

/// The band as drawn, in the list's coordinates, for a drag from `start` to
/// `end` (content coordinates) with the list scrolled to `scroll_y`; `None`
/// before the 3 px threshold or when nothing of it is visible
/// (`library.rs:1960-1980`).
pub fn visual_rect(start: (f32, f32), end: (f32, f32), scroll_y: f32) -> Option<Rect> {
    let (dx, dy) = (start.0 - end.0, start.1 - end.1);
    if dx * dx + dy * dy <= MARQUEE_DRAG_THRESHOLD_SQ {
        return None;
    }
    let top = (start.1 - scroll_y).min(end.1 - scroll_y);
    let bottom = (start.1 - scroll_y).max(end.1 - scroll_y);
    let y = top.max(0.0);
    let width = dx.abs().max(1.0);
    let height = (bottom - y).max(0.0);
    (height > 0.1 && width > 0.1).then(|| Rect::new(start.0.min(end.0), y, width, height))
}

#[derive(Debug, Default)]
struct Drag {
    /// Where it was armed, in content coordinates.
    start: Option<(f32, f32)>,
    /// The pointer, in the list's coordinates.
    pointer: (f32, f32),
    /// Whether it passed the threshold.
    dragging: bool,
    modifiers: Modifiers,
}

/// What a finished sweep selects: the band (content space), the viewport's
/// width, the modifiers at the release.
type SelectFn = Rc<dyn Fn(ContentRect, f32, Modifiers)>;

struct Shared {
    select: SelectFn,
    scroll_y: Signal<f32>,
    /// The list's (width, height).
    viewport: Rc<Cell<(f32, f32)>>,
    drag: RefCell<Drag>,
    dragging: Signal<bool>,
    band: Signal<Option<Rect>>,
}

/// The marquee's pointer handling, shared by the list (which feeds it its
/// pointer events) and the overlay (which draws it and auto-scrolls).
#[derive(Clone)]
pub struct MarqueeController {
    shared: Rc<Shared>,
}

impl std::fmt::Debug for MarqueeController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarqueeController")
            .field("drag", &self.shared.drag.borrow())
            .finish_non_exhaustive()
    }
}

impl MarqueeController {
    /// The marquee of `vm`'s list, scrolled by `scroll_y`, `viewport` big.
    pub fn new(
        vm: LibraryViewModel,
        scroll_y: Signal<f32>,
        viewport: Rc<Cell<(f32, f32)>>,
    ) -> Self {
        Self::with_select(scroll_y, viewport, move |rect, width, modifiers| {
            vm.select_marquee(rect, width, modifiers);
        })
    }

    /// The marquee of a view scrolled by `scroll_y`, `viewport` big, whose
    /// finished sweeps `select` applies (the band in content space, the
    /// viewport's width, the modifiers at the release).
    pub fn with_select(
        scroll_y: Signal<f32>,
        viewport: Rc<Cell<(f32, f32)>>,
        select: impl Fn(ContentRect, f32, Modifiers) + 'static,
    ) -> Self {
        Self {
            shared: Rc::new(Shared {
                select: Rc::new(select),
                scroll_y,
                viewport,
                drag: RefCell::new(Drag::default()),
                dragging: Signal::new(false),
                band: Signal::new(None),
            }),
        }
    }

    /// Whether a drag is in progress.
    pub fn dragging(&self) -> Signal<bool> {
        self.shared.dragging.clone()
    }

    /// The band as drawn, in the list's coordinates.
    pub fn band(&self) -> Signal<Option<Rect>> {
        self.shared.band.clone()
    }

    fn content(&self, local: (f32, f32)) -> (f32, f32) {
        (local.0, local.1 + self.shared.scroll_y.get())
    }

    fn redraw(&self) {
        let drag = self.shared.drag.borrow();
        let band = match (drag.dragging, drag.start) {
            (true, Some(start)) => visual_rect(
                start,
                self.content(drag.pointer),
                self.shared.scroll_y.get(),
            ),
            _ => None,
        };
        drop(drag);
        self.shared.band.set_if_changed(band);
    }

    /// A pointer event of the list, `position` in the list's coordinates.
    pub fn on_pointer(&self, event: &WidgetEvent, ctx: &mut EventContext) -> EventResponse {
        match event {
            WidgetEvent::PointerDown {
                position,
                button: PointerButton::Primary,
                modifiers,
                ..
            } => {
                let local = (position.x, position.y);
                *self.shared.drag.borrow_mut() = Drag {
                    start: Some(self.content(local)),
                    pointer: local,
                    dragging: false,
                    modifiers: *modifiers,
                };
                EventResponse::Ignored
            }
            WidgetEvent::PointerMove {
                position,
                modifiers,
                ..
            } => {
                let local = (position.x, position.y);
                let mut drag = self.shared.drag.borrow_mut();
                let Some(start) = drag.start else {
                    return EventResponse::Ignored;
                };
                drag.pointer = local;
                drag.modifiers = *modifiers;
                if !drag.dragging {
                    let now = self.content(local);
                    let (dx, dy) = (now.0 - start.0, now.1 - start.1);
                    if dx * dx + dy * dy > MARQUEE_DRAG_THRESHOLD_SQ {
                        drag.dragging = true;
                        drop(drag);
                        ctx.capture_pointer();
                        self.shared.dragging.set_if_changed(true);
                        self.redraw();
                        return EventResponse::Handled;
                    }
                    return EventResponse::Ignored;
                }
                drop(drag);
                self.redraw();
                EventResponse::Handled
            }
            WidgetEvent::PointerUp {
                position,
                button: PointerButton::Primary,
                modifiers,
                ..
            } => {
                let drag = std::mem::take(&mut *self.shared.drag.borrow_mut());
                let response = match (drag.dragging, drag.start) {
                    (true, Some(start)) => {
                        let end = self.content((position.x, position.y));
                        if let Some(rect) = marquee_rect(start, end) {
                            self.select(rect, *modifiers);
                        }
                        ctx.release_pointer();
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                };
                self.shared.dragging.set_if_changed(false);
                self.shared.band.set_if_changed(None);
                response
            }
            _ => EventResponse::Ignored,
        }
    }

    /// The pointer was taken away: the drag ends with no selection.
    pub fn cancel(&self) {
        *self.shared.drag.borrow_mut() = Drag::default();
        self.shared.dragging.set_if_changed(false);
        self.shared.band.set_if_changed(None);
    }

    fn select(&self, rect: ContentRect, modifiers: Modifiers) {
        let width = self.shared.viewport.get().0;
        (self.shared.select)(rect, width, modifiers);
    }

    /// One auto-scroll step for `elapsed_ms` since the last one.
    pub fn auto_scroll(&self, elapsed_ms: f32) {
        let (pointer_y, dragging) = {
            let drag = self.shared.drag.borrow();
            (drag.pointer.1, drag.dragging)
        };
        if !dragging {
            return;
        }
        let step = auto_scroll_step(pointer_y, self.shared.viewport.get().1);
        if step == 0.0 {
            return;
        }
        let scroll = &self.shared.scroll_y;
        let next = (scroll.get() + step * elapsed_ms / TICK_MS).max(0.0);
        scroll.set_if_changed(next);
        self.redraw();
    }
}

/// The band's overlay; see the module documentation. It takes no pointer
/// events: the list feeds them to its [`MarqueeController`].
pub struct Marquee {
    controller: MarqueeController,
    tick: Option<FrameTickSubscription>,
}

impl std::fmt::Debug for Marquee {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Marquee")
            .field("ticking", &self.tick.is_some())
            .finish_non_exhaustive()
    }
}

impl Marquee {
    /// The overlay of `controller`.
    pub fn new(controller: MarqueeController) -> Self {
        Self {
            controller,
            tick: None,
        }
    }
}

impl Widget for Marquee {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let id = ctx.self_id();
        let shared = &self.controller.shared;
        shared
            .dragging
            .bind_to(id, ctx.binding_registry(), BindingLevel::Rebuild);
        shared
            .band
            .bind_to(id, ctx.binding_registry(), BindingLevel::RepaintOnly);
        ctx.apply_self_handlers(HandlerSet::new().hit_transparent(true));
        // The subscription lives only while a drag does: the widget rebuilds
        // when the drag starts and ends.
        self.tick = None;
        if shared.dragging.get() {
            self.tick = Some(ctx.subscribe_frame_tick());
            let controller = self.controller.clone();
            let last = Rc::new(Cell::new(Instant::now()));
            ctx.effect(&ctx.frame_tick(), move |_| {
                let now = Instant::now();
                let elapsed = now.saturating_duration_since(last.replace(now));
                controller.auto_scroll(elapsed.as_secs_f32() * 1000.0);
            });
        }
        Vec::new()
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }

    fn paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let Some(band) = self.controller.shared.band.get() else {
            return;
        };
        let rect = Rect::new(
            bounds.x + band.x,
            bounds.y + band.y,
            band.width,
            band.height.min((bounds.height - band.y).max(0.0)),
        );
        let accent = ColorProp::from(SurfaceRole::Accent).resolve(ctx.theme, true);
        let radius = CornerRadius::uniform(RADIUS);
        canvas.fill_rounded_rect(rect, radius, accent.with_alpha(FILL_ALPHA));
        canvas.stroke_rounded_rect(rect, radius, accent, OUTLINE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_waits_for_a_drag_longer_than_three_pixels() {
        assert_eq!(visual_rect((10.0, 10.0), (12.0, 12.0), 0.0), None);
        assert_eq!(
            visual_rect((10.0, 10.0), (30.0, 50.0), 0.0),
            Some(Rect::new(10.0, 10.0, 20.0, 40.0))
        );
    }

    #[test]
    fn the_band_is_drawn_against_the_scroll_and_clipped_to_the_top() {
        // Started at content y 100, the list now scrolled to 150: the start
        // is above the viewport, so the band starts at the top.
        assert_eq!(
            visual_rect((40.0, 100.0), (20.0, 300.0), 150.0),
            Some(Rect::new(20.0, 0.0, 20.0, 150.0))
        );
        assert_eq!(
            visual_rect((40.0, 10.0), (20.0, 50.0), 300.0),
            None,
            "nothing of it in view"
        );
    }

    #[test]
    fn the_auto_scroll_moves_a_fifth_of_the_distance_into_a_zone() {
        assert_eq!(auto_scroll_step(30.0, 600.0), -10.0);
        assert_eq!(auto_scroll_step(80.0, 600.0), 0.0);
        assert_eq!(auto_scroll_step(300.0, 600.0), 0.0);
        assert_eq!(auto_scroll_step(590.0, 600.0), 10.0);
    }
}
