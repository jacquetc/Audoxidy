//! [`IconAction`]: an SVG icon that does something when clicked.
//!
//! The Iced UI has two of these (`src/gui/widgets.rs`):
//!
//! - `action_icon_button` (`widgets.rs:1169`): the icon at a size, in a
//!   `mouse_area` with a pointer cursor; nothing changes on hover. The player's
//!   `menu.svg` is one ([`IconAction::new`]).
//! - `icon_button` (`widgets.rs:1189`): a square button `max(size + 4, 20)`
//!   wide whose icon fills it; on hover the icon turns primary and, when the
//!   button is enabled, the square fills with the accent (radius 4). The
//!   Audio Center's preset load / save / reset icons are these
//!   ([`IconAction::button`]).
//!
//! Both are built on [`Pressable`], so they also take the keyboard focus,
//! activate on Enter and Space and on the assistive-technology click, and
//! carry an accessible name, which the Iced versions lack. The icon comes from
//! the caller (an `icons.rs` factory); with no colour of its own it is drawn in
//! [`style::ICON_ROLE`].

use std::rc::Rc;

use teksilo::prelude::*;
use teksilo::widgets::{FixedSize, IconWidget, Padding, RectWidget, ZStack};

use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;
use crate::teksilo_ui::widgets::pressable::ActivateFn;

/// The smallest `icon_button` square (`widgets.rs:1195`).
pub const BUTTON_MIN_SIZE: f32 = 20.0;
/// What an `icon_button` adds to the icon size (`widgets.rs:1195`).
pub const BUTTON_GROWTH: f32 = 4.0;
/// The hover fill's corner radius (`widgets.rs:1219`).
pub const BUTTON_RADIUS: f32 = 4.0;

/// The side of the square an Iced `icon_button` of `size` occupies.
pub fn button_side(size: f32) -> f32 {
    (size + BUTTON_GROWTH).max(BUTTON_MIN_SIZE)
}

/// The fill and icon colour of an Iced `icon_button` (`widgets.rs:1200-1225`):
/// the accent fill only while hovered and enabled, the icon primary while
/// hovered and secondary otherwise.
pub fn button_colors(hovered: bool, enabled: bool) -> (SurfaceRole, TextRole) {
    let fill = if hovered && enabled {
        SurfaceRole::Accent
    } else {
        SurfaceRole::Transparent
    };
    let icon = if hovered {
        TextRole::Primary
    } else {
        TextRole::Secondary
    };
    (fill, icon)
}

/// Which of the two Iced shapes.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Kind {
    /// `action_icon_button`: the bare icon.
    Plain,
    /// `icon_button`: the hover-filled square.
    Button,
}

/// A clickable SVG icon; see the module documentation.
pub struct IconAction {
    icon: Option<IconWidget>,
    size: f32,
    kind: Kind,
    label: LocalizedString,
    color: ColorProp,
    padding: f32,
    enabled: Prop<bool>,
    on_activate: Option<ActivateFn>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for IconAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IconAction")
            .field("label", &self.label.resolve_now())
            .field("size", &self.size)
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

impl IconAction {
    /// An Iced `action_icon_button`: `icon` at `size` px in the icon colour,
    /// announced as `label`.
    pub fn new(icon: IconWidget, size: f32, label: impl Into<LocalizedString>) -> Self {
        Self {
            icon: Some(icon),
            size,
            kind: Kind::Plain,
            label: label.into(),
            color: style::ICON_ROLE.into(),
            padding: 0.0,
            enabled: Prop::Static(true),
            on_activate: None,
            root: None,
        }
    }

    /// An Iced `icon_button` for an icon of `size` px: a
    /// [`button_side`]`(size)` square that the icon fills, lit on hover (see
    /// [`button_colors`]).
    pub fn button(icon: IconWidget, size: f32, label: impl Into<LocalizedString>) -> Self {
        Self {
            kind: Kind::Button,
            ..Self::new(icon, size, label)
        }
    }

    /// What a click, Enter, Space or the assistive-technology click does.
    pub fn on_activate(mut self, f: impl Fn(&mut EventContext) + 'static) -> Self {
        self.on_activate = Some(Rc::new(f));
        self
    }

    /// Whether it can be activated (default `true`). A disabled icon takes no
    /// focus, ignores every activation path, and an `icon_button` no longer
    /// fills on hover.
    pub fn enabled(mut self, enabled: impl Into<Prop<bool>>) -> Self {
        self.enabled = enabled.into();
        self
    }

    /// The icon colour of a plain icon (default [`style::ICON_ROLE`]); a
    /// signal of a role follows a state, as the playlist's shuffle and
    /// repeat icons do. An `icon_button` keeps the Iced hover colours.
    pub fn color(mut self, color: impl Into<ColorProp>) -> Self {
        self.color = color.into();
        self
    }

    /// Space around a plain icon, inside the clickable area (an Iced
    /// `button(icon).padding(p)`).
    pub fn padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// The side of the clickable square.
    pub fn side(&self) -> f32 {
        match self.kind {
            Kind::Plain => self.size + 2.0 * self.padding,
            Kind::Button => button_side(self.size),
        }
    }

    fn enabled_signal(&self, ctx: &mut BuildContext) -> Signal<bool> {
        match &self.enabled {
            Prop::Static(value) => ctx.signal(*value),
            Prop::Bound(signal) => signal.clone(),
        }
    }
}

impl Widget for IconAction {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let enabled = self.enabled_signal(ctx);
        let hovered = ctx.signal(false);
        let side = self.side();
        let Some(icon) = self.icon.take() else {
            return self.root.into_iter().collect();
        };
        let label = self.label.clone();
        let pressable = match self.kind {
            Kind::Plain => Pressable::new(
                Padding::uniform(self.padding).child(
                    FixedSize::new()
                        .width(self.size)
                        .height(self.size)
                        .child(icon.icon_size(self.size).color(self.color.clone())),
                ),
                label,
            ),
            Kind::Button => {
                let colors = hovered.zip(&enabled).map(|(h, e)| button_colors(*h, *e));
                let fill = colors.map(|(fill, _)| *fill);
                let tint = colors.map(|(_, icon)| *icon);
                Pressable::new(
                    FixedSize::new().width(side).height(side).child(
                        ZStack::new()
                            .child(
                                RectWidget::new()
                                    .background(fill)
                                    .corner_radius(CornerRadius::uniform(BUTTON_RADIUS)),
                            )
                            .child(icon.icon_size(side).color(tint)),
                    ),
                    label,
                )
                .corner_radius(BUTTON_RADIUS)
            }
        };
        let mut pressable = pressable
            .enabled(Prop::Bound(enabled.clone()))
            .hovered(hovered.clone());
        if let Some(action) = self.on_activate.take() {
            pressable = pressable.on_activate(move |ctx| action(ctx));
        }
        let root = ctx.add(pressable);
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
}

#[cfg(test)]
mod tests;
