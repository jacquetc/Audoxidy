//! The tab strip above the list: one tab per playlist.
//!
//! Port of `build_tabs_bar` (`src/gui/playlist.rs:398`): a 40 px strip in
//! `COLOR_CONTRAST`. From 12 px in, the tabs, 5 px apart, scroll sideways
//! with no visible bar. A tab is the playlist's name in Inter Medium 14 px
//! with 5 px around it, primary for the active playlist and secondary for the
//! others; a click opens the playlist (`SwitchPlaylist(id, false)`), a
//! right-click opens its menu ([`menus::tab_menu`]). A right-click on the
//! strip's free area opens [`menus::bar_menu`].
//!
//! When the tabs' estimated widths pass 380 px
//! ([`tabs_overflow`]), a chevron (28 px,
//! secondary, 4 px of padding) closes the strip: it opens the list of every
//! playlist ([`menus::overflow_menu`]) under itself, where Iced opens it at
//! a fixed window position (quirk fix 7), and shows the up chevron while the
//! list is open. After a switch, the active tab scrolls into view as Iced
//! scrolls it ([`tab_scroll_offset`]).
//!
//! The strip is rebuilt when the playlists change. A rebuild keeps the
//! strip's horizontal offset (Iced's scrollable keeps its state by id), and a
//! request to bring a tab into view that arrives before the tab exists (a
//! new playlist is switched to before the tabs reload) waits for the
//! rebuild that adds it.

use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::accesskit::Role;
use teksilo::core::binding::BindingLevel;
use teksilo::core::overlay::OverlayPlacement;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, VAlignment};
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, IconWidget, OverlayTrigger, Padding, PopoverCustom,
    RectWidget, ScrollArea, ScrollBarPolicy, TextWidget, ZStack,
};

use super::menus;
use super::playlist_vm::{PlaylistViewModel, TAB_SPACING, tab_scroll_offset, tabs_overflow};
use crate::teksilo_ui::icons;
use crate::teksilo_ui::models::PlaylistTab;
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::Pressable;
use crate::teksilo_ui::widgets::menu::context_menu;

/// The strip's height (`TABS_BAR_HEIGHT`, `src/gui/playlist.rs:394`).
pub const HEIGHT: f32 = 40.0;
/// The space before the first tab (`src/gui/playlist.rs:579`).
pub const LEADING: f32 = 12.0;
/// A tab's padding (`src/gui/playlist.rs:422`).
pub const TAB_PADDING: f32 = 5.0;
/// A tab's height: its 14 px line (Iced's 1.3 line height, 18 px) and the
/// padding.
pub const TAB_HEIGHT: f32 = 28.0;
/// The chevron's size (`src/gui/playlist.rs:621`).
pub const CHEVRON_SIZE: f32 = 28.0;
/// The chevron's padding (`src/gui/playlist.rs:629`).
pub const CHEVRON_PADDING: f32 = 4.0;

/// The tab strip; see the module documentation.
pub struct TabsStrip {
    vm: PlaylistViewModel,
    viewport: Rc<Cell<f32>>,
    offset: Rc<Cell<f32>>,
    pending: Rc<Cell<Option<i64>>>,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for TabsStrip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabsStrip").finish_non_exhaustive()
    }
}

impl TabsStrip {
    /// The strip of `vm`.
    pub fn new(vm: PlaylistViewModel) -> Self {
        Self {
            vm,
            viewport: Rc::new(Cell::new(0.0)),
            offset: Rc::new(Cell::new(0.0)),
            pending: Rc::new(Cell::new(None)),
            root: None,
        }
    }

    /// One tab.
    fn tab(&self, tab: &PlaylistTab) -> impl Widget {
        let id = tab.id;
        let active = self
            .vm
            .active_playlist()
            .map(move |active| *active == Some(id));
        let text = TextWidget::new(lit!(tab.name.clone()))
            .style(style::inter(14.0, FontWeight::MEDIUM))
            .color(active.map(|active| {
                if *active {
                    TextRole::Primary
                } else {
                    TextRole::Secondary
                }
            }))
            .single_line()
            .no_shrink()
            .a11y_hidden();
        let (switch, menu) = (self.vm.clone(), self.vm.clone());
        let menu_tab = tab.clone();
        Pressable::new(
            FixedSize::new()
                .height(TAB_HEIGHT)
                .child(Padding::symmetric(0.0, TAB_PADDING).child(Center::new().child(text))),
            lit!(tab.name.clone()),
        )
        .tab(active)
        .on_activate(move |_| switch.switch_playlist(id, false))
        .context_menu(move |_, _| Some(Box::new(context_menu(menus::tab_menu(&menu, &menu_tab)))))
    }

    /// The overflow chevron and its list.
    fn chevron(&self) -> impl Widget {
        let open = Signal::new(false);
        let closed = open.not();
        let icon = |icon: IconWidget| icon.icon_size(CHEVRON_SIZE).color(TextRole::Secondary);
        let visual = Padding::uniform(CHEVRON_PADDING).child(
            FixedSize::new()
                .width(CHEVRON_SIZE)
                .height(CHEVRON_SIZE)
                .child(
                    ZStack::new()
                        .child(icon(icons::shared::arrow_down_chevron()).visible_when(closed))
                        .child(icon(icons::shared::arrow_up_chevron()).visible_when(open.clone())),
                ),
        );
        let (on_open, on_close) = (open.clone(), open);
        PopoverCustom::new(
            OverlayTrigger::around(visual).named(tr!(playlist_all_lists()).resolve_now()),
        )
        .content(context_menu(menus::overflow_menu(&self.vm)))
        .bare()
        .placement(OverlayPlacement::Below)
        .show_disclosure_caret(false)
        .on_open(move || on_open.set(true))
        .on_close(move || on_close.set(false))
    }
}

impl Widget for TabsStrip {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        // A new playlist, a rename or a deletion rebuilds the strip.
        self.vm.tabs_version().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Rebuild,
        );
        let tabs = self.vm.tab_list();
        let mut row = HStack::new()
            .spacing(TAB_SPACING)
            .alignment(VAlignment::Center);
        for tab in &tabs {
            let id = ctx.add(self.tab(tab));
            // A tab reports whether it is selected: a switch from elsewhere
            // (the overflow list, a new playlist, an import) is no event on
            // the tab, and must still reach the accessibility tree.
            let playlist = tab.id;
            self.vm
                .active_playlist()
                .map(move |active| *active == Some(playlist))
                .bind_to(id, ctx.binding_registry(), BindingLevel::AccessibilityOnly);
            row = row.child(id);
        }
        let scroll = ScrollArea::new()
            .vertical_scroll_bar_policy(ScrollBarPolicy::AlwaysOff)
            .horizontal_scroll_bar_policy(ScrollBarPolicy::AlwaysOff)
            .smooth_scrolling(false)
            .child(
                // The tabs centred on the strip's height.
                FixedSize::new().height(HEIGHT).child(
                    row.access_role(Role::TabList)
                        .access_label(tr!(playlist_tabs())),
                ),
            );
        let scroll_x = scroll.scroll_x_signal().clone();
        // A rebuild starts where the previous strip was.
        scroll_x.set(self.offset.get());
        let offset = self.offset.clone();
        ctx.effect(&scroll_x, move |x| offset.set(*x));

        // After a switch, the active tab comes into view: the tab of the
        // request, among the tabs this build shows. A tab not built yet
        // stays pending until the rebuild that adds it.
        let names: Rc<Vec<(i64, String)>> =
            Rc::new(tabs.iter().map(|tab| (tab.id, tab.name.clone())).collect());
        let bring_into_view = {
            let (pending, viewport, scroll_x) = (
                self.pending.clone(),
                self.viewport.clone(),
                scroll_x.clone(),
            );
            move || {
                let Some(id) = pending.get() else {
                    return;
                };
                let Some(position) = names.iter().position(|(tab, _)| *tab == id) else {
                    return;
                };
                pending.set(None);
                let names: Vec<&str> = names.iter().map(|(_, name)| name.as_str()).collect();
                if let Some(offset) =
                    tab_scroll_offset(&names, position, scroll_x.get(), viewport.get())
                {
                    scroll_x.set(offset);
                }
            }
        };
        bring_into_view();
        let pending = self.pending.clone();
        ctx.effect(&self.vm.tab_scroll_request(), move |request| {
            if let Some((_, id)) = *request {
                pending.set(Some(id));
                bring_into_view();
            }
        });

        let mut bar = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Center)
            .child(Expand::horizontal().child(scroll));
        if tabs_overflow(tabs.iter().map(|tab| tab.name.as_str())) {
            bar = bar.child(self.chevron());
        }
        let menu_vm = self.vm.clone();
        let root = ctx.add(
            FixedSize::new()
                .height(HEIGHT)
                .child(
                    ZStack::new()
                        .child(RectWidget::new().background(SurfaceRole::Raised))
                        .child(Padding::new(0.0, 0.0, 0.0, LEADING).child(bar)),
                )
                .context_menu(move |_, _| Some(Box::new(context_menu(menus::bar_menu(&menu_vm))))),
        );
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        let size = self
            .root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| proposal.resolve(0.0, HEIGHT));
        // The tabs' viewport: the strip less the leading space and the
        // chevron, if any.
        let chevron = if tabs_overflow(self.vm.tab_list().iter().map(|tab| tab.name.as_str())) {
            CHEVRON_SIZE + 2.0 * CHEVRON_PADDING
        } else {
            0.0
        };
        self.viewport.set((size.width - LEADING - chevron).max(0.0));
        size.into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }
}

#[cfg(test)]
mod tests;
