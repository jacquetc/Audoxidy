//! [`BottomBar`]: the library's bottom bar.
//!
//! Port of the Iced library bar (`src/gui/library.rs:1807-1943`), 40 px of
//! `COLOR_CONTRAST`, 15 px in on both sides, everything centred on its middle:
//!
//! - the search field, 180 px, "Buscar..." ([`SearchField`]; Escape empties
//!   it, quirk fix 35);
//! - 15 px, the figures (Inter Medium 13 px, secondary): "N Canciones |
//!   N Álbumes | N Artistas | 12:34 | 1.23 GB", from the selection, else
//!   from the songs the search and the filters let through, else from the
//!   library totals (`format_duration`, `format_size`), with correct plurals
//!   (quirk fix 15: Iced writes "1 Canciones");
//! - 10 px, **the meter slot**: 400 px wide whether or not a meter is shown
//!   (quirk fix 10: Iced drops the meter and its right-click when the meter is
//!   disabled, `library.rs:1913-1943`); it holds the widget given to
//!   [`LibraryPanel::meter_slot`](super::panel::LibraryPanel::meter_slot),
//!   which fills it (400 × 40 px) and handles its own right-click;
//! - 10 px, the actions, 5 px apart: play the selection (36 px), add a folder
//!   (31 px), show what plays (30 px) and the view mode (30 px, its icon the
//!   current mode's), each an Iced `mouse_area` icon in the icon colour.

use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, VAlignment};
use teksilo::widgets::{FixedSize, HStack, IconWidget, Padding, Panel, Spacer, TextWidget};

use super::library_vm::LibraryViewModel;
use crate::logic::library_geometry::LibraryViewMode;
use crate::logic::library_groups::LibraryStats;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::{IconAction, SearchField};
use crate::utils::{format_duration, format_size};

/// The bar's height (`library.rs:2017`).
pub const HEIGHT: f32 = 40.0;
/// The bar's padding on each side (`library.rs:1926`).
pub const PADDING: f32 = 15.0;
/// The search field's width (`library.rs:1814`).
pub const SEARCH_WIDTH: f32 = 180.0;
/// The space after the search field (`library.rs:1916`).
pub const SEARCH_GAP: f32 = 15.0;
/// The space around the meter slot (`library.rs:1921`, `:1923`).
pub const METER_GAP: f32 = 10.0;
/// The meter slot's width (`VuMeterWidget`, `widgets.rs:4772`).
pub const METER_SLOT_WIDTH: f32 = 400.0;
/// The figures' text size (`library.rs:1918`).
pub const STATS_SIZE: f32 = 13.0;
/// The space between two actions (`library.rs:1877`).
pub const ACTION_GAP: f32 = 5.0;
/// The play action's size (`library.rs:1875`).
pub const PLAY_SIZE: f32 = 36.0;
/// The add-a-folder action's size (`library.rs:1878`).
pub const ADD_SIZE: f32 = 31.0;
/// The show-playing and view-mode actions' size (`library.rs:1880`, `:1882`).
pub const ICON_SIZE: f32 = 30.0;

/// The view-mode button's icon for `mode` (`library.rs:1864-1869`).
pub fn view_mode_icon(mode: LibraryViewMode) -> IconWidget {
    match mode {
        LibraryViewMode::Grid => icons::library::view_grid_outlined(),
        LibraryViewMode::ThumbnailList => icons::library::view_list_thumbnail_outlined(),
        LibraryViewMode::DetailedList => icons::library::view_list_thumbnail_fill(),
        LibraryViewMode::SimpleList => icons::library::view_list(),
    }
}

/// The figures of `stats` (`library.rs:1844-1862`).
pub fn stats_text(stats: &LibraryStats) -> LocalizedString {
    let count = |n: u64| i64::try_from(n).unwrap_or(i64::MAX);
    tr!(library_stats(
        songs = count(stats.songs),
        songs_text = stats.songs.to_string(),
        albums = count(stats.albums),
        albums_text = stats.albums.to_string(),
        artists = count(stats.artists),
        artists_text = stats.artists.to_string(),
        duration = format_duration(stats.duration_secs),
        size = format_size(stats.size_bytes as i64)
    ))
}

/// A fixed horizontal gap.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

/// The figures, rebuilt when they change.
struct Stats {
    stats: Signal<LibraryStats>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Stats")
            .field("stats", &self.stats.get())
            .finish_non_exhaustive()
    }
}

impl Widget for Stats {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.stats
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let root = ctx.add(
            TextWidget::new(stats_text(&self.stats.get()))
                .style(style::inter(STATS_SIZE, FontWeight::MEDIUM))
                .color(TextRole::Secondary)
                .single_line(),
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
}

/// The view-mode button, rebuilt with the mode's icon.
struct ViewModeButton {
    vm: LibraryViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ViewModeButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ViewModeButton").finish_non_exhaustive()
    }
}

impl Widget for ViewModeButton {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let mode = self.vm.view_mode();
        mode.bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let vm = self.vm.clone();
        let root = ctx.add(
            IconAction::new(
                view_mode_icon(mode.get()),
                ICON_SIZE,
                tr!(library_view_mode()),
            )
            .on_activate(move |_ctx| vm.cycle_view_mode()),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| Size::new(ICON_SIZE, ICON_SIZE))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

/// The library's bottom bar; see the module documentation.
pub struct BottomBar {
    vm: LibraryViewModel,
    meter: Option<Box<dyn Widget>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for BottomBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BottomBar")
            .field("meter", &self.meter.is_some())
            .finish_non_exhaustive()
    }
}

impl BottomBar {
    /// The bar of `vm`, with `meter` in its meter slot (an empty slot when
    /// `None`).
    pub fn new(vm: LibraryViewModel, meter: Option<Box<dyn Widget>>) -> Self {
        Self {
            vm,
            meter,
            root: None,
        }
    }
}

impl Widget for BottomBar {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        let slot = match self.meter.take() {
            Some(meter) => ctx.add_boxed(meter),
            None => ctx.add(Spacer::new()),
        };
        let (play_vm, add_vm, show_vm) = (vm.clone(), vm.clone(), vm.clone());
        let actions = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(
                IconAction::new(
                    icons::library::play_outlined(),
                    PLAY_SIZE,
                    tr!(library_play()),
                )
                .on_activate(move |_ctx| {
                    play_vm.play_selection();
                }),
            )
            .child(gap(ACTION_GAP))
            .child(
                IconAction::new(
                    icons::library::more_small(),
                    ADD_SIZE,
                    tr!(library_add_folder()),
                )
                .on_activate(move |ctx| add_vm.add_folder(ctx)),
            )
            .child(gap(ACTION_GAP))
            .child(
                IconAction::new(
                    icons::library::radio_button_on(),
                    ICON_SIZE,
                    tr!(library_show_playing()),
                )
                .on_activate(move |_ctx| show_vm.show_playing()),
            )
            .child(gap(ACTION_GAP))
            .child(ViewModeButton {
                vm: vm.clone(),
                root: None,
            });
        let row = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(
                SearchField::new(vm.search(), tr!(library_search_placeholder()), SEARCH_WIDTH)
                    .label(tr!(library_search())),
            )
            .child(gap(SEARCH_GAP))
            .child(Stats {
                stats: vm.stats(),
                root: None,
            })
            .child(gap(METER_GAP))
            .child(
                FixedSize::new()
                    .width(METER_SLOT_WIDTH)
                    .height(HEIGHT)
                    .child(slot),
            )
            .child(gap(METER_GAP))
            .child(actions)
            .child(Spacer::new());
        let root = ctx.add(
            FixedSize::new().height(HEIGHT).child(
                Panel::new()
                    .padding(0.0)
                    .border_width(0.0)
                    .background(SurfaceRole::Raised)
                    .child(Padding::new(0.0, PADDING, 0.0, PADDING).child(row)),
            ),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, HEIGHT))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}
