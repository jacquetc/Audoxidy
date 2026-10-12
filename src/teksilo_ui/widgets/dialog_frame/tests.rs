use std::cell::Cell;
use std::rc::Rc;

use teksilo::core::WidgetTree;
use teksilo::core::accesskit::Role;
use teksilo::widgets::{Button, FixedSize, ModalContainer, Spacer, TextInput, VStack};

use super::*;
use crate::teksilo_ui::test_support::{
    access_click, drain_dialog_titles, find_by_name, first_of_type, layout, press,
    with_shipped_messages,
};

fn tree() -> WidgetTree {
    let mut tree = WidgetTree::new();
    let mut theme = style::audoxidy_dark();
    install(&mut theme.style_slots);
    tree.set_theme(theme);
    tree
}

fn mounted(widget: impl Widget + 'static) -> (WidgetTree, WidgetId) {
    let mut tree = tree();
    let id = tree.add(widget);
    tree.add(
        VStack::new()
            .spacing(0.0)
            .alignment(HAlignment::Leading)
            .child(id)
            .child(Spacer::new()),
    );
    layout(&mut tree);
    let _ = tree.render();
    (tree, id)
}

fn block(width: f32, height: f32) -> FixedSize {
    FixedSize::new()
        .width(width)
        .height(height)
        .child(Spacer::new())
}

#[test]
fn a_frame_pads_its_content_and_keeps_its_minimum_width() {
    let (tree, id) = mounted(Frame::new(block(50.0, 20.0)).padding(15.0, 15.0, 15.0, 15.0));
    let frame = tree.bounds(id);
    assert_eq!((frame.width, frame.height), (80.0, 50.0));
    let content = first_of_type(&tree, id, "FixedSize").unwrap();
    assert_eq!(tree.bounds(content).x - frame.x, 15.0);

    let (tree, id) = mounted(Frame::new(block(50.0, 20.0)).min_width(230.0));
    assert_eq!(tree.bounds(id).width, 230.0);
}

#[test]
fn the_dialog_panel_pads_by_15_and_is_at_least_230_wide() {
    let (tree, id) = mounted(ModalContainer::new(block(100.0, 40.0)).style(DialogFrameStyle));
    let frame = first_of_type(&tree, id, "Frame").unwrap();
    let panel = tree.bounds(frame);
    assert_eq!(panel.width, CONTENT_MIN_WIDTH + 2.0 * PANEL_PADDING);
    let content = first_of_type(&tree, frame, "FixedSize").unwrap();
    let content = tree.bounds(content);
    assert_eq!(content.y - panel.y, PANEL_PADDING);
    assert_eq!(panel.height, 40.0 + 2.0 * PANEL_PADDING);
}

#[test]
fn the_dialog_buttons_light_up_with_the_accent_on_hover() {
    assert_eq!(button_fill(false), SurfaceRole::Raised);
    assert_eq!(button_fill(true), SurfaceRole::Accent);
    assert_eq!(button_text(false), TextRole::Secondary);
    assert_eq!(button_text(true), TextRole::Primary);
}

/// A dialog with both buttons, counting what they do: cancel adds 1,
/// confirm adds 10.
fn dialog(count: &Rc<Cell<u32>>, confirm_enabled: Signal<bool>) -> DialogFrame {
    let (cancel, confirm) = (count.clone(), count.clone());
    DialogFrame::new(lit!("Renombrar lista"), block(120.0, 30.0))
        .on_cancel(move |_| cancel.set(cancel.get() + 1))
        .on_confirm(lit!("Renombrar"), move |_| confirm.set(confirm.get() + 10))
        .confirm_enabled(confirm_enabled)
}

#[test]
fn the_dialog_column_centres_the_title_content_and_footer() {
    with_shipped_messages("es-ES", || {
        let count = Rc::new(Cell::new(0));
        let (tree, id) = mounted(dialog(&count, Signal::new(true)));
        let column = tree.bounds(id);
        assert_eq!(column.width, CONTENT_MIN_WIDTH);
        let content = tree.bounds(first_of_type(&tree, id, "FixedSize").unwrap());
        let _ = content;
        let cancel = find_by_name(&tree, id, "Cancelar")[0];
        let confirm = find_by_name(&tree, id, "Renombrar")[0];
        let (cancel, confirm) = (tree.bounds(cancel), tree.bounds(confirm));
        assert_eq!(confirm.x - (cancel.x + cancel.width), FOOTER_SPACING);
        let footer_center = (cancel.x + confirm.x + confirm.width) / 2.0;
        assert!((footer_center - column.center().x).abs() < 0.5);
        let title = first_of_type(&tree, id, "TextWidget").unwrap();
        assert!((tree.bounds(title).center().x - column.center().x).abs() < 0.5);
    });
}

#[test]
fn the_footer_buttons_run_their_actions() {
    with_shipped_messages("en-US", || {
        let count = Rc::new(Cell::new(0));
        let enabled = Signal::new(false);
        let (mut tree, id) = mounted(dialog(&count, enabled.clone()));
        let cancel = find_by_name(&tree, id, "Cancel")[0];
        let confirm = find_by_name(&tree, id, "Renombrar")[0];
        assert_eq!(tree.accessibility_node(confirm).role(), Role::Button);
        access_click(&mut tree, confirm);
        assert_eq!(count.get(), 0, "a disabled confirm does nothing");
        enabled.set(true);
        layout(&mut tree);
        access_click(&mut tree, confirm);
        access_click(&mut tree, cancel);
        assert_eq!(count.get(), 11);
    });
}

#[test]
fn presenting_the_dialog_queues_a_modal_with_its_title() {
    let count = Rc::new(Cell::new(0));
    let mut tree = tree();
    let frame = Rc::new(Cell::new(Some(dialog(&count, Signal::new(true)))));
    press(&mut tree, move |ctx| {
        if let Some(frame) = frame.take() {
            frame.present(ctx);
        }
    });
    assert_eq!(drain_dialog_titles(&mut tree), vec!["Renombrar lista"]);
}

#[test]
fn the_stock_dialog_presets_queue_their_titles() {
    let mut tree = tree();
    press(&mut tree, |ctx| {
        input_dialog(
            lit!("Nueva lista de reproducción"),
            lit!("Nombre para la nueva lista:"),
            lit!("Crear"),
        )
        .placeholder(lit!("Nombre de la lista..."))
        .present(ctx);
    });
    press(&mut tree, |ctx| {
        confirm_box(
            lit!("¿Eliminar lista?"),
            lit!("Esta acción no se puede deshacer."),
            lit!("Eliminar"),
        )
        .present(ctx);
    });
    assert_eq!(
        drain_dialog_titles(&mut tree).len(),
        2,
        "both stock dialogs were presented"
    );
}

#[test]
fn modal_text_centres_every_line_and_reads_as_one_text() {
    let (tree, id) = mounted(
        FixedSize::new()
            .width(300.0)
            .child(ModalText::new(lit!("¿Estás seguro de querer eliminar\nla lista \"Oxidy Drift\"?\nEsta acción no se puede deshacer."))),
    );
    let text = first_of_type(&tree, id, "ModalText").unwrap();
    let mut lines = Vec::new();
    let mut stack = vec![text];
    while let Some(node) = stack.pop() {
        if tree
            .widget_type_name(node)
            .is_some_and(|name| name.ends_with("TextWidget"))
        {
            lines.push(tree.bounds(node));
        }
        stack.extend(tree.children(node));
    }
    assert_eq!(lines.len(), 3);
    let center = lines[0].center().x;
    assert!(
        lines
            .iter()
            .all(|line| (line.center().x - center).abs() < 0.5)
    );
    let label = tree.find_by_role(Role::Label).unwrap();
    assert!(
        tree.accessibility_node(label)
            .name()
            .is_some_and(|name| name.contains("Oxidy Drift"))
    );
}

#[test]
fn a_stock_button_in_the_dialog_style_pads_its_label() {
    let (mut tree, id) = mounted(Button::new(lit!("Eliminar")).style(DialogButtonStyle::default()));
    let label = first_of_type(&tree, id, "TextWidget").unwrap();
    assert_eq!(tree.bounds(label).x - tree.bounds(id).x, BUTTON_PADDING.1);
    tree.pointer_move(tree.bounds(id).center());
    layout(&mut tree);
    let _ = tree.render();
}

#[test]
fn a_dialog_field_is_framed_and_editable() {
    let text = Signal::new(String::new());
    let (mut tree, _) = mounted(
        FixedSize::new().width(200.0).child(
            TextInput::new(text.clone())
                .style(DialogInputStyle)
                .label(lit!("Nombre")),
        ),
    );
    let field = tree.find_by_role(Role::TextInput).unwrap();
    tree.focus(field);
    tree.type_text(field, "Glam");
    layout(&mut tree);
    assert_eq!(text.get(), "Glam");
}

#[test]
fn install_fills_the_dialog_button_and_field_slots() {
    let mut theme = teksilo::prelude::intui::dark();
    install(&mut theme.style_slots);
    let installed = theme.style_slots.installed();
    for slot in ["dialog", "button", "text_input"] {
        assert!(installed.contains(&slot), "{slot} in {installed:?}");
    }
}

#[test]
fn the_audio_center_buttons_take_the_larger_padding() {
    let (tree, id) =
        mounted(Button::new(lit!("Aplicar")).style(DialogButtonStyle::padded(8.0, 12.0)));
    let label = first_of_type(&tree, id, "TextWidget").unwrap();
    assert_eq!(tree.bounds(label).x - tree.bounds(id).x, 12.0);
}

#[test]
fn a_text_button_is_named_by_its_label_and_can_be_disabled() {
    let count = Rc::new(Cell::new(0));
    let hits = count.clone();
    let (mut tree, id) = mounted(
        text_button(lit!("Aplicar"), 15.0, (8.0, 12.0))
            .enabled(false)
            .on_activate(move |_| hits.set(hits.get() + 1)),
    );
    assert_eq!(tree.accessibility_node(id).name(), Some("Aplicar"));
    access_click(&mut tree, id);
    assert_eq!(count.get(), 0);
}

// ---- the confirmation: Enter does nothing (review #14e) -------------------

/// A presented [`confirm_box`], its content built into the tree.
struct Confirm {
    tree: WidgetTree,
    layer: WidgetId,
    answers: Rc<std::cell::RefCell<Vec<ConfirmResult>>>,
    on_dismiss: Option<teksilo::core::overlay::OverlayDismissCallback>,
}

impl Confirm {
    fn open() -> Self {
        use teksilo::core::modal::ModalContent;
        let mut tree = tree();
        let answers: Rc<std::cell::RefCell<Vec<ConfirmResult>>> = Rc::default();
        let sink = answers.clone();
        press(&mut tree, move |ctx| {
            let sink = sink.clone();
            confirm_box(
                lit!("¿Eliminar lista?"),
                lit!("Esta acción no se puede deshacer."),
                lit!("Eliminar"),
            )
            .on_result(move |result, _ctx| sink.borrow_mut().push(result))
            .present(ctx);
        });
        let queued = tree
            .drain_pending_modal_requests()
            .pop()
            .expect("a modal was presented");
        let on_dismiss = queued.request.on_dismiss.clone();
        let layer = match queued.request.content {
            ModalContent::Deferred(build) => build(&mut tree),
            ModalContent::ExistingWidget(id) => id,
        };
        layout(&mut tree);
        let _ = tree.render();
        Self {
            tree,
            layer,
            answers,
            on_dismiss,
        }
    }

    /// The button named `name` ("Cancelar" is the shipped locale's word).
    fn button(&self, name: &str) -> WidgetId {
        let name = if name == "Cancelar" {
            tr!(chrome_dialog_cancel()).resolve_now()
        } else {
            name.to_string()
        };
        find_by_name(&self.tree, self.layer, &name)[0]
    }

    fn key_on(&mut self, name: &str, key: teksilo::core::Key) {
        let button = self.button(name);
        self.tree.focus(button);
        crate::teksilo_ui::test_support::key_down(&mut self.tree, button, key);
    }

    fn answers(&self) -> Vec<ConfirmResult> {
        self.answers.borrow().clone()
    }
}

#[test]
fn enter_does_nothing_in_a_confirmation_whatever_has_the_keyboard() {
    // Iced's delete dialog takes no key (`app.rs:5296-5304`); the stock
    // `MessageBox` pressed "Cancelar", its default button, on Enter.
    let mut c = Confirm::open();
    c.key_on("Cancelar", teksilo::core::Key::Enter);
    c.key_on("Eliminar", teksilo::core::Key::Enter);
    assert!(c.answers().is_empty(), "{:?}", c.answers());
    assert!(!c.tree.drain_pending_modal_dismissal(), "still open");
    // Space still presses the button the keyboard is on.
    c.key_on("Eliminar", teksilo::core::Key::Space);
    assert_eq!(c.answers()[0].button, StandardButton::Ok);
}

#[test]
fn a_confirmations_buttons_answer_with_the_pointer() {
    let mut c = Confirm::open();
    let at = c.tree.bounds(c.button("Cancelar")).center();
    crate::teksilo_ui::test_support::tap_at(&mut c.tree, at);
    let answer = c.answers()[0];
    assert_eq!(answer.button, StandardButton::Cancel);
    assert!(!answer.was_dismissed());

    let mut c = Confirm::open();
    let at = c.tree.bounds(c.button("Eliminar")).center();
    crate::teksilo_ui::test_support::tap_at(&mut c.tree, at);
    assert_eq!(c.answers().len(), 1);
    assert_eq!(c.answers()[0].button, StandardButton::Ok);
}

#[test]
fn escape_or_a_click_outside_answers_cancel_once() {
    let mut c = Confirm::open();
    let dismiss = c.on_dismiss.clone().expect("an on_dismiss callback");
    let again = dismiss.clone();
    press(&mut c.tree, move |ctx| {
        dismiss(teksilo::core::overlay::DismissReason::Escape, ctx)
    });
    press(&mut c.tree, move |ctx| {
        again(teksilo::core::overlay::DismissReason::Escape, ctx)
    });
    let answers = c.answers();
    assert_eq!(answers.len(), 1);
    assert_eq!(answers[0].button, StandardButton::Cancel);
    assert!(answers[0].was_dismissed());
}
