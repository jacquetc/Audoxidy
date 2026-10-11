//! Shared scaffolding for the tests of the models and singles.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use teksilo::core::widget_tree::WidgetTree;
use teksilo::prelude::*;

use crate::frontend::AppContext;
use crate::frontend::test_fixtures::{LibraryFixture, SeededLibrary};

/// How long a test waits for a worker's answer.
pub const TIMEOUT: Duration = Duration::from_secs(10);

/// An in-memory context seeded with `fixture`.
pub fn seeded(fixture: LibraryFixture) -> (AppContext, SeededLibrary) {
    let ctx = AppContext::in_memory().unwrap();
    let seeded = fixture.seed_on(&ctx.db).recv_blocking().unwrap();
    (ctx, seeded)
}

/// An in-memory context seeded with [`LibraryFixture::demo`].
pub fn demo() -> (AppContext, SeededLibrary) {
    seeded(LibraryFixture::demo())
}

/// An in-memory context seeded with the screenshot library.
pub fn screenshot() -> (AppContext, SeededLibrary) {
    seeded(crate::teksilo_ui::mock_data::screenshot_library())
}

/// A widget that runs `wire` in its `build`, so a model's `wire` gets a real
/// `BuildContext` (and a real frame-tick effect).
struct Mount<F: Fn(&mut BuildContext)> {
    wire: F,
}

impl<F: Fn(&mut BuildContext)> std::fmt::Debug for Mount<F> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Mount").finish()
    }
}

impl<F: Fn(&mut BuildContext) + 'static> Widget for Mount<F> {
    fn build(&mut self, ctx: &mut BuildContext) -> Vec<WidgetId> {
        (self.wire)(ctx);
        Vec::new()
    }

    fn layout_response(&self, proposal: SizeProposal, _ctx: &LayoutContext) -> LayoutResponse {
        proposal.resolve(0.0, 0.0).into()
    }
}

/// Builds a widget in `tree` that calls `wire` with its `BuildContext`.
pub fn mount(tree: &mut WidgetTree, wire: impl Fn(&mut BuildContext) + 'static) {
    tree.add_boxed(Box::new(Mount { wire }));
}

/// A wake handle like the one `BuildContext::wake_at_handle` returns.
pub fn wake() -> Rc<Cell<Option<std::time::Instant>>> {
    Rc::new(Cell::new(None))
}
