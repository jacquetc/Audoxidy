//! The artist and album header rows of the list modes.
//!
//! Ports of `artist_header_widget` (`src/gui/widgets.rs:2229-2310`),
//! `album_header_widget` (`:2313-2388`) and `chevron_btn` (`:2199-2226`).
//! A header is a row of the list's height (32 px, 42 px in Thumbnail mode;
//! always 32 px for the album headers of Detailed mode) on `COLOR_CONTRAST`,
//! 15 px in on the leading side and 6 px on the trailing one, holding:
//!
//! - the name in Inter Medium (artist headers 16 px in a 42 px row, 15 px
//!   otherwise), taking the room left by the rest. A selected header is
//!   primary with a primary " •" after it; a header whose songs hold the
//!   playing track is accent with an accent " •"; any other is primary with
//!   no dot. The name is cut with an ellipsis where the room ends (Iced cuts
//!   at `0.55 × size` px per character, `widgets.rs:936-959`);
//! - the figures in Inter Medium 14 px, secondary, 10 px after the name
//!   ("N Canciones | N Álbumes | 12:34", or "N Canciones | 12:34" for an
//!   album);
//! - 15 px, then the chevron: a square button as tall as the row with the
//!   `row height − 4` px chevron centred in the secondary colour, pointing
//!   down while the group is collapsed and up while it is open.
//!
//! A click on the row selects the header (and a second one within 500 ms
//! toggles it, in the view-model); the chevron toggles it. The chevron is a
//! [`Pressable`], so it also answers Enter, Space and the
//! assistive-technology click.

use std::rc::Rc;

use teksilo::core::Modifiers;
use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, VAlignment};
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, IconWidget, Padding, RectWidget, Spacer, TextWidget, ZStack,
};

use crate::teksilo_ui::icons;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;
use crate::utils::format_duration;

/// The leading padding of a header (`widgets.rs:2297`).
pub const LEADING: f32 = 15.0;
/// The trailing padding of a header (`widgets.rs:2298`).
pub const TRAILING: f32 = 6.0;
/// The space before the figures (`widgets.rs:2286`).
pub const STATS_GAP: f32 = 10.0;
/// The space before the chevron (`widgets.rs:2292`).
pub const CHEVRON_GAP: f32 = 15.0;
/// The figures' text size (`widgets.rs:2280`).
pub const STATS_SIZE: f32 = 14.0;
/// What the chevron is smaller than the row (`widgets.rs:2293`).
pub const CHEVRON_INSET: f32 = 4.0;

/// The name's size in a header `height` px tall (`widgets.rs:2259`).
pub fn name_size(height: f32) -> f32 {
    if height > 40.0 { 16.0 } else { 15.0 }
}

/// The name's colour and whether the " •" shows (`widgets.rs:2248-2254`).
pub fn name_look(selected: bool, playing: bool) -> (TextRole, bool) {
    if selected {
        (TextRole::Primary, true)
    } else if playing {
        (TextRole::Accent, true)
    } else {
        (TextRole::Primary, false)
    }
}

/// The callback of a click on a header: the click's modifiers.
pub type SelectFn = Rc<dyn Fn(Modifiers, &mut EventContext)>;
/// The callback of the chevron.
pub type ToggleFn = Rc<dyn Fn(&mut EventContext)>;

/// What a header row shows and does.
pub struct HeaderSpec {
    /// The group's name.
    pub name: String,
    /// The figures after the name.
    pub stats: LocalizedString,
    /// Whether the group is open (the chevron points up).
    pub open: bool,
    /// The row's height.
    pub height: f32,
    /// The name's size.
    pub name_size: f32,
    /// Whether the header is selected.
    pub selected: Signal<bool>,
    /// Whether the group holds the playing track.
    pub playing: Signal<bool>,
    /// A click on the row.
    pub on_select: SelectFn,
    /// The chevron.
    pub on_toggle: ToggleFn,
}

impl std::fmt::Debug for HeaderSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderSpec")
            .field("name", &self.name)
            .field("open", &self.open)
            .field("height", &self.height)
            .finish_non_exhaustive()
    }
}

/// The figures of an artist header: "N Canciones | N Álbumes | 12:34"
/// (`widgets.rs:2276`).
pub fn artist_stats(songs: usize, albums: usize, duration_secs: f64) -> LocalizedString {
    tr!(library_artist_stats(
        songs = count(songs),
        songs_text = songs.to_string(),
        albums = count(albums),
        albums_text = albums.to_string(),
        duration = format_duration(duration_secs)
    ))
}

/// The figures of an album header: "N Canciones | 12:34" (`widgets.rs:2357`).
pub fn album_stats(songs: usize, duration_secs: f64) -> LocalizedString {
    tr!(library_album_stats(
        songs = count(songs),
        songs_text = songs.to_string(),
        duration = format_duration(duration_secs)
    ))
}

/// A count as a Fluent number, for the plural selectors. The messages show
/// the count through a `*_text` argument, the plain digits Iced writes: a
/// Fluent number would be written with the locale's grouping ("27,797").
pub fn count(n: usize) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// A header row; see the module documentation.
pub struct HeaderRow {
    spec: Option<HeaderSpec>,
    label: String,
    stats: LocalizedString,
    open: bool,
    selected: Signal<bool>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for HeaderRow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeaderRow")
            .field("spec", &self.spec)
            .finish_non_exhaustive()
    }
}

impl HeaderRow {
    /// A header of `spec`.
    pub fn new(spec: HeaderSpec) -> Self {
        Self {
            label: spec.name.clone(),
            stats: spec.stats.clone(),
            open: spec.open,
            selected: spec.selected.clone(),
            spec: Some(spec),
            root: None,
        }
    }

    /// An artist header (`artist_header_widget`).
    #[allow(clippy::too_many_arguments)]
    pub fn artist(
        name: &str,
        collapsed: bool,
        albums: usize,
        songs: usize,
        duration_secs: f64,
        height: f32,
        selected: Signal<bool>,
        playing: Signal<bool>,
        on_select: SelectFn,
        on_toggle: ToggleFn,
    ) -> Self {
        Self::new(HeaderSpec {
            name: name.to_string(),
            stats: artist_stats(songs, albums, duration_secs),
            open: !collapsed,
            height,
            name_size: name_size(height),
            selected,
            playing,
            on_select,
            on_toggle,
        })
    }

    /// An album header (`album_header_widget`): its name is always 15 px.
    #[allow(clippy::too_many_arguments)]
    pub fn album(
        name: &str,
        expanded: bool,
        songs: usize,
        duration_secs: f64,
        height: f32,
        selected: Signal<bool>,
        playing: Signal<bool>,
        on_select: SelectFn,
        on_toggle: ToggleFn,
    ) -> Self {
        Self::new(HeaderSpec {
            name: name.to_string(),
            stats: album_stats(songs, duration_secs),
            open: expanded,
            height,
            name_size: 15.0,
            selected,
            playing,
            on_select,
            on_toggle,
        })
    }
}

/// The chevron of an open (`up`) or a closed group.
fn chevron(up: bool) -> IconWidget {
    if up {
        icons::shared::arrow_up_chevron()
    } else {
        icons::shared::arrow_down_chevron()
    }
}

impl Widget for HeaderRow {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let Some(spec) = self.spec.take() else {
            return self.root.into_iter().collect();
        };
        let look = spec
            .selected
            .zip(&spec.playing)
            .map(|(selected, playing)| name_look(*selected, *playing));
        let role = look.map(|(role, _)| *role);
        let dot = look.map(|(_, dot)| *dot);
        let name = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(
                TextWidget::new(lit!(spec.name.clone()))
                    .style(style::inter(spec.name_size, FontWeight::MEDIUM))
                    .color(role.clone())
                    .single_line()
                    .a11y_hidden(),
            )
            .child(
                TextWidget::new(lit!(" •"))
                    .style(style::inter(spec.name_size, FontWeight::MEDIUM))
                    .color(role)
                    .single_line()
                    .no_shrink()
                    .a11y_hidden()
                    .visible_when(dot),
            )
            .child(Spacer::new());
        let stats = TextWidget::new(spec.stats.clone())
            .style(style::inter(STATS_SIZE, FontWeight::MEDIUM))
            .color(TextRole::Secondary)
            .single_line()
            .no_shrink();
        let on_toggle = spec.on_toggle.clone();
        let toggle_label = if spec.open {
            tr!(library_collapse(name = spec.name.clone()))
        } else {
            tr!(library_expand(name = spec.name.clone()))
        };
        let chevron_button = Pressable::new(
            FixedSize::new()
                .width(spec.height)
                .height(spec.height)
                .child(
                    Center::new().child(
                        chevron(spec.open)
                            .icon_size(spec.height - CHEVRON_INSET)
                            .color(TextRole::Secondary),
                    ),
                ),
            toggle_label,
        )
        .on_activate(move |ctx| on_toggle(ctx));
        let content = Padding::new(0.0, TRAILING, 0.0, LEADING).child(
            HStack::new()
                .spacing(0.0)
                .alignment(VAlignment::Center)
                .child(Expand::new().child(name))
                .child(Padding::new(0.0, 0.0, 0.0, STATS_GAP).child(stats))
                .child(
                    FixedSize::new()
                        .width(CHEVRON_GAP)
                        .height(0.0)
                        .child(Spacer::new()),
                )
                .child(chevron_button),
        );
        let on_select = spec.on_select.clone();
        self.selected.bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::AccessibilityOnly,
        );
        ctx.apply_self_handlers(
            HandlerSet::new()
                .on_tap(move |tap, ctx| on_select(tap.modifiers, ctx))
                .cursor(CursorIcon::Pointer),
        );
        let root = ctx.add(
            FixedSize::new().height(spec.height).child(
                ZStack::new()
                    .child(RectWidget::new().background(SurfaceRole::Raised))
                    .child(content),
            ),
        );
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

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(Role::Row);
        builder.set_name(self.label.clone());
        builder.set_description(self.stats.resolve_now());
        builder.set_selected(self.selected.get());
        builder.set_expanded(self.open);
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use teksilo::core::WidgetTree;

    use super::*;
    use crate::teksilo_ui::library::test_util::{all_of_type, mount_at, plain, settle_layout};
    use crate::teksilo_ui::test_support::{access_click, find_by_name, with_shipped_messages};

    fn mounted(
        height: f32,
        selected: bool,
        playing: bool,
    ) -> (WidgetTree, WidgetId, Rc<Cell<u32>>, Rc<Cell<u32>>) {
        let (selects, toggles) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
        let (s, t) = (selects.clone(), toggles.clone());
        let header = HeaderRow::artist(
            "David Bowie",
            false,
            28,
            220,
            3_600.0,
            height,
            Signal::new(selected),
            Signal::new(playing),
            Rc::new(move |_m: Modifiers, _c: &mut EventContext| s.set(s.get() + 1)),
            Rc::new(move |_c: &mut EventContext| t.set(t.get() + 1)),
        );
        let mut tree = WidgetTree::new();
        tree.set_theme(style::audoxidy_dark());
        let id = mount_at(&mut tree, header, 700.0, height);
        settle_layout(&mut tree);
        (tree, id, selects, toggles)
    }

    #[test]
    fn the_chevron_is_a_square_as_tall_as_the_row_at_its_trailing_end() {
        for height in [32.0, 42.0] {
            let (tree, id, _, _) = mounted(height, false, false);
            let row = tree.bounds(id);
            assert_eq!(row.height, height);
            let chevron = all_of_type(&tree, id, "Pressable")[0];
            let bounds = tree.bounds(chevron);
            assert_eq!((bounds.width, bounds.height), (height, height));
            assert_eq!(row.x + row.width - (bounds.x + bounds.width), TRAILING);
        }
    }

    #[test]
    fn the_name_is_bigger_in_a_42_px_row_and_lit_by_selection_or_playback() {
        assert_eq!(name_size(32.0), 15.0);
        assert_eq!(name_size(42.0), 16.0);
        assert_eq!(name_look(true, true), (TextRole::Primary, true));
        assert_eq!(name_look(false, true), (TextRole::Accent, true));
        assert_eq!(name_look(false, false), (TextRole::Primary, false));
    }

    #[test]
    fn the_chevron_toggles_and_the_row_reports_selection_and_expansion() {
        let (mut tree, id, _selects, toggles) = mounted(32.0, true, false);
        let chevron = find_by_name(&tree, id, "Collapse \u{2068}David Bowie\u{2069}")
            .into_iter()
            .chain(find_by_name(&tree, id, "Collapse David Bowie"))
            .next()
            .unwrap();
        access_click(&mut tree, chevron);
        assert_eq!(toggles.get(), 1);
        let node = tree.accessibility_node(id);
        assert_eq!(node.name(), Some("David Bowie"));
        assert!(node.is_selected());
    }

    #[test]
    fn the_figures_read_as_in_iced_with_correct_plurals() {
        assert_eq!(
            plain(&artist_stats(220, 28, 3_600.0).resolve_now()),
            "220 Songs | 28 Albums | 01:00:00"
        );
        with_shipped_messages("es-ES", || {
            assert_eq!(
                plain(&artist_stats(220, 28, 3_600.0).resolve_now()),
                "220 Canciones | 28 Álbumes | 01:00:00"
            );
            assert_eq!(
                plain(&album_stats(1, 61.0).resolve_now()),
                "1 Canción | 01:01"
            );
            assert_eq!(
                plain(&tr!(library_collapse(name = "X".to_string())).resolve_now()),
                "Plegar X"
            );
        });
    }
}
