//! The bar under the list: the search field and four icons.
//!
//! Port of `build_bottom_bar` (`src/gui/playlist.rs:1144`): a 40 px bar in
//! `COLOR_CONTRAST` with 15 px on each side. At the leading end the search
//! field ("Buscar...", 180 px, `widgets::SearchField`; Escape clears it,
//! quirk fix 35), which filters the rows. At the trailing end, 10 px apart,
//! four 24 px icons with 2 px around them (`icon_button`,
//! `src/gui/playlist.rs:1202`), in the accent while their mode is on and in
//! the secondary colour otherwise:
//!
//! - the lyrics (3 px lower): an Iced stub (`ToggleLyrics` does nothing,
//!   `src/gui/app.rs:2772`), shown disabled;
//! - repeat: Off → All → One; the "repeat one" icon in mode One, lit in All
//!   and One;
//! - shuffle, lit while on;
//! - the equalizer, which opens the Audio Center on its equalizer tab
//!   (`ToggleAudioCenter(Some(1))`).

use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::tokens::VAlignment;
use teksilo::widgets::{FixedSize, HStack, Padding, RectWidget, Spacer, ZStack};

use super::playlist_vm::PlaylistViewModel;
use crate::logic::playlist_nav::RepeatMode;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::widgets::{IconAction, SearchField};

/// The bar's height (`BOTTOM_BAR_HEIGHT`, `src/gui/playlist.rs:395`).
pub const HEIGHT: f32 = 40.0;
/// The space at each end (`src/gui/playlist.rs:1190`).
pub const SIDE_PADDING: f32 = 15.0;
/// The space between the icons, and between the field and the rest
/// (`src/gui/playlist.rs:1184`, `:1191`).
pub const SPACING: f32 = 10.0;
/// The search field's width (`src/gui/playlist.rs:1151`).
pub const SEARCH_WIDTH: f32 = 180.0;
/// An icon's size (`src/gui/playlist.rs:1211`).
pub const ICON_SIZE: f32 = 24.0;
/// The padding around an icon (`src/gui/playlist.rs:1218`).
pub const ICON_PADDING: f32 = 2.0;
/// How far the lyrics icon sits below the others (`src/gui/playlist.rs:1176`).
pub const LYRICS_DROP: f32 = 3.0;

/// An icon's colour: the accent while its mode is on (`icon_button`,
/// `src/gui/playlist.rs:1203`).
pub fn icon_role(active: bool) -> TextRole {
    if active {
        TextRole::Accent
    } else {
        TextRole::Secondary
    }
}

/// The repeat button: rebuilt when the mode changes, since its icon and its
/// name change with it.
struct RepeatButton {
    vm: PlaylistViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for RepeatButton {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RepeatButton").finish_non_exhaustive()
    }
}

impl Widget for RepeatButton {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let repeat = self.vm.repeat();
        repeat.bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let (icon, label) = match repeat.get() {
            RepeatMode::Off => (
                icons::playlist::repeat_outlined(),
                tr!(playlist_repeat_off()),
            ),
            RepeatMode::All => (
                icons::playlist::repeat_outlined(),
                tr!(playlist_repeat_all()),
            ),
            RepeatMode::One => (
                icons::playlist::repeat_one_outlined(),
                tr!(playlist_repeat_one()),
            ),
        };
        let vm = self.vm.clone();
        let root = ctx.add(
            IconAction::new(icon, ICON_SIZE, label)
                .padding(ICON_PADDING)
                .color(icon_role(repeat.get() != RepeatMode::Off))
                .on_activate(move |_| vm.toggle_repeat()),
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

/// The bottom bar; see the module documentation.
pub struct BottomBar {
    vm: PlaylistViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for BottomBar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BottomBar").finish_non_exhaustive()
    }
}

impl BottomBar {
    /// The bar of `vm`.
    pub fn new(vm: PlaylistViewModel) -> Self {
        Self { vm, root: None }
    }
}

impl Widget for BottomBar {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = &self.vm;
        // TODO(iced-stub): the lyrics button sends `ToggleLyrics`, which does nothing in the Iced UI (src/gui/app.rs:2772).
        let lyrics = IconAction::new(
            icons::playlist::lyrics_outlined(),
            ICON_SIZE,
            tr!(playlist_lyrics()),
        )
        .padding(ICON_PADDING)
        .color(icon_role(false))
        .enabled(false);
        let shuffle_vm = vm.clone();
        let shuffle = IconAction::new(
            icons::playlist::shuffle(),
            ICON_SIZE,
            tr!(playlist_shuffle()),
        )
        .padding(ICON_PADDING)
        .color(vm.shuffle().map(|on| icon_role(*on)))
        .on_activate(move |_| shuffle_vm.toggle_shuffle());
        let eq_vm = vm.clone();
        let equalizer = IconAction::new(
            icons::shared::equalizer(),
            ICON_SIZE,
            tr!(playlist_equalizer()),
        )
        .padding(ICON_PADDING)
        .color(icon_role(false))
        .on_activate(move |_| eq_vm.toggle_equalizer());
        let icons_group = HStack::new()
            .spacing(SPACING)
            .alignment(VAlignment::Center)
            .child(Padding::new(LYRICS_DROP, 0.0, 0.0, 0.0).child(lyrics))
            .child(RepeatButton {
                vm: vm.clone(),
                root: None,
            })
            .child(shuffle)
            .child(equalizer);
        let row = HStack::new()
            .spacing(SPACING)
            .alignment(VAlignment::Center)
            .child(SearchField::new(
                vm.search(),
                tr!(playlist_search()),
                SEARCH_WIDTH,
            ))
            .child(Spacer::new())
            .child(icons_group);
        let root = ctx.add(
            FixedSize::new().height(HEIGHT).child(
                ZStack::new()
                    .child(RectWidget::new().background(SurfaceRole::Raised))
                    .child(Padding::symmetric(0.0, SIDE_PADDING).child(row)),
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

#[cfg(test)]
mod tests;
