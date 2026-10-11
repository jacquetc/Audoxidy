//! [`Pressable`]: button semantics around any content.
//!
//! The Iced UI makes many things clickable by wrapping a drawing in a
//! `mouse_area`: the window controls, the library source pills, the icons of
//! the bottom bars. Those have no keyboard access and no accessible name. A
//! `Pressable` keeps the drawing exactly as it is and adds what a button needs:
//!
//! - activation by a primary click, by Enter or Space while focused, and by
//!   the assistive-technology `Click` action;
//! - keyboard focus, with a focus ring drawn over the content when the focus
//!   came from the keyboard;
//! - an accessible role (button, or tab for a tab strip), name, selected and
//!   disabled state;
//! - a disabled state that blocks every activation path;
//! - a hover signal the content can bind its colours to.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::accesskit::{Action, Role};
use teksilo::core::binding::BindingLevel;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{Key, WidgetEvent};
use teksilo::prelude::*;
use teksilo::widgets::Spacer;

/// Width of the keyboard focus ring, in logical pixels.
const FOCUS_RING_WIDTH: f32 = 1.5;

/// The activation callback.
pub type ActivateFn = Rc<dyn Fn(&mut EventContext)>;

/// Button semantics around arbitrary content; see the module documentation.
pub struct Pressable {
    content: Option<Box<dyn Widget>>,
    label: LocalizedString,
    role: Role,
    on_activate: Option<ActivateFn>,
    enabled: Prop<bool>,
    selected: Option<Prop<bool>>,
    hovered: Signal<bool>,
    focused: Signal<bool>,
    focus_visible: Option<Signal<bool>>,
    enabled_now: Rc<Cell<bool>>,
    corner_radius: f32,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for Pressable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pressable")
            .field("label", &self.label.resolve_now())
            .field("role", &self.role)
            .field("enabled", &self.enabled_now.get())
            .finish_non_exhaustive()
    }
}

impl Pressable {
    /// A button showing `content`, announced as `label`.
    pub fn new(content: impl Widget + 'static, label: impl Into<LocalizedString>) -> Self {
        Self {
            content: Some(Box::new(content)),
            label: label.into(),
            role: Role::Button,
            on_activate: None,
            enabled: Prop::Static(true),
            selected: None,
            hovered: Signal::new(false),
            focused: Signal::new(false),
            focus_visible: None,
            enabled_now: Rc::new(Cell::new(true)),
            corner_radius: 0.0,
            root: None,
        }
    }

    /// What activation does.
    pub fn on_activate(mut self, f: impl Fn(&mut EventContext) + 'static) -> Self {
        self.on_activate = Some(Rc::new(f));
        self
    }

    /// Whether it can be activated (default `true`). A disabled pressable
    /// takes no focus and ignores every activation path.
    pub fn enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    /// Announces it as a tab of a tab strip, selected when `selected` is true.
    pub fn tab(mut self, selected: impl Into<Prop<bool>>) -> Self {
        self.role = Role::Tab;
        self.selected = Some(selected.into());
        self
    }

    /// Writes `true` to `hovered` while the pointer is over it.
    pub fn hovered(mut self, hovered: Signal<bool>) -> Self {
        self.hovered = hovered;
        self
    }

    /// The corner radius of the focus ring (default square).
    pub fn corner_radius(mut self, radius: f32) -> Self {
        self.corner_radius = radius;
        self
    }

    fn activate(enabled: &Cell<bool>, action: Option<&ActivateFn>, ctx: &mut EventContext) {
        if enabled.get()
            && let Some(action) = action
        {
            action(ctx);
        }
    }

    fn prop_signal(ctx: &mut BuildContext, prop: &Prop<bool>) -> Signal<bool> {
        match prop {
            Prop::Static(value) => ctx.signal(*value),
            Prop::Bound(signal) => signal.clone(),
        }
    }
}

impl Widget for Pressable {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let self_id = ctx.self_id();
        let enabled = Self::prop_signal(ctx, &self.enabled);
        self.enabled_now.set(enabled.get());
        let enabled_now = self.enabled_now.clone();
        ctx.effect(&enabled, move |on| enabled_now.set(*on));
        // Disabling also takes it out of the Tab order (pinned by a test).
        ctx.enabled_when(self_id, Prop::Bound(enabled.clone()));
        enabled.bind_to(self_id, ctx.binding_registry(), BindingLevel::RepaintOnly);
        if let Some(selected) = &self.selected {
            let selected = Self::prop_signal(ctx, selected);
            selected.bind_to(self_id, ctx.binding_registry(), BindingLevel::RepaintOnly);
            self.selected = Some(Prop::Bound(selected));
        }

        let focus_visible = ctx.focus_visible();
        focus_visible.bind_to(self_id, ctx.binding_registry(), BindingLevel::RepaintOnly);
        self.focused
            .bind_to(self_id, ctx.binding_registry(), BindingLevel::RepaintOnly);
        self.focus_visible = Some(focus_visible);

        let tap = (self.enabled_now.clone(), self.on_activate.clone());
        let key = (self.enabled_now.clone(), self.on_activate.clone());
        let access = (self.enabled_now.clone(), self.on_activate.clone());
        let hovered = self.hovered.clone();
        let focused = self.focused.clone();
        let cursor = if self.enabled_now.get() {
            CursorIcon::Pointer
        } else {
            CursorIcon::Default
        };
        ctx.apply_self_handlers(
            HandlerSet::new()
                .focusable(true)
                .cursor(cursor)
                .on_tap(move |_event, ctx| Self::activate(&tap.0, tap.1.as_ref(), ctx))
                .on_key(move |event, ctx| match event {
                    WidgetEvent::KeyDown {
                        key: Key::Enter | Key::Space,
                        ..
                    } if key.0.get() => {
                        Self::activate(&key.0, key.1.as_ref(), ctx);
                        EventResponse::Handled
                    }
                    _ => EventResponse::Ignored,
                })
                .on_access_action(move |action, ctx| {
                    if action == Action::Click && access.0.get() {
                        Self::activate(&access.0, access.1.as_ref(), ctx);
                        EventResponse::Handled
                    } else {
                        EventResponse::Ignored
                    }
                })
                .on_hover(move |entered, _ctx| hovered.set(entered))
                .on_focus(move |gained, _ctx| focused.set(gained)),
        );

        let root = match self.content.take() {
            Some(content) => ctx.add_boxed(content),
            None => ctx.add(Spacer::new()),
        };
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, 0.0))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }

    fn wants_post_paint(&self) -> bool {
        true
    }

    fn post_paint(&self, bounds: Rect, canvas: &mut Canvas, ctx: &PaintContext) {
        let visible = self.focus_visible.as_ref().is_some_and(Signal::get);
        if !(self.focused.get() && visible) {
            return;
        }
        let inset = FOCUS_RING_WIDTH / 2.0;
        let ring = Rect::new(
            bounds.x + inset,
            bounds.y + inset,
            (bounds.width - FOCUS_RING_WIDTH).max(0.0),
            (bounds.height - FOCUS_RING_WIDTH).max(0.0),
        );
        canvas.stroke_rounded_rect(
            ring,
            CornerRadius::uniform(self.corner_radius),
            ctx.theme.colors.focus_ring,
            FOCUS_RING_WIDTH,
        );
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(self.role);
        builder.set_name(self.label.resolve_now());
        if let Some(selected) = &self.selected {
            let selected = match selected {
                Prop::Static(value) => *value,
                Prop::Bound(signal) => signal.get(),
            };
            builder.set_selected(selected);
        }
        if self.enabled_now.get() {
            builder.add_action(Action::Click);
        } else {
            builder.set_disabled();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use teksilo::core::WidgetTree;
    use teksilo::core::accesskit::Role;
    use teksilo::widgets::{FixedSize, HStack};

    use super::*;
    use crate::teksilo_ui::test_support::{access_click, key_down};

    fn square() -> FixedSize {
        FixedSize::new()
            .width(30.0)
            .height(30.0)
            .child(Spacer::new())
    }

    /// A pressable counting its activations, in a row (a tree's root always
    /// takes the whole viewport).
    fn counting(tree: &mut WidgetTree, enabled: Signal<bool>) -> (WidgetId, Rc<Cell<u32>>) {
        let count = Rc::new(Cell::new(0));
        let hits = count.clone();
        let id = tree.add(
            Pressable::new(square(), lit!("Close"))
                .enabled(enabled)
                .on_activate(move |_| hits.set(hits.get() + 1)),
        );
        tree.add(HStack::new().child(id).child(Spacer::new()));
        tree.layout(SizeProposal::exact(100.0, 100.0));
        (id, count)
    }

    #[test]
    fn it_takes_the_size_of_its_content() {
        let mut tree = WidgetTree::new();
        let (id, _) = counting(&mut tree, Signal::new(true));
        let bounds = tree.bounds(id);
        assert_eq!((bounds.width, bounds.height), (30.0, 30.0));
    }

    #[test]
    fn the_click_action_enter_and_space_activate_it() {
        let mut tree = WidgetTree::new();
        let (id, count) = counting(&mut tree, Signal::new(true));
        access_click(&mut tree, id);
        key_down(&mut tree, id, Key::Enter);
        key_down(&mut tree, id, Key::Space);
        assert_eq!(count.get(), 3);
    }

    #[test]
    fn a_disabled_pressable_ignores_every_activation_path() {
        let mut tree = WidgetTree::new();
        let enabled = Signal::new(false);
        let (id, count) = counting(&mut tree, enabled.clone());
        access_click(&mut tree, id);
        key_down(&mut tree, id, Key::Enter);
        assert_eq!(count.get(), 0);
        enabled.set(true);
        tree.layout(SizeProposal::exact(100.0, 100.0));
        access_click(&mut tree, id);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn tab_skips_a_disabled_pressable() {
        let mut tree = WidgetTree::new();
        let disabled = tree.add(Pressable::new(square(), lit!("Spotify")).enabled(false));
        let enabled = tree.add(Pressable::new(square(), lit!("Library")));
        tree.add(
            HStack::new()
                .child(disabled)
                .child(enabled)
                .child(Spacer::new()),
        );
        tree.layout(SizeProposal::exact(200.0, 100.0));
        tree.dispatch_event(WidgetEvent::KeyDown {
            key: Key::Tab,
            modifiers: teksilo::core::Modifiers::NONE,
            text: None,
        });
        assert_eq!(tree.focused(), Some(enabled));
    }

    #[test]
    fn it_is_announced_as_a_named_button_or_a_selected_tab() {
        let mut tree = WidgetTree::new();
        let selected = Signal::new(true);
        let row = tree.add(
            HStack::new()
                .child(Pressable::new(square(), lit!("Close")))
                .child(Pressable::new(square(), lit!("Library")).tab(selected.clone())),
        );
        tree.layout(SizeProposal::exact(100.0, 100.0));
        let _ = row;
        let button = tree.find_by_role(Role::Button).unwrap();
        assert_eq!(tree.accessibility_node(button).name(), Some("Close"));
        let tab = tree.find_by_role(Role::Tab).unwrap();
        let tab = tree.accessibility_node(tab);
        assert_eq!(tab.name(), Some("Library"));
        assert!(tab.is_selected());
    }
}
