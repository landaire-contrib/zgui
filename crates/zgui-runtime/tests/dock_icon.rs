//! The application's icon in the Dock, shown and hidden from anywhere in the application.

mod support;

use zgui_platform::{AppEvent, MenuId, PlatformCapabilities, PlatformCx, WakeReason};
use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::{ExitPolicy, Runtime};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// The stylesheet the window is styled by.
const SHEET: &str = "root { display: block } column { display: block }";

/// A view that builds one empty column, for a window whose contents are not the point.
fn empty(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::column().into_view().build(cx))
}

/// An application that hides its Dock icon on the menu action `hide` and shows it on `show`.
fn application(dock_icon: bool) -> Harness<Runtime> {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.dock_icon = dock_icon;
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .with_exit_policy(ExitPolicy::Explicit)
        .with_context(|| {
            let windows = zgui_runtime::use_windows();
            let guard = zgui_runtime::desktop::on_menu(move |id| match id.as_str() {
                "hide" => windows.set_dock_icon_visible(false),
                "show" => windows.set_dock_icon_visible(true),
                _ => {}
            });
            core::mem::forget(guard);
        })
        .into_handler(empty)
        .expect("the reactive runtime installs");
    Harness::over(Headless::new().with_capabilities(capabilities), handler)
}

/// Picks the menu action `id`, as the desktop would.
fn pick(harness: &mut Harness<Runtime>, id: &str) {
    harness
        .platform()
        .waker()
        .wake(WakeReason::App(AppEvent::Menu(MenuId::new(id))));
    harness.settle(4);
}

#[test]
fn the_dock_icon_follows_what_the_application_asks_for_in_order() {
    let mut harness = application(true);
    harness.settle(4);
    assert!(harness.platform().dock_icon_log().is_empty());

    pick(&mut harness, "hide");
    pick(&mut harness, "show");
    assert_eq!(harness.platform().dock_icon_log(), [false, true]);
}

#[test]
fn a_desktop_with_no_dock_icon_is_not_asked_to_hide_one() {
    let mut harness = application(false);
    harness.settle(4);
    pick(&mut harness, "hide");
    assert!(harness.platform().dock_icon_log().is_empty());
}
