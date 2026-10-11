//! The title bar: the 40 px strip above the filter column and the library
//! (`src/gui/library_filters.rs:164` and `src/gui/library.rs:1300`).
//!
//! From leading to trailing edge:
//!
//! - the logo, "AuDoxiDY" in Stage Wander 19 px, centred in the 202 px of the
//!   filter column below it, 9 px from the top;
//! - 15 px in, the library source pills: "Biblioteca", then the streaming
//!   services, 10 px apart, Inter Medium 15 px. The active pill is filled
//!   with the source's colour (radius 14, padding 2 × 10) and its text is
//!   white; the others are in the secondary text colour;
//! - the drag region;
//! - the window controls, `minimize.svg` (30 px, lifted 4 px), `maximize.svg`
//!   (30 px) and `close.svg` (34 px), 5 px apart, 5 px from the edge.
//!
//! The streaming pills are disabled: in the Iced UI they only move the
//! highlight (no streaming source exists).
//!
//! With custom chrome the strip is a Teksilo `TitleBar`, so it moves the
//! window and a double click maximizes it: the Iced window can be neither
//! moved nor resized (quirk fix 32). Its own window controls are hidden in
//! favour of the Audoxidy ones. Where the platform keeps native decorations,
//! the same strip is drawn without the drag region and the window controls,
//! which the OS then provides.

use std::rc::Rc;

use teksilo::core::PlatformTitleBarHost;
use teksilo::core::accesskit::Role;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    FixedSize, HStack, IconWidget, Padding, Panel, Spacer, TextWidget, TitleBar, VStack,
};

use crate::teksilo_ui::icons;
use crate::teksilo_ui::style::{self, source_colors};
use crate::teksilo_ui::widgets::{BrandLogo, Pressable};

/// The strip's height (`library.rs:1386`).
pub const HEIGHT: f32 = 40.0;
/// The width of the logo cell: the filter column's width.
pub const LOGO_WIDTH: f32 = 202.0;
/// The logo's distance from the top of the strip.
const LOGO_TOP: f32 = 9.0;
/// The logo's size.
const LOGO_SIZE: f32 = 19.0;
/// The space before the first pill.
const PILLS_LEADING: f32 = 15.0;
/// The space between pills.
const PILL_SPACING: f32 = 10.0;
/// The pill text size.
const PILL_TEXT_SIZE: f32 = 15.0;
/// The active pill's corner radius.
const PILL_RADIUS: f32 = 14.0;
/// The pill padding: 2 px above and below, 10 px on the sides.
const PILL_PADDING: (f32, f32) = (2.0, 10.0);
/// The space between window controls.
const CONTROL_SPACING: f32 = 5.0;
/// The space after the last window control.
const CONTROLS_TRAILING: f32 = 5.0;
/// The minimize and maximize icon size.
const CONTROL_SIZE: f32 = 30.0;
/// The close icon size.
const CLOSE_SIZE: f32 = 34.0;
/// The padding under the minimize icon, which lifts it by half as much.
const MINIMIZE_LIFT: f32 = 8.0;

/// A library source, in pill order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The local library ("Biblioteca").
    Local,
    /// Spotify.
    Spotify,
    /// YouTube.
    YouTube,
    /// Qobuz.
    Qobuz,
    /// Deezer.
    Deezer,
    /// Tidal.
    Tidal,
}

impl Source {
    /// Every source, in pill order.
    pub const ALL: [Source; 6] = [
        Source::Local,
        Source::Spotify,
        Source::YouTube,
        Source::Qobuz,
        Source::Deezer,
        Source::Tidal,
    ];

    /// The pill's label. The streaming services are brand names, shown as is.
    pub fn label(self) -> LocalizedString {
        match self {
            Source::Local => tr!(titlebar_source_library()),
            Source::Spotify => lit!("Spotify"),
            Source::YouTube => lit!("YouTube"),
            Source::Qobuz => lit!("Qobuz"),
            Source::Deezer => lit!("Deezer"),
            Source::Tidal => lit!("Tidal"),
        }
    }

    /// The colour of the pill when it is the active source.
    pub fn color(self) -> Color {
        match self {
            Source::Local => source_colors::LOCAL,
            Source::Spotify => source_colors::SPOTIFY,
            Source::YouTube => source_colors::YOUTUBE,
            Source::Qobuz => source_colors::QOBUZ,
            Source::Deezer => source_colors::DEEZER,
            Source::Tidal => source_colors::TIDAL,
        }
    }

    /// Whether the source can be chosen. Only the local library exists.
    pub fn is_available(self) -> bool {
        // TODO(iced-stub): the Iced UI highlights a streaming pill when clicked (`LibrarySourceSelected`) and changes nothing else; no streaming source exists, so these pills are disabled.
        self == Source::Local
    }
}

/// The source the library shows. Always the local library.
const ACTIVE_SOURCE: Source = Source::Local;

/// The title bar for a window drawn with custom chrome.
pub fn custom(host: Rc<dyn PlatformTitleBarHost>) -> TitleBar {
    TitleBar::new(host)
        .height(HEIGHT)
        .background(SurfaceRole::Raised)
        .controls_visible(false)
        .leading(leading())
        .trailing(window_controls())
}

/// The same strip for a window with native decorations: no drag region and no
/// window controls (the OS draws them).
pub fn native() -> FixedSize {
    FixedSize::new().height(HEIGHT).child(
        Panel::new()
            .padding(0.0)
            .border_width(0.0)
            .background(SurfaceRole::Raised)
            .child(
                HStack::new()
                    .spacing(0.0)
                    .alignment(VAlignment::Center)
                    .child(leading())
                    .child(Spacer::new()),
            ),
    )
}

/// The logo cell and the source pills.
fn leading() -> HStack {
    HStack::new()
        .spacing(0.0)
        .alignment(VAlignment::Center)
        .child(logo())
        .child(
            Padding::new(0.0, 0.0, 0.0, PILLS_LEADING).child(
                HStack::new()
                    .spacing(PILL_SPACING)
                    .alignment(VAlignment::Center)
                    .children(Source::ALL.into_iter().map(pill))
                    .access_role(Role::TabList)
                    .access_label(tr!(titlebar_sources())),
            ),
        )
}

/// "AuDoxiDY", centred in the filter column's width.
fn logo() -> FixedSize {
    FixedSize::new().width(LOGO_WIDTH).height(HEIGHT).child(
        Padding::new(LOGO_TOP, 0.0, 0.0, 0.0).child(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Center)
                .child(BrandLogo::new(LOGO_SIZE)),
        ),
    )
}

/// One source pill (`library.rs:1300`).
fn pill(source: Source) -> Pressable {
    let active = source == ACTIVE_SOURCE;
    let text_color: ColorProp = if active {
        Color::WHITE.into()
    } else {
        TextRole::Secondary.into()
    };
    let background: ColorProp = if active {
        source.color().into()
    } else {
        SurfaceRole::Transparent.into()
    };
    let body = Panel::new()
        .padding(0.0)
        .border_width(0.0)
        .background(background)
        .corner_radius(PILL_RADIUS)
        .child(
            Padding::symmetric(PILL_PADDING.0, PILL_PADDING.1).child(
                TextWidget::new(source.label())
                    .style(style::inter(PILL_TEXT_SIZE, FontWeight::MEDIUM))
                    .color(text_color)
                    .single_line(),
            ),
        );
    Pressable::new(body, source.label())
        .tab(active)
        .enabled(source.is_available())
        .corner_radius(PILL_RADIUS)
}

/// Minimize, maximize and close (`library.rs:1361`).
fn window_controls() -> Padding {
    let minimize = Padding::new(0.0, 0.0, MINIMIZE_LIFT, 0.0).child(control(
        icons::window::minimize(),
        CONTROL_SIZE,
        tr!(titlebar_minimize()),
        |ctx| {
            if let Some(window) = ctx.window() {
                window.placement().set(WindowPlacement::Minimized);
            }
        },
    ));
    let maximize = control(
        icons::window::maximize(),
        CONTROL_SIZE,
        tr!(titlebar_maximize()),
        |ctx| {
            if let Some(window) = ctx.window() {
                let placement = window.placement();
                placement.set(toggled_maximize(placement.get()));
            }
        },
    );
    let close = control(
        icons::window::close(),
        CLOSE_SIZE,
        tr!(titlebar_close()),
        |ctx| ctx.close_window(),
    );
    Padding::new(0.0, CONTROLS_TRAILING, 0.0, 0.0).child(
        HStack::new()
            .spacing(CONTROL_SPACING)
            .alignment(VAlignment::Center)
            .child(minimize)
            .child(maximize)
            .child(close),
    )
}

/// The placement a click on maximize moves to: maximized, or back to a
/// floating window when it already is (Iced's `toggle_maximize`).
pub fn toggled_maximize(current: WindowPlacement) -> WindowPlacement {
    if current.is_maximized() {
        WindowPlacement::Floating
    } else {
        WindowPlacement::Maximized
    }
}

/// One window control: the icon at `size` px in the icon colour.
fn control(
    icon: IconWidget,
    size: f32,
    label: LocalizedString,
    action: impl Fn(&mut EventContext) + 'static,
) -> Pressable {
    let body = FixedSize::new()
        .width(size)
        .height(size)
        .child(icon.icon_size(size).color(style::ICON_ROLE));
    Pressable::new(body, label).on_activate(action)
}

#[cfg(test)]
mod tests {
    use teksilo::core::WidgetTree;
    use teksilo::core::accesskit::Role;

    use super::*;
    use crate::teksilo_ui::test_support::{find_by_name, layout, with_shipped_messages};

    /// The native strip at the top of a window-sized column (a tree's root
    /// always takes the whole viewport).
    fn strip() -> (WidgetTree, WidgetId) {
        let mut tree = WidgetTree::new();
        tree.set_theme(style::audoxidy_dark());
        let id = tree.add(native());
        tree.add(
            VStack::new()
                .spacing(0.0)
                .alignment(HAlignment::Leading)
                .child(id)
                .child(Spacer::new()),
        );
        layout(&mut tree);
        (tree, id)
    }

    fn tab_named(tree: &WidgetTree, root: WidgetId, name: &str) -> WidgetId {
        find_by_name(tree, root, name)
            .into_iter()
            .find(|id| tree.accessibility_node(*id).role() == Role::Tab)
            .unwrap()
    }

    #[test]
    fn the_strip_is_forty_pixels_high() {
        let (tree, id) = strip();
        assert_eq!(tree.bounds(id).height, HEIGHT);
    }

    #[test]
    fn the_pills_start_fifteen_pixels_after_the_logo_cell() {
        with_shipped_messages("en-US", || {
            let (tree, id) = strip();
            let library = tab_named(&tree, id, "Library");
            assert_eq!(tree.bounds(library).x, LOGO_WIDTH + PILLS_LEADING);
        });
    }

    #[test]
    fn the_spanish_strip_names_the_local_library_biblioteca() {
        with_shipped_messages("es-ES", || {
            let (tree, id) = strip();
            let library = tab_named(&tree, id, "Biblioteca");
            assert!(tree.accessibility_node(library).is_selected());
        });
    }

    #[test]
    fn only_the_local_library_is_available_and_it_is_the_selected_tab() {
        with_shipped_messages("en-US", || {
            let (tree, id) = strip();
            for source in Source::ALL {
                assert_eq!(source.is_available(), source == Source::Local);
            }
            let library = tree.accessibility_node(tab_named(&tree, id, "Library"));
            assert!(!library.is_disabled());
            assert!(library.is_selected());
            for name in ["Spotify", "YouTube", "Qobuz", "Deezer", "Tidal"] {
                let pill = tree.accessibility_node(tab_named(&tree, id, name));
                assert!(pill.is_disabled(), "{name}");
                assert!(!pill.is_selected(), "{name}");
            }
        });
    }

    #[test]
    fn maximize_toggles_between_maximized_and_floating() {
        assert_eq!(
            toggled_maximize(WindowPlacement::Floating),
            WindowPlacement::Maximized
        );
        assert_eq!(
            toggled_maximize(WindowPlacement::Maximized),
            WindowPlacement::Floating
        );
        assert_eq!(
            toggled_maximize(WindowPlacement::Fullscreen),
            WindowPlacement::Maximized
        );
    }
}
