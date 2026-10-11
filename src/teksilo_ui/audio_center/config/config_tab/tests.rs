use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::tokens::Alignment;

use super::*;
use crate::audio::engine::AudioDeviceInfo;
use crate::frontend::fakes::{InMemoryFakes, PlayerCall, SystemAudioCall};
use crate::teksilo_ui::app::services::{AppServices, audio_center_tab};
use crate::teksilo_ui::audio_center::shell::testing::{
    find_by_name, services_with_fakes, tree_for,
};
use crate::teksilo_ui::test_support::{
    a11y_node, access_click, first_of_type, key_down, layout, run_until, with_shipped_messages,
};

/// The size of the body the panel gives a tab: 906 wide, and what is left of
/// 480 under the header (40), the tab strip, its 2 px line and the 15 px below.
const BODY: (f32, f32) = (906.0, 391.0);

struct Fixture {
    services: AppServices,
    fakes: InMemoryFakes,
    vm: ConfigViewModel,
    tree: WidgetTree,
    tab: WidgetId,
}

fn fixture() -> Fixture {
    let (services, fakes) = services_with_fakes();
    fakes
        .player
        .set_hosts(vec!["Fake".to_string(), "ALSA".to_string()]);
    fakes.player.set_devices(vec![
        AudioDeviceInfo {
            name: "Fake Output".to_string(),
            id: Some("fake-output".to_string()),
        },
        AudioDeviceInfo {
            name: "USB DAC".to_string(),
            id: Some("usb-dac".to_string()),
        },
    ]);
    let vm = ConfigViewModel::new(&services);
    let (mut tree, _toasts) = tree_for(&services);
    let tab = tree.add(ConfigTab::new(vm.clone()));
    tree.add(
        ZStack::new().alignment(Alignment::TOP_LEADING).child(
            FixedSize::new()
                .width(BODY.0)
                .height(BODY.1)
                .child(tab_slot(tab)),
        ),
    );
    layout(&mut tree);
    Fixture {
        services,
        fakes,
        vm,
        tree,
        tab,
    }
}

/// `tab` placed in its slot.
fn tab_slot(tab: WidgetId) -> impl Widget {
    ZStack::new().alignment(Alignment::TOP_LEADING).child(tab)
}

impl Fixture {
    /// Opens the Audio Center on the configuration tab and waits for the
    /// engine's state.
    fn show(&mut self) {
        self.services
            .links
            .audio_center
            .toggle(Some(audio_center_tab::CONFIG));
        layout(&mut self.tree);
        let devices = self.services.audio_devices.clone();
        assert!(run_until(
            &self.services.runtime,
            std::time::Duration::from_secs(10),
            || devices.state().get().is_some()
        ));
        layout(&mut self.tree);
    }

    fn find(&mut self, name: &str) -> Vec<WidgetId> {
        find_by_name(&mut self.tree, self.tab, name)
    }

    fn named(&mut self, name: &str) -> WidgetId {
        let found = self.find(name);
        assert_eq!(found.len(), 1, "{name}: {found:?}");
        found[0]
    }

    fn bounds(&mut self, name: &str) -> Rect {
        let id = self.named(name);
        self.tree.bounds(id)
    }

    /// The accessibility nodes of `role`, in tree order.
    fn all_of_role(&mut self, role: Role) -> Vec<WidgetId> {
        let nodes: std::collections::HashSet<_> = self
            .tree
            .sync_accessibility()
            .nodes
            .into_iter()
            .filter(|(_, node)| node.role() == role)
            .map(|(id, _)| id)
            .collect();
        let mut found = Vec::new();
        let mut stack = vec![self.tab];
        while let Some(id) = stack.pop() {
            if nodes.contains(&teksilo::core::accessibility::widget_id_to_node_id(id)) {
                found.push(id);
            }
            stack.extend(self.tree.children(id).into_iter().rev());
        }
        found
    }

    fn combos(&mut self) -> Vec<Rect> {
        self.all_of_role(Role::ComboBox)
            .into_iter()
            .map(|id| self.tree.bounds(id))
            .collect()
    }
}

// ---- geometry ----

#[test]
fn the_settings_are_rows_of_a_180_pixel_label_and_a_combo_box_16_pixels_apart() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let combos = f.combos();
        assert_eq!(combos.len(), 8, "{combos:?}");
        let widths: Vec<f32> = combos.iter().map(|c| c.width).collect();
        assert_eq!(
            widths,
            [125.0, 260.0, 125.0, 125.0, 125.0, 125.0, 125.0, 125.0]
        );
        let host_label = f.bounds("Audio Core:");
        assert_eq!(host_label.x, 0.0);
        assert_eq!(combos[0].x, LABEL_WIDTH + ROW_SPACING);
        assert_eq!(combos[0].y, LEFT_TOP, "16 px under the top");
        for pair in combos[..6].windows(2) {
            assert_eq!(pair[1].x, combos[0].x);
            assert_eq!(pair[1].y, pair[0].y + pair[0].height + SETTINGS_SPACING);
        }
        // The two sound-server combo boxes sit side by side, 10 px apart.
        assert_eq!(combos[6].x, LABEL_WIDTH + ROW_SPACING);
        assert_eq!(combos[7].x, combos[6].x + combos[6].width + ROW_SPACING);
        assert_eq!(combos[6].y, combos[7].y);
        assert!(
            combos[6].y >= combos[5].y + combos[5].height + SETTINGS_SPACING + SYSTEM_TOP,
            "4 px lower than the others"
        );
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_settings_take_five_ninths_of_what_the_divider_leaves() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let left = (BODY.0 - DIVIDER_CELL) * 5.0 / 9.0;
        let line = f
            .tree
            .children(f.tab)
            .into_iter()
            .flat_map(|id| descendants(&f.tree, id))
            .map(|id| f.tree.bounds(id))
            .find(|b| b.width == LINE_WIDTH && b.height == DIVIDER_HEIGHT)
            .expect("the 2 x 220 divider");
        assert!(
            (line.x - (left + (DIVIDER_CELL - LINE_WIDTH) / 2.0)).abs() < 0.5,
            "{line:?}"
        );
        assert!(line.y >= DIVIDER_TOP);
        // The information column starts after the divider cell.
        let title = f.bounds("Audio Information");
        let middle = title.x + title.width / 2.0;
        let column_start = left + DIVIDER_CELL + INFO_PADDING.1;
        let column_end = BODY.0 - INFO_PADDING.2;
        assert!(
            (middle - (column_start + column_end) / 2.0).abs() < 1.0,
            "the title is centred in the column: {title:?}"
        );
        assert!(f.services.ctx.shutdown());
    });
}

/// Every widget under `root`, depth first, without the parked ones.
fn descendants(tree: &WidgetTree, root: WidgetId) -> Vec<WidgetId> {
    let mut found = vec![root];
    for child in tree.children(root) {
        found.extend(descendants(tree, child));
    }
    found
}

#[test]
fn the_information_rows_have_the_iced_label_cells() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let song = f.bounds("Song:");
        let format = f.bounds("Format:");
        let gain = f.bounds("Gain:");
        let quantum = f.bounds("Audoxidy quantum:");
        let server = f.bounds("Pipewire | PulseAudio:");
        assert_eq!(song.x, format.x);
        assert_eq!(gain.x, quantum.x);
        assert_eq!(gain.x, server.x);
        // The value of a 170 px row starts 170 px after its label.
        let gain_value = f.bounds("-- | --");
        assert_eq!(gain_value.x, gain.x + INFO_WIDE_LABEL_WIDTH);
        // The fake engine's buffer of 512 frames.
        let frames = f.bounds("512");
        assert_eq!(frames.x, quantum.x + INFO_WIDE_LABEL_WIDTH);
        // The 80 px cell of the song and the format.
        let format_value = f
            .find("--")
            .into_iter()
            .map(|id| f.tree.bounds(id))
            .find(|b| (b.y - format.y).abs() < 4.0)
            .expect("the format's value is on the format's row");
        assert_eq!(format_value.x, format.x + INFO_LABEL_WIDTH);
        assert!(song.y < format.y && format.y < gain.y && gain.y < quantum.y);
        assert!(quantum.y < server.y);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_table_has_the_file_and_the_output_either_side_of_a_line() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let input = f.bounds("Input");
        let output = f.bounds("Output");
        let frequency = f.bounds("Rate:");
        let depth = f.bounds("Depth:");
        let channels = f.bounds("Channels:");
        assert!(input.x > frequency.x + TABLE_LABELS - 1.0);
        assert!(output.x > input.x);
        assert_eq!(input.y, output.y);
        assert!(frequency.y < depth.y && depth.y < channels.y);
        assert!(
            ((depth.y - frequency.y) - (channels.y - depth.y)).abs() < 0.01,
            "evenly spaced"
        );
        let line = f
            .tree
            .children(f.tab)
            .into_iter()
            .flat_map(|id| descendants(&f.tree, id))
            .map(|id| f.tree.bounds(id))
            .find(|b| b.width == LINE_WIDTH && b.height == TABLE_LINE_HEIGHT)
            .expect("the 2 x 90 line");
        assert!(line.x > input.x && line.x < output.x, "{line:?}");
        // The output stream of the fake engine.
        assert_eq!(f.find("48000 Hz").len(), 1);
        assert_eq!(f.find("32 bits Float").len(), 1);
        assert_eq!(f.find("Stereo").len(), 1);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_three_buttons_are_10_pixels_apart_at_the_bottom_leading_corner() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let restart = f.bounds("Restart Audio Service");
        let reset = f.bounds("Default");
        let apply = f.bounds("Apply");
        assert_eq!(restart.x, 0.0);
        assert_eq!(reset.x, restart.x + restart.width + ROW_SPACING);
        assert_eq!(apply.x, reset.x + reset.width + ROW_SPACING);
        assert_eq!(restart.y + restart.height, BODY.1, "the row ends the tab");
        // 15 px text with 8 x 12 px of padding.
        let restart_button = f.named("Restart Audio Service");
        let label = f
            .tree
            .bounds(first_of_type(&f.tree, restart_button, "TextWidget").expect("the label"));
        assert_eq!(restart.width, label.width + 2.0 * BUTTON_PADDING.1);
        assert_eq!(restart.height, label.height + 2.0 * BUTTON_PADDING.0);
        assert!(f.services.ctx.shutdown());
    });
}

// ---- what it shows ----

#[test]
fn the_fake_engines_selections_are_shown_after_the_first_open() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let hosts = f.all_of_role(Role::ComboBox);
        let value = |f: &mut Fixture, id: WidgetId| {
            a11y_node(&mut f.tree, id)
                .value()
                .map(str::to_string)
                .unwrap_or_default()
        };
        let shown: Vec<String> = hosts.iter().map(|id| value(&mut f, *id)).collect();
        assert_eq!(shown[0], "ALSA");
        assert_eq!(shown[1], "Fake Output");
        assert_eq!(shown[2], "48000 Hz");
        assert_eq!(shown[3], "32 bits Float");
        assert_eq!(shown[4], "2.0 Stereo");
        assert_eq!(shown[5], "Automatic");
        assert_eq!(shown[6], "Default");
        assert_eq!(shown[7], "Default");
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_notice_shows_in_the_accent_when_the_engine_has_one() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        assert!(f.find("The saved output is gone").is_empty());
        f.services
            .now_playing
            .on_event(&crate::frontend::Event::Playback(
                crate::frontend::events::PlaybackEvent::NoticeChanged(Some(
                    "The saved output is gone".to_string(),
                )),
            ));
        layout(&mut f.tree);
        assert_eq!(f.find("The saved output is gone").len(), 1);
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_polled_status_and_latency_are_shown() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        f.vm.poll(Instant::now());
        let status = f.vm.system_status();
        assert!(run_until(
            &f.services.runtime,
            std::time::Duration::from_secs(10),
            || status.get().status != "--"
        ));
        layout(&mut f.tree);
        assert_eq!(f.find("PipeWire 48000 Hz").len(), 1);
        assert_eq!(f.find("1024 / 48000").len(), 1);
        assert!(f.services.ctx.shutdown());
    });
}

// ---- interaction ----

#[test]
fn apply_is_dimmed_and_inert_until_a_selection_changes() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let apply = f.named("Apply");
        assert!(a11y_node(&mut f.tree, apply).is_disabled());
        access_click(&mut f.tree, apply);
        assert!(!f.vm.busy().get());
        f.vm.selection_changed();
        layout(&mut f.tree);
        assert!(!a11y_node(&mut f.tree, apply).is_disabled());
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn choosing_in_a_combo_box_selects_and_enables_apply() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let rate = f.named("Sample rate");
        // ArrowDown opens the list and moves the highlight one past the value,
        // Enter commits it.
        key_down(&mut f.tree, rate, teksilo::core::Key::ArrowDown);
        key_down(&mut f.tree, rate, teksilo::core::Key::Enter);
        let chosen = rules::SAMPLE_RATE_OPTIONS
            .iter()
            .position(|&rate| rate == 48_000)
            .map(|at| rules::SAMPLE_RATE_OPTIONS[at + 1]);
        assert_eq!(f.vm.sample_rate().get(), chosen);
        assert_eq!(f.vm.pending().sample_rate, chosen);
        assert!(f.vm.apply_enabled().get());
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn predeterminado_selects_the_defaults() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        f.vm.sample_rate().set(Some(192_000));
        let reset = f.named("Default");
        access_click(&mut f.tree, reset);
        assert_eq!(f.vm.sample_rate().get(), Some(rules::DEFAULT_SAMPLE_RATE));
        assert!(f.vm.apply_enabled().get());
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn reiniciar_restarts_the_audio_services() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        let restart = f.named("Restart Audio Service");
        access_click(&mut f.tree, restart);
        let system_audio = f.fakes.system_audio.clone();
        assert!(run_until(
            &f.services.runtime,
            std::time::Duration::from_secs(10),
            || system_audio
                .calls()
                .contains(&SystemAudioCall::RestartAudioServices)
        ));
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn the_apply_button_starts_apply() {
    with_shipped_messages("en-US", || {
        let mut f = fixture();
        f.show();
        f.vm.selection_changed();
        layout(&mut f.tree);
        let apply = f.named("Apply");
        access_click(&mut f.tree, apply);
        assert!(f.vm.busy().get());
        layout(&mut f.tree);
        assert!(
            a11y_node(&mut f.tree, apply).is_disabled(),
            "disabled while it runs"
        );
        let player = f.fakes.player.clone();
        assert!(run_until(
            &f.services.runtime,
            std::time::Duration::from_secs(10),
            || player
                .calls()
                .iter()
                .any(|call| matches!(call, PlayerCall::ApplyAudioSettings(_)))
        ));
        assert!(f.services.ctx.shutdown());
    });
}

// ---- the poll ----

fn render(tree: &mut WidgetTree) {
    let _ = tree.render();
    layout(tree);
}

#[test]
fn the_poll_subscription_lives_only_while_the_tab_is_shown() {
    let mut f = fixture();
    render(&mut f.tree);
    let _ = f.tree.render();
    assert_eq!(f.tree.frame_tick_deadline(), None, "closed: nothing wakes");

    f.services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::CONFIG));
    layout(&mut f.tree);
    render(&mut f.tree);
    let _ = f.tree.render();
    assert!(f.tree.frame_tick_deadline().is_some(), "shown: polling");

    f.services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::EFFECTS));
    layout(&mut f.tree);
    render(&mut f.tree);
    let _ = f.tree.render();
    assert_eq!(
        f.tree.frame_tick_deadline(),
        None,
        "another tab: the guard is dropped"
    );

    f.services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::CONFIG));
    layout(&mut f.tree);
    render(&mut f.tree);
    let _ = f.tree.render();
    assert!(f.tree.frame_tick_deadline().is_some());

    f.services.links.audio_center.close();
    layout(&mut f.tree);
    render(&mut f.tree);
    let _ = f.tree.render();
    assert_eq!(
        f.tree.frame_tick_deadline(),
        None,
        "closed again: polling stops"
    );
    assert!(f.services.ctx.shutdown());
}

#[test]
fn a_frame_while_shown_polls_the_sound_server_once_and_not_again_for_3_seconds() {
    let mut f = fixture();
    f.show();
    let status_calls = |f: &Fixture| {
        f.fakes
            .system_audio
            .calls()
            .iter()
            .filter(|call| **call == SystemAudioCall::SystemSoundStatus)
            .count()
    };
    for _ in 0..5 {
        f.tree.frame_tick().set(0.016);
    }
    assert!(run_until(
        &f.services.runtime,
        std::time::Duration::from_secs(10),
        || status_calls(&f) >= 1
    ));
    assert_eq!(status_calls(&f), 1, "five frames, one request");
    assert!(f.services.ctx.shutdown());
}

#[test]
fn a_hidden_tab_asks_the_sound_server_nothing() {
    let mut f = fixture();
    layout(&mut f.tree);
    for _ in 0..5 {
        f.tree.frame_tick().set(0.016);
    }
    run_until(
        &f.services.runtime,
        std::time::Duration::from_millis(50),
        || false,
    );
    assert!(
        !f.fakes
            .system_audio
            .calls()
            .contains(&SystemAudioCall::SystemSoundStatus)
    );
    assert!(f.services.ctx.shutdown());
}

// ---- Spanish ----

#[test]
fn the_spanish_strings_are_the_iced_ones() {
    with_shipped_messages("es-ES", || {
        let mut f = fixture();
        f.show();
        for name in [
            "Núcleo de Audio:",
            "Dispositivo de Salida:",
            "Frecuencia de Muestreo:",
            "Profundidad de Bits:",
            "Canales de Salida:",
            "Quantum (Buffer):",
            "Pipewire | PulseAudio",
            "Información de Audio",
            "Canción:",
            "Formato:",
            "Frecuencia:",
            "Profundidad:",
            "Canales:",
            "Entrada",
            "Salida",
            "Ganancia:",
            "Album | Canción",
            "Quantum de Audoxidy:",
            "Pipewire | PulseAudio:",
            "Reiniciar Servicio de Audio",
            "Predeterminado",
            "Aplicar",
        ] {
            assert!(!f.find(name).is_empty(), "{name}");
        }
        // The pick lists' own labels and the corrected "Automático".
        assert_eq!(f.find("Frecuencia | Quantum").len(), 2);
        let buffer = f.named("Quantum (Buffer)");
        assert_eq!(a11y_node(&mut f.tree, buffer).value(), Some("Automático"));
        assert!(f.services.ctx.shutdown());
    });
}

#[test]
fn a_channel_count_without_a_name_is_labelled_in_both_languages() {
    with_shipped_messages("es-ES", || {
        assert_eq!(channels_label(Some(5)).resolve_now(), "5 canales");
        assert_eq!(channels_label(Some(2)).resolve_now(), "Stereo");
        assert_eq!(channels_label(None).resolve_now(), "--");
    });
    with_shipped_messages("en-US", || {
        assert_eq!(channels_label(Some(5)).resolve_now(), "5 channels");
    });
}

#[test]
fn a_saved_device_that_is_not_listed_is_still_shown_selected() {
    with_shipped_messages("en-US", || {
        let (services, _fakes) = services_with_fakes();
        crate::frontend::test_fixtures::LibraryFixture::new()
            .setting(
                crate::frontend::settings::keys::AUDIO_DEVICE,
                "Unplugged DAC",
            )
            .seed_on(&services.ctx.db)
            .recv_blocking()
            .unwrap();
        let vm = ConfigViewModel::new(&services);
        let (mut tree, _toasts) = tree_for(&services);
        let tab = tree.add(ConfigTab::new(vm.clone()));
        tree.add(
            ZStack::new().alignment(Alignment::TOP_LEADING).child(
                FixedSize::new()
                    .width(BODY.0)
                    .height(BODY.1)
                    .child(tab_slot(tab)),
            ),
        );
        layout(&mut tree);
        services
            .links
            .audio_center
            .toggle(Some(audio_center_tab::CONFIG));
        layout(&mut tree);
        let devices = services.audio_devices.clone();
        assert!(run_until(
            &services.runtime,
            std::time::Duration::from_secs(10),
            || devices.state().get().is_some()
        ));
        layout(&mut tree);
        let combo = find_by_name(&mut tree, tab, "Output device")[0];
        assert_eq!(
            a11y_node(&mut tree, combo).value(),
            Some("Unplugged DAC"),
            "Iced shows the saved name in its pick list as well"
        );
        assert!(services.ctx.shutdown());
    });
}

#[test]
fn inside_the_panel_polling_stops_when_the_panel_closes_or_changes_tab() {
    use crate::teksilo_ui::audio_center::shell::{AudioCenterPanel, AudioCenterViewModel};
    let (services, _fakes) = services_with_fakes();
    let shell = AudioCenterViewModel::new(&services);
    let config = ConfigViewModel::new(&services);
    let (mut tree, _toasts) = tree_for(&services);
    let spare = || -> Box<dyn Widget> { Box::new(Spacer::new()) };
    let panel = AudioCenterPanel::new(
        shell.clone(),
        Box::new(ConfigTab::new(config)),
        spare(),
        spare(),
        spare(),
    );
    tree.add(ZStack::new().alignment(Alignment::TOP_LEADING).child(panel));
    layout(&mut tree);
    let polling = |tree: &mut WidgetTree| {
        layout(tree);
        let _ = tree.render();
        layout(tree);
        let _ = tree.render();
        tree.frame_tick_deadline().is_some()
    };
    assert!(!polling(&mut tree), "closed");
    services
        .links
        .audio_center
        .toggle(Some(audio_center_tab::CONFIG));
    assert!(polling(&mut tree), "open on the configuration tab");
    shell.select_tab(audio_center_tab::EQUALIZER);
    assert!(!polling(&mut tree), "another tab");
    shell.select_tab(audio_center_tab::CONFIG);
    assert!(polling(&mut tree), "back");
    shell.close();
    assert!(!polling(&mut tree), "closed again");
    assert!(services.ctx.shutdown());
}
