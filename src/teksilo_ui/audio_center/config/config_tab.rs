//! [`ConfigTab`]: the Audio Center's "Configuración de Audio" tab.
//!
//! Port of `view_audio_config` (`src/gui/audio_center.rs:2033-2779`). The tab
//! is a column of the two halves and the bottom row; its area is the 906 px
//! wide body of the panel:
//!
//! - the **selections** (left, 5 of 9 parts of what is left of the 80 px
//!   divider column, 16 px from the top, `:2260-2340`): a row per setting,
//!   Inter Medium 14 px, primary, in a 180 px label cell and the combo box 10 px
//!   after it: the audio core (125 px), the output device (260 px), the sample
//!   rate, the bit depth, the output channels and the buffer ("Quantum
//!   (Buffer)", 125 px each), then, 4 px lower, the sound server's rate and
//!   quantum side by side under the two-line label "Pipewire | PulseAudio" /
//!   "Frecuencia | Quantum". Rows are 16 px apart. The combo boxes are the
//!   Iced pick lists in the theme's look (square, `COLOR_CONTRAST`, accent
//!   while hovered or open);
//! - the **divider**: a 2 × 220 `COLOR_CONTRAST` line in an 80 px cell, 8 px
//!   below the top and centred in the height (`:2681`);
//! - the **information column** (right, 4 of 9 parts, 18 px from the top, 5 px
//!   from the divider, 1 px from the edge, rows 8 px apart, `:2440-2620`): the
//!   title "Información de Audio" (16 px, centred), the song (80 px label cell,
//!   the title or "Sin reproducción"), the format, a table with the frequency,
//!   depth and channels of the file ("Entrada") and of the output ("Salida")
//!   either side of a 2 × 90 line, the gain ("Album | Canción", 170 px label
//!   cell), Audoxidy's quantum and the sound server's status with its latency
//!   under it in secondary 13 px;
//! - the **notice** (`:2690`): the engine's non-modal message in the accent at
//!   13 px, when there is one;
//! - the **buttons** (`:2620-2680`): "Reiniciar Servicio de Audio",
//!   "Predeterminado" and "Aplicar", 10 px apart, 15 px text with 8 × 12 px of
//!   padding. Aplicar is dimmed and inert until a selection changes, and while
//!   it runs.
//!
//! The tab polls the sound server every 3 s while it is on screen: a
//! `Poller` inside it holds a throttled frame-tick subscription only while
//! the tab is shown, and is rebuilt when that changes.

use std::time::Instant;

use teksilo::core::binding::BindingLevel;
use teksilo::prelude::*;
use teksilo::tokens::{FontWeight, HAlignment, VAlignment};
use teksilo::widgets::{
    ComboBox, Expand, FixedSize, HStack, Padding, RectWidget, Spacer, TextWidget, VStack, ZStack,
};

use super::config_vm::{ConfigViewModel, InfoRows, POLL_INTERVAL};
use crate::audio::engine::BitDepth;
use crate::logic::audio_settings::{self as rules, SystemSelection};
use crate::teksilo_ui::style;
use crate::teksilo_ui::widgets::dialog_frame::text_button;
use crate::teksilo_ui::widgets::stock_styles::combo_box;

/// A setting's label cell (`:2260`).
pub const LABEL_WIDTH: f32 = 180.0;
/// The width of the narrow combo boxes (`:2142`).
pub const COMBO_WIDTH: f32 = 125.0;
/// The width of the device combo box (`:2150`).
pub const DEVICE_WIDTH: f32 = 260.0;
/// The space between a label and its control, and between the two sound
/// server combo boxes (`:2264`).
pub const ROW_SPACING: f32 = 10.0;
/// The space between settings (`:2340`).
pub const SETTINGS_SPACING: f32 = 16.0;
/// The space above the sound server row (`:2330`).
pub const SYSTEM_TOP: f32 = 4.0;
/// The space above the selections (`:2692`).
pub const LEFT_TOP: f32 = 16.0;
/// The divider cell's width (`:2704`) ...
pub const DIVIDER_CELL: f32 = 80.0;
/// ... the line's height (`:2682`) ...
pub const DIVIDER_HEIGHT: f32 = 220.0;
/// ... and the line's width.
pub const LINE_WIDTH: f32 = 2.0;
/// The space above the divider (`:2708`).
pub const DIVIDER_TOP: f32 = 8.0;
/// The information column's padding: top, leading, trailing (`:2712`).
pub const INFO_PADDING: (f32, f32, f32) = (18.0, 5.0, 1.0);
/// The space between information rows (`:2650`).
pub const INFO_SPACING: f32 = 8.0;
/// The song and format label cell (`:2539`).
pub const INFO_LABEL_WIDTH: f32 = 80.0;
/// The gain, quantum and server label cell (`:2579`).
pub const INFO_WIDE_LABEL_WIDTH: f32 = 170.0;
/// The table's label column (`:2475`).
pub const TABLE_LABELS: f32 = 90.0;
/// The table's divider cell (`:2478`) and line height (`:2468`).
pub const TABLE_DIVIDER_CELL: f32 = 20.0;
/// The line between the file's and the output's columns.
pub const TABLE_LINE_HEIGHT: f32 = 90.0;
/// The space between the table's rows (`:2457`).
pub const TABLE_SPACING: f32 = 8.0;
/// The space between a label and the small line under it (`:2339`).
pub const LABEL_LINES_SPACING: f32 = 2.0;
/// The buttons' text size (`:2619`) and padding (vertical, horizontal).
pub const BUTTON_TEXT: f32 = 15.0;
/// The buttons' padding.
pub const BUTTON_PADDING: (f32, f32) = (8.0, 12.0);

/// Inter Medium at `size`, in `role`: the tab's only text style.
fn text(label: impl Into<LocalizedString>, size: f32, role: TextRole) -> TextWidget {
    TextWidget::new(label)
        .style(style::inter(size, FontWeight::MEDIUM))
        .color(role)
        .single_line()
        .no_shrink()
}

/// A value shown as text, following `value`.
fn dynamic(value: Signal<String>, size: f32, role: TextRole) -> TextWidget {
    TextWidget::new(lit!(""))
        .text(value)
        .style(style::inter(size, FontWeight::MEDIUM))
        .color(role)
        .single_line()
        .no_shrink()
}

/// A column as Iced lays one out by default: no spacing, children at the
/// leading edge.
fn column() -> VStack {
    VStack::new().spacing(0.0).alignment(HAlignment::Leading)
}

/// A row as Iced lays one out by default, centred vertically.
fn row() -> HStack {
    HStack::new().spacing(0.0).alignment(VAlignment::Center)
}

/// A label cell of `width`.
fn cell(width: f32, content: impl Widget + 'static) -> FixedSize {
    FixedSize::new().width(width).child(row().child(content))
}

// ---------------------------------------------------------------------------
// The combo boxes
// ---------------------------------------------------------------------------

/// A pick list over typed items in the Iced look.
fn pick<T: Clone + PartialEq + 'static>(
    vm: &ConfigViewModel,
    items: Vec<T>,
    selected: Signal<Option<T>>,
    label: impl Fn(&T) -> LocalizedString + 'static,
    name: LocalizedString,
    width: f32,
) -> FixedSize {
    let vm = vm.clone();
    FixedSize::new().width(width).child(combo_box(
        ComboBox::from_items(items, selected, label)
            .label(name)
            .on_select(move |_, _| vm.selection_changed()),
    ))
}

/// A pick list over names the engine lists (hosts, devices). It is rebuilt
/// when the lists change. A selection that is not in the list (the saved
/// device is unplugged) stays selected, as in Iced, where Apply then makes the
/// engine fall back and say so in the notice.
struct NamedPick {
    vm: ConfigViewModel,
    selected: Signal<Option<String>>,
    names: fn(&ConfigViewModel) -> Vec<String>,
    name: LocalizedString,
    width: f32,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for NamedPick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NamedPick")
            .field("width", &self.width)
            .finish_non_exhaustive()
    }
}

impl Widget for NamedPick {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.lists_version().bind_to(
            ctx.self_id(),
            ctx.binding_registry(),
            BindingLevel::Rebuild,
        );
        let vm = self.vm.clone();
        let combo = ComboBox::new((self.names)(&self.vm), self.selected.clone())
            .label(self.name.clone())
            .on_select(move |_, _| vm.selection_changed());
        let root = ctx.add(FixedSize::new().width(self.width).child(combo_box(combo)));
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

/// The label of a sound-server selection: "Default", "Automático", or the
/// value with `unit` (`:2216`).
fn selection_label(selection: &SystemSelection<u32>, unit: &'static str) -> LocalizedString {
    match selection {
        SystemSelection::Default => tr!(ac_config_system_default()),
        SystemSelection::Automatic => tr!(ac_config_automatic()),
        SystemSelection::Fixed(value) => lit!(format!("{value}{unit}")),
    }
}

/// The sound server's choices: default, automatic, then the values.
fn selections(values: &[u32]) -> Vec<SystemSelection<u32>> {
    [SystemSelection::Default, SystemSelection::Automatic]
        .into_iter()
        .chain(values.iter().map(|&value| SystemSelection::Fixed(value)))
        .collect()
}

/// The channel count's label in a pick list.
fn output_channels_label(channels: u16) -> LocalizedString {
    rules::OUTPUT_CHANNEL_OPTIONS
        .iter()
        .find(|(_, count)| *count == channels)
        .map_or_else(|| lit!(channels.to_string()), |(label, _)| lit!(*label))
}

/// The label of a channel count in the information column: Iced's labels, and
/// "N canales" for other counts, as a translation.
pub fn channels_label(channels: Option<u16>) -> LocalizedString {
    match channels {
        None => lit!("--"),
        Some(1 | 2 | 3 | 4 | 6 | 8) => {
            lit!(rules::channel_count_label(channels.unwrap_or_default()))
        }
        Some(count) => tr!(ac_config_channels_other(count = i64::from(count))),
    }
}

// ---------------------------------------------------------------------------
// The poller
// ---------------------------------------------------------------------------

/// Polls the sound server while the tab is on screen. It occupies no space.
/// It holds the frame-tick subscription only while `shown`, and is rebuilt
/// when that changes; its tick effect reads the clock (it also fires on frames
/// others request).
struct Poller {
    vm: ConfigViewModel,
    shown: Signal<bool>,
    tick: Option<teksilo::core::frame_tick_scheduler::FrameTickSubscription>,
}

impl std::fmt::Debug for Poller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Poller")
            .field("polling", &self.tick.is_some())
            .finish_non_exhaustive()
    }
}

impl Widget for Poller {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.shown
            .bind_to(ctx.self_id(), ctx.binding_registry(), BindingLevel::Rebuild);
        self.tick = None;
        if self.shown.get() {
            self.vm.on_shown();
            self.tick = Some(ctx.subscribe_frame_tick_throttled(POLL_INTERVAL));
            let vm = self.vm.clone();
            ctx.effect(&ctx.frame_tick(), move |_| vm.poll(Instant::now()));
        }
        Vec::new()
    }

    fn layout_response(&self, _proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        Size::new(0.0, 0.0).into()
    }
}

// ---------------------------------------------------------------------------
// The tab
// ---------------------------------------------------------------------------

/// The settings column.
fn settings(vm: &ConfigViewModel) -> impl Widget + use<> {
    let setting = |label: LocalizedString, control: Box<dyn Widget>| {
        row()
            .spacing(ROW_SPACING)
            .child(cell(LABEL_WIDTH, text(label, 14.0, TextRole::Primary)))
            .child(control)
    };
    let host: Box<dyn Widget> = Box::new(NamedPick {
        vm: vm.clone(),
        selected: vm.host(),
        names: ConfigViewModel::host_names,
        name: tr!(ac_config_host()),
        width: COMBO_WIDTH,
        root: None,
    });
    let device: Box<dyn Widget> = Box::new(NamedPick {
        vm: vm.clone(),
        selected: vm.device(),
        names: ConfigViewModel::device_names,
        name: tr!(ac_config_device()),
        width: DEVICE_WIDTH,
        root: None,
    });
    let rate: Box<dyn Widget> = Box::new(pick(
        vm,
        rules::SAMPLE_RATE_OPTIONS.to_vec(),
        vm.sample_rate(),
        |rate| lit!(format!("{rate} Hz")),
        tr!(ac_config_sample_rate()),
        COMBO_WIDTH,
    ));
    let depth: Box<dyn Widget> = Box::new(pick(
        vm,
        rules::BIT_DEPTH_OPTIONS.to_vec(),
        vm.bit_depth(),
        |depth: &BitDepth| lit!(rules::bit_depth_label(*depth)),
        tr!(ac_config_bit_depth()),
        COMBO_WIDTH,
    ));
    let channels: Box<dyn Widget> = Box::new(pick(
        vm,
        rules::OUTPUT_CHANNEL_OPTIONS
            .iter()
            .map(|(_, count)| *count)
            .collect(),
        vm.channels(),
        |count| output_channels_label(*count),
        tr!(ac_config_channels()),
        COMBO_WIDTH,
    ));
    let buffer: Box<dyn Widget> = Box::new(pick(
        vm,
        std::iter::once(None)
            .chain(rules::BUFFER_SIZE_OPTIONS.iter().map(|&size| Some(size)))
            .collect(),
        vm.buffer_size(),
        |size: &Option<u32>| match size {
            None => tr!(ac_config_automatic()),
            Some(size) => lit!(size.to_string()),
        },
        tr!(ac_config_buffer()),
        COMBO_WIDTH,
    ));
    let system_rate = pick(
        vm,
        selections(&rules::SYSTEM_RATE_OPTIONS),
        vm.system_rate(),
        |selection| selection_label(selection, " Hz"),
        tr!(ac_config_system_rate()),
        COMBO_WIDTH,
    );
    let system_quantum = pick(
        vm,
        selections(&rules::SYSTEM_QUANTUM_OPTIONS),
        vm.system_quantum(),
        |selection| selection_label(selection, ""),
        tr!(ac_config_system_quantum()),
        COMBO_WIDTH,
    );
    let system = Padding::new(SYSTEM_TOP, 0.0, 0.0, 0.0).child(
        row()
            .spacing(ROW_SPACING)
            .child(cell(
                LABEL_WIDTH,
                column()
                    .spacing(LABEL_LINES_SPACING)
                    .child(text(tr!(ac_config_system_title()), 14.0, TextRole::Primary))
                    .child(text(
                        tr!(ac_config_system_lines()),
                        13.0,
                        TextRole::Secondary,
                    )),
            ))
            .child(
                row()
                    .spacing(ROW_SPACING)
                    .child(system_rate)
                    .child(system_quantum),
            ),
    );
    column()
        .spacing(SETTINGS_SPACING)
        .child(setting(tr!(ac_config_host_label()), host))
        .child(setting(tr!(ac_config_device_label()), device))
        .child(setting(tr!(ac_config_sample_rate_label()), rate))
        .child(setting(tr!(ac_config_bit_depth_label()), depth))
        .child(setting(tr!(ac_config_channels_label()), channels))
        .child(setting(tr!(ac_config_buffer_label()), buffer))
        .child(system)
}

/// A value of the information table: centred in its column.
fn table_value(info: &Signal<InfoRows>, pick: fn(&InfoRows) -> String) -> impl Widget + use<> {
    let value = info.map(pick);
    dynamic(value, 14.0, TextRole::Primary)
}

/// The channel count of the file or the output, labelled.
fn channels_value(info: &Signal<InfoRows>, output: bool) -> impl Widget + use<> {
    let value = info.map(move |rows| {
        let channels = if output {
            rows.output_channels
        } else {
            rows.input_channels
        };
        channels_label(channels).resolve_now()
    });
    dynamic(value, 14.0, TextRole::Primary)
}

/// The information table (`:2455-2490`).
fn table(info: &Signal<InfoRows>) -> impl Widget + use<> {
    let heading = |label: LocalizedString| text(label, 12.0, TextRole::Secondary);
    let labels = column()
        .spacing(TABLE_SPACING)
        .child(FixedSize::new().height(12.0 * 1.3).child(Spacer::new()))
        .child(text(tr!(ac_config_frequency()), 14.0, TextRole::Primary))
        .child(text(tr!(ac_config_depth()), 14.0, TextRole::Primary))
        .child(text(
            tr!(ac_config_channels_short()),
            14.0,
            TextRole::Primary,
        ));
    let side = |output: bool| {
        let rate = if output {
            table_value(info, InfoRows::output_rate_text)
        } else {
            table_value(info, InfoRows::input_rate_text)
        };
        let depth = if output {
            table_value(info, InfoRows::output_depth_text)
        } else {
            table_value(info, InfoRows::input_depth_text)
        };
        VStack::new()
            .spacing(TABLE_SPACING)
            .alignment(HAlignment::Center)
            .child(heading(if output {
                tr!(ac_config_output())
            } else {
                tr!(ac_config_input())
            }))
            .child(rate)
            .child(depth)
            .child(channels_value(info, output))
    };
    let line = FixedSize::new()
        .width(LINE_WIDTH)
        .height(TABLE_LINE_HEIGHT)
        .child(RectWidget::new().background(SurfaceRole::Raised));
    row()
        .child(cell(TABLE_LABELS, labels))
        .child(Expand::new().flex(1.0).child(centred(side(false))))
        .child(
            FixedSize::new()
                .width(TABLE_DIVIDER_CELL)
                .child(centred(line)),
        )
        .child(Expand::new().flex(1.0).child(centred(side(true))))
}

/// `content` centred in the space it is given.
fn centred(content: impl Widget + 'static) -> impl Widget {
    teksilo::widgets::Center::new().child(content)
}

/// The song's title, or "Sin reproducción" (`:2410`).
fn song_title(info: &Signal<InfoRows>) -> impl Widget + use<> {
    let title = info.map(|rows| rows.title.clone());
    let shown = info.map(|rows| !rows.title.is_empty());
    let idle = info.map(|rows| rows.title.is_empty());
    ZStack::new()
        .alignment(teksilo::tokens::Alignment::CENTER_LEADING)
        .child(
            TextWidget::new(lit!(""))
                .text(title)
                .style(style::inter(14.0, FontWeight::MEDIUM))
                .color(TextRole::Primary)
                .single_line()
                .overflow(TextOverflow::Ellipsis(EllipsisMode::Trailing))
                .visible_when(shown),
        )
        .child(text(tr!(ac_config_idle()), 14.0, TextRole::Primary).visible_when(idle))
}

/// The information column.
fn information(vm: &ConfigViewModel) -> impl Widget + use<> {
    let info = vm.info();
    let status = vm.system_status();
    let status_line = status.map(|status| status.status.clone());
    let latency = status.map(|status| status.latency.clone());
    let title = Expand::horizontal().child(centred(text(
        tr!(ac_config_info_title()),
        16.0,
        TextRole::Primary,
    )));
    let song = row()
        .child(cell(
            INFO_LABEL_WIDTH,
            text(tr!(ac_config_song()), 14.0, TextRole::Primary),
        ))
        .child(Expand::horizontal().child(song_title(&info)));
    let format = row()
        .child(cell(
            INFO_LABEL_WIDTH,
            text(tr!(ac_config_format()), 14.0, TextRole::Primary),
        ))
        .child(dynamic(
            info.map(|rows| rows.format_text()),
            14.0,
            TextRole::Primary,
        ));
    let wide = |top: LocalizedString, sub: Option<LocalizedString>| {
        let mut label =
            column()
                .spacing(LABEL_LINES_SPACING)
                .child(text(top, 14.0, TextRole::Primary));
        if let Some(sub) = sub {
            label = label.child(text(sub, 13.0, TextRole::Secondary));
        }
        cell(INFO_WIDE_LABEL_WIDTH, label)
    };
    let gain = row()
        .child(wide(
            tr!(ac_config_gain()),
            Some(tr!(ac_config_gain_lines())),
        ))
        .child(dynamic(
            info.map(|rows| rows.gain_text()),
            14.0,
            TextRole::Primary,
        ));
    let quantum = row()
        .child(wide(tr!(ac_config_quantum()), None))
        .child(dynamic(
            info.map(|rows| rows.quantum_text()),
            14.0,
            TextRole::Primary,
        ));
    let server = row()
        .child(wide(
            tr!(ac_config_server()),
            Some(tr!(ac_config_system_lines())),
        ))
        .child(
            column()
                .spacing(LABEL_LINES_SPACING)
                .child(dynamic(status_line, 14.0, TextRole::Primary))
                .child(dynamic(latency, 13.0, TextRole::Secondary)),
        );
    column()
        .spacing(INFO_SPACING)
        .child(title)
        .child(song)
        .child(format)
        .child(table(&info))
        .child(gain)
        .child(quantum)
        .child(server)
}

/// The engine's notice, when there is one.
fn notice(vm: &ConfigViewModel) -> impl Widget + use<> {
    let message = vm.notice();
    let text_signal = message.map(|notice| notice.clone().unwrap_or_default());
    let shown = message.map(|notice| notice.is_some());
    TextWidget::new(lit!(""))
        .text(text_signal)
        .style(style::inter(13.0, FontWeight::REGULAR))
        .color(TextRole::Accent)
        .visible_when(shown)
}

/// The three buttons.
fn buttons(vm: &ConfigViewModel) -> impl Widget + use<> {
    let restart = vm.clone();
    let reset = vm.clone();
    let apply = vm.clone();
    row()
        .spacing(ROW_SPACING)
        .child(
            text_button(tr!(ac_config_restart()), BUTTON_TEXT, BUTTON_PADDING)
                .on_activate(move |_| restart.restart_service()),
        )
        .child(
            text_button(tr!(ac_config_reset()), BUTTON_TEXT, BUTTON_PADDING)
                .on_activate(move |_| reset.reset_to_defaults()),
        )
        .child(
            text_button(tr!(ac_config_apply()), BUTTON_TEXT, BUTTON_PADDING)
                .enabled(vm.apply_enabled())
                .on_activate(move |ctx| apply.apply(ctx)),
        )
}

/// The Audio Center's configuration tab; see the module documentation.
pub struct ConfigTab {
    vm: ConfigViewModel,
    root: Option<WidgetId>,
}

impl std::fmt::Debug for ConfigTab {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigTab").finish_non_exhaustive()
    }
}

impl ConfigTab {
    /// The tab over `vm`.
    pub fn new(vm: ConfigViewModel) -> Self {
        Self { vm, root: None }
    }
}

impl Widget for ConfigTab {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        self.vm.wire(ctx);
        let vm = &self.vm;
        let divider = FixedSize::new().width(DIVIDER_CELL).child(
            Padding::new(DIVIDER_TOP, 0.0, 0.0, 0.0).child(centred(
                FixedSize::new()
                    .width(LINE_WIDTH)
                    .height(DIVIDER_HEIGHT)
                    .child(RectWidget::new().background(SurfaceRole::Raised)),
            )),
        );
        let halves = HStack::new()
            .spacing(0.0)
            .alignment(VAlignment::Top)
            .child(
                Expand::new()
                    .flex(5.0)
                    .child(Padding::new(LEFT_TOP, 0.0, 0.0, 0.0).child(settings(vm))),
            )
            .child(divider)
            .child(
                Expand::new().flex(4.0).child(
                    Padding::new(INFO_PADDING.0, INFO_PADDING.2, 0.0, INFO_PADDING.1)
                        .child(information(vm)),
                ),
            );
        let root = ctx.add(
            column()
                .child(Expand::new().child(halves))
                .child(notice(vm))
                .child(buttons(vm))
                .child(Poller {
                    vm: vm.clone(),
                    shown: vm.shown(),
                    tick: None,
                }),
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

#[cfg(test)]
mod tests;
