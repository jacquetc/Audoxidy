//! `PlayerPanel`: the 400 x 400 player.
//!
//! Port of `src/gui/player.rs::view`. Six layers, bottom to top, each the size
//! of the panel or of the band it occupies (a `ZStack` centres the 400 x 320
//! band between the 40 px top row and the 40 px seek bar):
//!
//! 1. the cover ([`AlbumArt`], the `Player` placeholder when there is none);
//! 2. a 30 % black overlay;
//! 3. the info layer: the 40 px top row with the menu button, the title
//!    (18 px) and artist (15 px) rows and the elapsed time in the band's last
//!    58 px, and the [`SeekBar`];
//! 4. the three control zones over the band, each a third of the width: Previous,
//!    Play / Pause and Next, with round hover feedback while the user is
//!    active; the mouse wheel over them changes the volume;
//! 5. the volume bubble over the band, while it is up (it takes no input);
//! 6. the time tooltip over the seek bar, while the pointer is on it.
//!
//! The pointer's position in the panel feeds the view-model
//! ([`PlayerViewModel::pointer_moved`]), which derives the zone and the
//! tooltip from it exactly as Iced does; the zones are real buttons on top of
//! that (a click, Enter or Space activate them, they have names, and they are
//! disabled until the session restore settles: quirk fix 6).
//!
//! On macOS the OS draws the window buttons over the player's top-leading
//! corner; [`PlayerPanel::leading_inset`] moves the top row's content out of
//! their way.
//!
//! # Differences from the Iced view
//!
//! - A zone acts on release, as every Teksilo button does; Iced acts on press
//!   (`player.rs:372`, `mouse_area(..).on_press`).
//! - The controls stay active 1500 ms after the last activity; Iced counts
//!   three 500 ms ticks (`app.rs:1134`).
//! - A wheel turn of zero (a sideways wheel, the end of a touchpad gesture) is
//!   not a volume step; in Iced it lowers the volume (`app.rs:4609`).
//! - Text that does not fit ends in an ellipsis; Iced cuts it at the cell's
//!   edge (`player.rs:144-150`).
//! - The title and artist tooltips are attached as Iced attaches them
//!   (`player.rs:156`, `185`), with the same layer order: the control zones lie
//!   over the text and take the pointer, so the tooltips do not show in
//!   Teksilo, as in Iced (`iced_widget::stack` hides the cursor from layers
//!   below a layer with an interaction).
//! - The icons over the cover are `Color::WHITE`: Iced draws the SVGs' own
//!   white (`player.rs:125`, `294`), not the secondary icon role.
//! - The menu opens at the button's left edge; Iced's is 15 px from the
//!   player's left (`player.rs:513`), 7 px further right. The popover API
//!   places the panel relative to its anchor and offers no offset.

use std::time::Duration;

use teksilo::core::binding::BindingLevel;
use teksilo::core::frame_tick_scheduler::FrameTickSubscription;
use teksilo::core::widget_builder::HandlerSet;
use teksilo::core::{ScrollDelta, WidgetEvent};
use teksilo::prelude::*;
use teksilo::tokens::{Alignment, FontWeight, HAlignment, VAlignment};
use teksilo::widgets::tooltip::TooltipWidget;
use teksilo::widgets::{
    Center, Expand, FixedSize, HStack, IconWidget, Padding, RectWidget, Spacer, Switcher,
    TextWidget, VStack, ZStack,
};

use super::geometry::{
    self, ARTIST_ROW_HEIGHT, ARTIST_SIZE, BAND_HEIGHT, BLACKOUT_ALPHA, CONTRAST_ALPHA,
    ELAPSED_SIZE, HoverZone, INFO_PADDING, SEEK_TIP_PADDING, SEEK_TIP_SIZE, SIZE, TEXT_PADDING,
    TITLE_HEIGHT, TITLE_SIZE, TITLE_TOP, TOP_ROW_HEIGHT, TOP_ROW_PADDING, VOLUME_ICON, VOLUME_TEXT,
    VolumeIcon, ZONE_ICON, ZONE_PAUSE_ICON,
};
use super::menu;
use super::player_vm::PlayerViewModel;
use super::seek_bar::SeekBar;
use super::title_marquee;
use crate::frontend::FrontendError;
use crate::teksilo_ui::icons;
use crate::teksilo_ui::style::{self, inter};
use crate::teksilo_ui::widgets::album_art::Placeholder;
use crate::teksilo_ui::widgets::dialog_frame::Frame;
use crate::teksilo_ui::widgets::{AlbumArt, Pressable};

/// The player panel; see the module documentation.
pub struct PlayerPanel {
    vm: PlayerViewModel,
    leading_inset: f32,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for PlayerPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerPanel")
            .field("leading_inset", &self.leading_inset)
            .finish_non_exhaustive()
    }
}

impl PlayerPanel {
    /// The panel over `vm`.
    pub fn new(vm: PlayerViewModel) -> Self {
        Self {
            vm,
            leading_inset: 0.0,
            root: None,
        }
    }

    /// Starts the top row after `inset` px (default 0): the room the OS's own
    /// window buttons take at the window's top-leading corner on macOS
    /// (`host.reserved_leading_inset().width`).
    pub fn leading_inset(mut self, inset: f32) -> Self {
        self.leading_inset = inset.max(0.0);
        self
    }
}

// ---------------------------------------------------------------------------
// Small building blocks
// ---------------------------------------------------------------------------

/// A fixed horizontal gap.
fn gap(width: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(0.0)
        .child(Spacer::new())
}

/// A fixed vertical gap.
fn drop_down(height: f32) -> FixedSize {
    FixedSize::new()
        .width(0.0)
        .height(height)
        .child(Spacer::new())
}

/// A row as Iced lays one out by default: no spacing, children at the top.
fn row() -> HStack {
    HStack::new().spacing(0.0).alignment(VAlignment::Center)
}

/// A column as Iced lays one out by default.
fn column() -> VStack {
    VStack::new().spacing(0.0).alignment(HAlignment::Leading)
}

/// `COLOR_CONTRAST` at 50 % (`player.rs:230`): a literal alpha over the
/// palette's colour, since no theme role carries an alpha.
fn contrast_half() -> Color {
    style::CONTRAST.with_alpha(CONTRAST_ALPHA)
}

/// A white Inter label (`Color::WHITE`: the Iced text sits on the darkened
/// cover whatever the theme).
fn white_text(size: f32, weight: FontWeight) -> TextWidget {
    TextWidget::new(lit!(""))
        .style(inter(size, weight))
        .color(Color::WHITE)
}

/// Reports a track that could not be loaded: the log line and an error toast
/// (quirk fix 8).
fn report(ctx: &mut EventContext, error: &FrontendError) {
    tracing::error!("the track could not be changed: {error}");
    ctx.show_toast(Toast::error(tr!(player_track_failed(
        error = error.to_string()
    ))));
}

// ---------------------------------------------------------------------------
// Title and artist text
// ---------------------------------------------------------------------------

/// A line of the info column: white Inter text in a cell with the tooltip
/// Iced attaches when it is long.
struct InfoText {
    text: Signal<String>,
    tooltip: Signal<Option<String>>,
    size: f32,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for InfoText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfoText")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

impl InfoText {
    fn new(text: Signal<String>, tooltip: Signal<Option<String>>, size: f32) -> Self {
        Self {
            text,
            tooltip,
            size,
            root: None,
        }
    }
}

impl Widget for InfoText {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.tooltip
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        let text = white_text(self.size, FontWeight::REGULAR)
            .text(self.text.clone())
            .single_line();
        let root = ctx.add(text);
        if let Some(tip) = self.tooltip.get() {
            // Iced shows it at once, following the cursor (`player.rs:157`).
            let tooltip = ctx.add(TooltipWidget::new(lit!(tip)));
            ctx.attach_tooltip(root, tooltip, Duration::ZERO);
        }
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

/// The title row: 22 px high, text 2 px in, the row 10 px in (`player.rs:196`).
fn title_row(vm: &PlayerViewModel) -> impl Widget + 'static {
    FixedSize::new().width(SIZE).height(TITLE_HEIGHT).child(
        Padding::symmetric(0.0, INFO_PADDING).child(row().child(
            Padding::symmetric(0.0, TEXT_PADDING).child(InfoText::new(
                vm.display_title(),
                vm.title_tooltip(),
                TITLE_SIZE,
            )),
        )),
    )
}

/// The artist row: 36 px high; the artist cell and a spacer share the width
/// left by the time, equally, as Iced's two `Fill` children do
/// (`player.rs:203-216`).
fn artist_row(vm: &PlayerViewModel) -> impl Widget + 'static {
    FixedSize::new()
        .width(SIZE)
        .height(ARTIST_ROW_HEIGHT)
        .child(
            Padding::symmetric(0.0, INFO_PADDING).child(
                row()
                    .child(Expand::horizontal().child(row().child(
                        Padding::symmetric(0.0, TEXT_PADDING).child(InfoText::new(
                            vm.display_artist(),
                            vm.artist_tooltip(),
                            ARTIST_SIZE,
                        )),
                    )))
                    .child(Expand::horizontal().child(Spacer::new()))
                    .child(
                        Padding::symmetric(0.0, TEXT_PADDING).child(
                            white_text(ELAPSED_SIZE, FontWeight::REGULAR).text(vm.elapsed()),
                        ),
                    ),
            ),
        )
}

/// The band of the info layer: the title and artist, shown while the track
/// plays or has moved (`player.rs:117`, `192-221`).
fn info_band(vm: &PlayerViewModel) -> impl Widget + 'static {
    FixedSize::new().width(SIZE).height(BAND_HEIGHT).child(
        column()
            .child(drop_down(TITLE_TOP))
            .child(title_row(vm))
            .child(artist_row(vm))
            .visible_when(vm.info_visible()),
    )
}

// ---------------------------------------------------------------------------
// The info layer's top row
// ---------------------------------------------------------------------------

fn top_row(vm: &PlayerViewModel, inset: f32) -> impl Widget + 'static {
    FixedSize::new().width(SIZE).height(TOP_ROW_HEIGHT).child(
        row()
            .child(gap(TOP_ROW_PADDING + inset))
            .child(menu::button(vm))
            .child(Spacer::new()),
    )
}

fn info_layer(vm: &PlayerViewModel, inset: f32) -> impl Widget + 'static {
    FixedSize::new().width(SIZE).height(SIZE).child(
        column()
            .child(top_row(vm, inset))
            .child(info_band(vm))
            .child(SeekBar::new(vm.clone())),
    )
}

// ---------------------------------------------------------------------------
// The control zones
// ---------------------------------------------------------------------------

/// The round feedback of a zone: `icon` at `size` px in a 48 px bubble
/// (`player.rs:299-322`), shown while `zone` is the hover zone.
fn bubble(vm: &PlayerViewModel, zone: HoverZone, icon: IconWidget, size: f32) -> impl Widget {
    let pad = (geometry::ZONE_BUBBLE - size) / 2.0;
    Frame::new(
        FixedSize::new()
            .width(size)
            .height(size)
            .child(icon.icon_size(size).color(Color::WHITE)),
    )
    .fill(contrast_half())
    .radius(30.0)
    .padding(pad, pad, pad, pad)
    .visible_when(vm.hover_zone().map(move |current| *current == zone))
    .hit_transparent(true)
}

/// One third of the band: a button over the whole third whose centre shows
/// `feedback` while the pointer is in it.
fn zone(
    label: LocalizedString,
    feedback: impl Widget + 'static,
    enabled: Signal<bool>,
    on_activate: impl Fn(&mut EventContext) + 'static,
) -> impl Widget + 'static {
    Expand::new().child(
        Pressable::new(
            ZStack::new()
                .child(RectWidget::new())
                .child(Center::new().child(feedback)),
            label,
        )
        .enabled(enabled)
        .on_activate(on_activate),
    )
}

/// The Previous, Play / Pause and Next zones, with the volume wheel over them
/// (`VolumeScrollArea`, `widgets.rs:1005`).
fn zones_layer(vm: &PlayerViewModel) -> impl Widget + 'static {
    let enabled = vm.transport_enabled();
    let previous = {
        let vm = vm.clone();
        zone(
            tr!(player_previous()),
            bubble(
                &vm,
                HoverZone::Previous,
                icons::player::skip_previous_fill(),
                ZONE_ICON,
            ),
            enabled.clone(),
            move |ctx| {
                if let Some(Err(error)) = vm.previous() {
                    report(ctx, &error);
                }
            },
        )
    };
    let play_pause = {
        let vm = vm.clone();
        let index = vm.is_playing().map(|playing| usize::from(*playing));
        let feedback = Switcher::new(index)
            .child(bubble(
                &vm,
                HoverZone::PlayPause,
                icons::player::play_fill(),
                ZONE_ICON,
            ))
            .child(bubble(
                &vm,
                HoverZone::PlayPause,
                icons::player::pause_fill(),
                ZONE_PAUSE_ICON,
            ));
        zone(
            tr!(player_play_pause()),
            feedback,
            enabled.clone(),
            move |_| {
                vm.play_pause();
            },
        )
    };
    let next = {
        let vm = vm.clone();
        zone(
            tr!(player_next()),
            bubble(
                &vm,
                HoverZone::Next,
                icons::player::skip_next_fill(),
                ZONE_ICON,
            ),
            enabled,
            move |ctx| {
                if let Some(Err(error)) = vm.next() {
                    report(ctx, &error);
                }
            },
        )
    };
    let wheel = vm.clone();
    FixedSize::new().width(SIZE).height(BAND_HEIGHT).child(
        row()
            .child(previous)
            .child(play_pause)
            .child(next)
            .on_scroll(move |event, _ctx| {
                let WidgetEvent::Scroll { delta, .. } = event else {
                    return EventResponse::Ignored;
                };
                // Teksilo's delta is a scroll offset: positive is the wheel
                // turned down. Iced's is the opposite, and a zero turn
                // (a sideways wheel, the end of a touchpad gesture) is not a
                // step.
                let turned = match delta {
                    ScrollDelta::Lines { y, .. } | ScrollDelta::Pixels { y, .. } => -*y,
                };
                if turned != 0.0 {
                    wheel.scroll_volume(turned);
                }
                EventResponse::Handled
            }),
    )
}

// ---------------------------------------------------------------------------
// The volume bubble and the seek tooltip
// ---------------------------------------------------------------------------

/// The volume bubble over the band: the speaker icon for the percentage, and
/// the percentage (`player.rs:405-440`). Shown while the view-model has a
/// volume to show; it takes no input.
fn volume_layer(vm: &PlayerViewModel) -> impl Widget + 'static {
    let volume = vm.volume();
    let icon_index = volume.map(
        |percent| match VolumeIcon::for_percent(percent.unwrap_or(0.0)) {
            VolumeIcon::Off => 0,
            VolumeIcon::Down => 1,
            VolumeIcon::Up => 2,
        },
    );
    let icon = |icon: IconWidget| {
        FixedSize::new()
            .width(VOLUME_ICON)
            .height(VOLUME_ICON)
            .child(icon.icon_size(VOLUME_ICON).color(Color::WHITE))
    };
    let icons = Switcher::new(icon_index)
        .child(icon(icons::player::volume_off_fill()))
        .child(icon(icons::player::volume_down_fill()))
        .child(icon(icons::player::volume_up_fill()));
    let text = white_text(VOLUME_TEXT, FontWeight::MEDIUM)
        .text(volume.map(|percent| geometry::volume_text(percent.unwrap_or(0.0))));
    let bubble = Frame::new(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Center)
            .child(icons)
            .child(text),
    )
    .fill(contrast_half())
    .radius(50.0)
    .padding(8.0, 22.0, 13.0, 22.0);
    FixedSize::new()
        .width(SIZE)
        .height(BAND_HEIGHT)
        .child(Center::new().child(bubble))
        .visible_when(volume.map(Option::is_some))
        .hit_transparent(true)
}

/// The time tooltip: a small box above the pointer (`player.rs:612-644`).
fn seek_tooltip_layer(vm: &PlayerViewModel) -> impl Widget + 'static {
    let hover = vm.seek_hover();
    let top = hover.map(|hover| hover.as_ref().map_or(0.0, |hover| hover.top));
    let left = hover.map(|hover| hover.as_ref().map_or(0.0, |hover| hover.left));
    let text = hover.map(|hover| {
        hover
            .as_ref()
            .map(|hover| hover.text.clone())
            .unwrap_or_default()
    });
    let tip = Frame::new(
        white_text(SEEK_TIP_SIZE, FontWeight::REGULAR)
            .text(text)
            .a11y_hidden(),
    )
    .fill(SurfaceRole::Raised)
    .border(TextRole::Secondary, 1.0)
    .padding(
        SEEK_TIP_PADDING,
        SEEK_TIP_PADDING,
        SEEK_TIP_PADDING,
        SEEK_TIP_PADDING,
    );
    FixedSize::new()
        .width(SIZE)
        .height(SIZE)
        .child(
            ZStack::new()
                .alignment(Alignment::TOP_LEADING)
                .child(Padding::new(top, 0.0, 0.0, left).child(tip)),
        )
        .visible_when(hover.map(Option::is_some))
        .hit_transparent(true)
}

// ---------------------------------------------------------------------------
// The marquee's frame subscription
// ---------------------------------------------------------------------------

/// A widget with no size that holds the marquee's frame subscription while
/// the marquee runs: it rebuilds when [`PlayerViewModel::marquee_running`]
/// changes, so the guard exists only while a text scrolls and the track plays.
struct MarqueeDriver {
    running: Signal<bool>,
    guard: Option<FrameTickSubscription>,
}

impl std::fmt::Debug for MarqueeDriver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarqueeDriver")
            .field("subscribed", &self.guard.is_some())
            .finish()
    }
}

impl Widget for MarqueeDriver {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.running
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        // The old guard drops here, before the new one is made.
        self.guard = None;
        if self.running.get() {
            self.guard = Some(ctx.subscribe_frame_tick_throttled(title_marquee::PERIOD));
        }
        Vec::new()
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }
}

// ---------------------------------------------------------------------------
// The panel
// ---------------------------------------------------------------------------

impl Widget for PlayerPanel {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        let vm = self.vm.clone();
        vm.wire(ctx);

        // The pointer's position in the panel, wherever over it (a preview
        // handler sees the moves over every descendant); leaving it clears.
        let inside = ctx.signal(false);
        let leaving = vm.clone();
        ctx.effect(&inside, move |inside| {
            if !*inside {
                leaving.pointer_moved(None);
            }
        });
        let moving = vm.clone();
        ctx.apply_self_handlers(HandlerSet::new().hover_within(inside).on_pointer_event(
            move |event, _ctx| {
                if let WidgetEvent::PointerMove { position, .. } = event {
                    moving.pointer_moved(Some((position.x, position.y)));
                }
                EventResponse::Ignored
            },
        ));

        let blackout = RectWidget::new().background(Color::new(0.0, 0.0, 0.0, BLACKOUT_ALPHA));
        let stack = ZStack::new()
            .child(AlbumArt::new(vm.cover(), Placeholder::Player, SIZE))
            .child(blackout)
            .child(info_layer(&vm, self.leading_inset))
            .child(zones_layer(&vm))
            .child(volume_layer(&vm))
            .child(seek_tooltip_layer(&vm))
            // Sized zero, it would still take presses re-attributed from near
            // its centre.
            .child(
                MarqueeDriver {
                    running: vm.marquee_running(),
                    guard: None,
                }
                .hit_transparent(true),
            );
        let root = ctx.add(FixedSize::new().width(SIZE).height(SIZE).child(stack));
        self.root = Some(root);
        vec![root]
    }

    fn layout_response(&self, proposal: SizeProposal, ctx: &LayoutContext) -> LayoutResponse {
        self.root
            .and_then(|id| ctx.child_size(id, proposal))
            .unwrap_or_else(|| Size::new(SIZE, SIZE))
            .into()
    }

    fn children(&self) -> Vec<WidgetId> {
        self.root.into_iter().collect()
    }

    fn accessibility(&self, builder: &mut AccessNodeBuilder) {
        builder.set_role(teksilo::core::accesskit::Role::Group);
        builder.set_name(tr!(player_name()).resolve_now());
    }
}

#[cfg(test)]
mod tests;
