//! Who draws the frame of each window, as the window handle reports it.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_platform::{DecorationSource, PlatformCapabilities, SurfaceAttributes, SurfaceEvent};
use zgui_platform_headless::{Harness, Headless};
use zgui_reactive::prelude::Get;
use zgui_runtime::{Runtime, WindowHandle, WindowSpec};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// The stylesheet every window here is styled by.
const SHEET: &str = "root { display: block } column { display: block }";

/// A view that builds one empty column.
fn empty(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::column().into_view().build(cx))
}

/// An application over a platform that says `source` draws window frames.
fn app_on(source: DecorationSource) -> Harness<Runtime> {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.decorations = source;
    let mut harness = support::app_with_text_on(
        Headless::new().with_capabilities(capabilities),
        SHEET,
        empty,
    );
    harness.settle(8);
    harness
}

/// The handle of window `index`.
fn handle(harness: &Harness<Runtime>, index: usize) -> WindowHandle {
    harness.app().windows()[index].handle().clone()
}

/// Tells window `index` that `source` now draws its frame, as a compositor does.
fn answer(harness: &mut Harness<Runtime>, index: usize, source: DecorationSource) {
    let surface = harness.app().windows()[index].surface().id();
    harness.deliver(surface, SurfaceEvent::DecorationSourceChanged(source));
    harness.settle(8);
}

/// An effect that counts its own runs, kept for the life of the test.
fn counting(signal: zgui_reactive::Signal<DecorationSource>) -> Rc<Cell<u32>> {
    let runs = Rc::new(Cell::new(0));
    let seen = Rc::clone(&runs);
    core::mem::forget(zgui_reactive::RenderEffect::new(move |_| {
        let _ = signal.get();
        seen.set(seen.get() + 1);
    }));
    runs
}

#[test]
fn a_window_starts_with_the_answer_of_the_platform() {
    let mut drawn_by_platform = app_on(DecorationSource::Platform);
    assert_eq!(
        handle(&drawn_by_platform, 0).decoration_source().get(),
        DecorationSource::Platform
    );
    drawn_by_platform.shut_down();

    let mut drawn_by_application = app_on(DecorationSource::Application);
    assert_eq!(
        handle(&drawn_by_application, 0).decoration_source().get(),
        DecorationSource::Application
    );
    drawn_by_application.shut_down();
}

#[test]
fn a_new_answer_from_the_surface_changes_the_signal() {
    let mut harness = app_on(DecorationSource::Platform);
    let window = handle(&harness, 0);
    let runs = counting(window.decoration_source());
    harness.settle(8);
    let before = runs.get();

    answer(&mut harness, 0, DecorationSource::Application);
    assert_eq!(
        window.decoration_source().get(),
        DecorationSource::Application
    );
    assert_eq!(runs.get(), before + 1, "the change woke its readers once");

    // A compositor that repeats its answer wakes nobody.
    answer(&mut harness, 0, DecorationSource::Application);
    assert_eq!(runs.get(), before + 1, "a repeated answer woke its readers");

    answer(&mut harness, 0, DecorationSource::Platform);
    assert_eq!(window.decoration_source().get(), DecorationSource::Platform);
    assert_eq!(
        runs.get(),
        before + 2,
        "the change back woke its readers once"
    );

    harness.shut_down();
}

#[test]
fn each_window_has_its_own_answer() {
    let mut harness = app_on(DecorationSource::Platform);
    let mut options = zgui_runtime::WindowContent::default();
    options.window_stylesheets.push(SHEET.to_owned());
    harness.app().window_commands().open(WindowSpec::new(
        SurfaceAttributes::new("second"),
        options,
        Box::new(empty),
    ));
    harness.settle(8);
    assert_eq!(
        harness.app().windows().len(),
        2,
        "the second window is open"
    );

    answer(&mut harness, 1, DecorationSource::Application);

    assert_eq!(
        handle(&harness, 0).decoration_source().get(),
        DecorationSource::Platform
    );
    assert_eq!(
        handle(&harness, 1).decoration_source().get(),
        DecorationSource::Application
    );

    harness.shut_down();
}
