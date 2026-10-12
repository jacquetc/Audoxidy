//! Test-only scaffolding for headless widget and view-model tests.
//!
//! A bare `WidgetTree` has no event source, and `ctx.subscribe_event` panics
//! without one, so a widget that subscribes to the facade's events could not
//! be laid out in a test. [`tree_with_events`] registers the same
//! [`FrontendEventSource`] the app does, over the test's [`AppContext`], with a
//! poster that drops what it is given: the subscriptions are real, they are
//! simply never delivered, since a headless tree has no event loop.
//! [`tree_with_pump`] keeps what is posted instead, and [`EventPump::deliver`]
//! hands it to the subscribers, as the event loop would.
//!
//! The input helpers drive the real dispatch path ([`access_click`],
//! [`key_down`], [`type_text`], [`pointer_click`], [`press`]), so a handler
//! runs with a real `EventContext` rather than being called directly.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use teksilo::canvas::Point;
use teksilo::core::WidgetEvent;
use teksilo::core::accessibility::widget_id_to_node_id;
use teksilo::core::accesskit::Node;
use teksilo::core::event_source::{
    AppEventPoster, EventSourceAdapter, SubscriptionId, TreeAppContext,
};
use teksilo::core::widget_id::WidgetId;
use teksilo::core::widget_tree::WidgetTree;
use teksilo::core::{EventTime, Key, Modifiers, PointerButton, PointerPhase, PointerSample};
use teksilo::prelude::{AsyncRuntimeHandle, EventContext, SizeProposal, lit};
use teksilo::widgets::Button;

use super::event_source::FrontendEventSource;
use crate::frontend::AppContext;

/// The viewport the input helpers lay the tree out in.
pub const VIEWPORT: (f32, f32) = (1360.0, 880.0);

/// A poster that drops everything: there is no event loop to post to.
struct NullPoster;

impl AppEventPoster for NullPoster {
    fn post_subscription_event(&self, _sub_id: SubscriptionId, _event: Box<dyn Any + Send>) {}
}

/// An event posted to a subscription, waiting to be delivered.
type Posted = (SubscriptionId, Box<dyn Any + Send>);

/// The events posted to a tree's subscriptions, kept until
/// [`deliver`](EventPump::deliver) hands them over: the event loop of a
/// headless tree.
#[derive(Clone, Default)]
pub struct EventPump {
    posted: Arc<Mutex<Vec<Posted>>>,
}

impl AppEventPoster for EventPump {
    fn post_subscription_event(&self, sub_id: SubscriptionId, event: Box<dyn Any + Send>) {
        self.posted.lock().unwrap().push((sub_id, event));
    }
}

impl EventPump {
    /// How many events wait to be delivered.
    pub fn pending(&self) -> usize {
        self.posted.lock().unwrap().len()
    }

    /// Delivers the waiting events to their subscribers in `tree`, in the
    /// order they were posted; returns how many. Only `subscribe_event`
    /// subscriptions: one made with `subscribe_event_with_ctx` needs an
    /// event context, which a test does not have.
    pub fn deliver(&self, tree: &WidgetTree) -> usize {
        let posted = std::mem::take(&mut *self.posted.lock().unwrap());
        let count = posted.len();
        for (sub_id, event) in posted {
            tree.app_context()
                .dispatch_subscription_event(sub_id, &*event);
        }
        count
    }
}

/// [`tree_with_async`] whose subscriptions are delivered by the returned
/// [`EventPump`].
pub fn tree_with_pump(ctx: &AppContext) -> (WidgetTree, AsyncRuntimeHandle, EventPump) {
    let runtime = AsyncRuntimeHandle::new();
    let pump = EventPump::default();
    let mut tree = WidgetTree::new();
    tree.set_theme(super::style::audoxidy_dark());
    let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
    state.insert(
        TypeId::of::<AsyncRuntimeHandle>(),
        Box::new(runtime.clone()),
    );
    let adapter = EventSourceAdapter::new(FrontendEventSource::new(ctx.hub.clone()));
    tree.set_app_context(Rc::new(
        TreeAppContext::with_source_and_poster(adapter, Arc::new(pump.clone()))
            .with_app_state(state),
    ));
    (tree, runtime, pump)
}

/// A `WidgetTree` whose widgets can subscribe to the facade's events.
pub fn tree_with_events(ctx: &AppContext) -> WidgetTree {
    tree_with_app_state(ctx, HashMap::new())
}

/// [`tree_with_events`], with `state` registered as app state (what
/// `TeksiloAppBuilder::app_state` registers in the app).
pub fn tree_with_app_state(ctx: &AppContext, state: HashMap<TypeId, Box<dyn Any>>) -> WidgetTree {
    let mut tree = WidgetTree::new();
    tree.set_theme(super::style::audoxidy_dark());
    let adapter = EventSourceAdapter::new(FrontendEventSource::new(ctx.hub.clone()));
    tree.set_app_context(Rc::new(
        TreeAppContext::with_source_and_poster(adapter, Arc::new(NullPoster)).with_app_state(state),
    ));
    tree
}

/// [`tree_with_events`] with a main-thread async runtime registered, as
/// `install_async` does in the app. The returned handle is the test's executor:
/// drive it with [`run_until`].
pub fn tree_with_async(ctx: &AppContext) -> (WidgetTree, AsyncRuntimeHandle) {
    let runtime = AsyncRuntimeHandle::new();
    let mut state: HashMap<TypeId, Box<dyn Any>> = HashMap::new();
    state.insert(
        TypeId::of::<AsyncRuntimeHandle>(),
        Box::new(runtime.clone()),
    );
    (tree_with_app_state(ctx, state), runtime)
}

/// Ticks `runtime` until `done` holds, for at most `timeout`. Returns whether
/// it held. The futures under test await replies from the facade's worker
/// threads, so this polls, sleeping a little between ticks.
pub fn run_until(runtime: &AsyncRuntimeHandle, timeout: Duration, done: impl Fn() -> bool) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        runtime.tick();
        if done() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

/// Lays `tree` out at [`VIEWPORT`].
pub fn layout(tree: &mut WidgetTree) {
    tree.layout(SizeProposal::exact(VIEWPORT.0, VIEWPORT.1));
}

/// Activates `id` through the accessibility `Click` action, the path a screen
/// reader takes and the one that reaches a widget's `on_access_action`.
pub fn access_click(tree: &mut WidgetTree, id: WidgetId) {
    tree.dispatch_event(WidgetEvent::AccessAction {
        action: teksilo::core::accesskit::Action::Click,
        target: Some(id),
        target_node: widget_id_to_node_id(id),
        data: None,
    });
    layout(tree);
}

/// Focuses `id`, then presses `key` with no modifier.
pub fn key_down(tree: &mut WidgetTree, id: WidgetId, key: Key) {
    key_down_with(tree, id, key, Modifiers::NONE);
}

/// Focuses `id`, then presses `key` with `modifiers`.
pub fn key_down_with(tree: &mut WidgetTree, id: WidgetId, key: Key, modifiers: Modifiers) {
    tree.focus(id);
    tree.dispatch_event(WidgetEvent::KeyDown {
        key,
        modifiers,
        text: None,
    });
    layout(tree);
}

/// Types `text` into the focused widget: one `KeyDown` per character,
/// carrying the character as its text (the way a text field receives typing),
/// with a layout pass after each.
pub fn type_text(tree: &mut WidgetTree, text: &str) {
    for ch in text.chars() {
        tree.dispatch_event(WidgetEvent::KeyDown {
            key: Key::Character(ch),
            modifiers: Modifiers::NONE,
            text: Some(ch.to_string()),
        });
        layout(tree);
    }
}

/// Sends one mouse event, `phase` of `button` at `at` (window coordinates),
/// stamped `ms` milliseconds on the tree's input clock, then lays the tree
/// out. This is the real pointer path: hit testing, capture, press-to-focus,
/// gesture recognition. Space successive clicks a few hundred milliseconds
/// apart when they must not count as a double click.
pub fn pointer_event(
    tree: &mut WidgetTree,
    phase: PointerPhase,
    at: Point,
    button: PointerButton,
    ms: u64,
) {
    tree.dispatch_pointer(
        PointerSample::mouse(phase, at, EventTime::from_millis(ms)).with_button(button),
    );
    layout(tree);
}

/// A primary-button tap at `at`, hit-tested and recognised as a tap, so it
/// reaches `on_tap` handlers and the stock widgets' activation (`Button`,
/// `Pressable`).
///
/// [`pointer_click`] does not do that: its pointer samples reach raw pointer
/// handlers (`on_pointer_event`), but no tap is recognised from them in a
/// headless tree. This goes through the legacy pointer events, as Teksilo's own
/// tests and Skribisto's do.
pub fn tap_at(tree: &mut WidgetTree, at: Point) {
    tree.dispatch_event(WidgetEvent::pointer_down(
        at,
        PointerButton::Primary,
        Modifiers::NONE,
    ));
    tree.dispatch_event(WidgetEvent::pointer_up(
        at,
        PointerButton::Primary,
        Modifiers::NONE,
    ));
    layout(tree);
}

/// A mouse click with `button` at `at`: down at `ms`, up 5 ms later (see
/// [`pointer_event`]). It drives raw pointer handlers; to activate a tap
/// handler use [`tap_at`].
pub fn pointer_click(tree: &mut WidgetTree, at: Point, button: PointerButton, ms: u64) {
    pointer_event(tree, PointerPhase::Down, at, button, ms);
    pointer_event(tree, PointerPhase::Up, at, button, ms + 5);
}

/// The accessibility node `id` publishes, as the platform adapter receives
/// it. Unlike `WidgetTree::accessibility_node`, it carries the node's value,
/// numeric value, range, step and custom actions.
pub fn a11y_node(tree: &mut WidgetTree, id: WidgetId) -> Node {
    let wanted = widget_id_to_node_id(id);
    match tree
        .sync_accessibility()
        .nodes
        .into_iter()
        .find(|(node, _)| *node == wanted)
    {
        Some((_, node)) => node,
        None => panic!("{id:?} publishes no accessibility node"),
    }
}

/// Runs `f` with a real `EventContext`, as a button press would: a button
/// wired to it is added to `tree` and clicked, with a layout pass on either
/// side. For view-model methods that need the context but have no widget of
/// their own to test through.
pub fn press(tree: &mut WidgetTree, f: impl Fn(&mut EventContext) + 'static) {
    let button = tree.add(Button::new(lit!("press")).on_activate_fn(f));
    layout(tree);
    access_click(tree, button);
}

/// The titles of the dialogs presented since the last drain, taking them off
/// the queue (a headless tree has no window manager to present them).
pub fn drain_dialog_titles(tree: &mut WidgetTree) -> Vec<String> {
    tree.drain_pending_modal_requests()
        .into_iter()
        .filter_map(|queued| queued.request.title)
        .collect()
}

/// The first node at or under `root` whose type name ends with `suffix`
/// (depth first). Type names come from `std::any::type_name`, so match the
/// last path segment.
pub fn first_of_type(tree: &WidgetTree, root: WidgetId, suffix: &str) -> Option<WidgetId> {
    if tree
        .widget_type_name(root)
        .is_some_and(|name| name.ends_with(suffix))
    {
        return Some(root);
    }
    tree.children(root)
        .into_iter()
        .find_map(|child| first_of_type(tree, child, suffix))
}

/// Every node at or under `root` that the accessibility tree names `name`.
///
/// It reads `WidgetTree::accessibility_node`, which panics on a parked widget
/// (a page of a `Switcher` not shown, a `visible_when(false)` subtree): use it
/// on trees without parked pages, or walk `tree.sync_accessibility().nodes`
/// as `audio_center::shell::testing::find_by_name` does.
pub fn find_by_name(tree: &WidgetTree, root: WidgetId, name: &str) -> Vec<WidgetId> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(id) = stack.pop() {
        if tree.accessibility_node(id).name() == Some(name) {
            found.push(id);
        }
        stack.extend(tree.children(id));
    }
    found
}

/// Runs `f` with the shipped `.ftl` files of `locale` installed as the only
/// locale, so `tr!(…).resolve_now()` returns what a user running in it reads.
/// `tr!` checks keys against `en-US` only; resolving in `es-ES` is how a
/// missing or misspelt Spanish message shows up.
pub fn with_shipped_messages(locale: &str, f: impl FnOnce()) {
    use teksilo::i18n::config::I18nConfig;
    use teksilo::i18n::manager::I18nManager;
    use teksilo::i18n::thread_local::{clear, install};

    let Some(&(tag, files)) = super::startup::app_locales()
        .iter()
        .find(|(tag, _)| *tag == locale)
    else {
        panic!("{locale} is not a locale the app ships");
    };
    let id: teksilo::i18n::LanguageIdentifier = match tag.parse() {
        Ok(id) => id,
        Err(e) => panic!("{tag} is not a locale: {e:?}"),
    };
    clear();
    let config = I18nConfig::new()
        .source_locale(id.clone())
        .supported_locales([id.clone()])
        .compile_in(&[(tag, files)])
        .auto_detect_os_locale(false)
        .fallback_locale(id);
    install(I18nManager::from_config(&config));
    f();
    clear();
}
