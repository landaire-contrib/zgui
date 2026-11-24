//! Haptics, played from anywhere in the application.

mod support;

use zgui_platform::{
    AppEvent, Haptic, ImpactWeight, MenuId, Outcome, PlatformCapabilities, PlatformCx, WakeReason,
};
use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::{ExitPolicy, Runtime};
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// The stylesheet the window is styled by.
const SHEET: &str = "root { display: block } column { display: block }";

/// Every haptic there is.
const EVERY: [Haptic; 7] = [
    Haptic::Impact(ImpactWeight::Light),
    Haptic::Impact(ImpactWeight::Medium),
    Haptic::Impact(ImpactWeight::Heavy),
    Haptic::Selection,
    Haptic::Notification(Outcome::Success),
    Haptic::Notification(Outcome::Warning),
    Haptic::Notification(Outcome::Error),
];

/// A view that builds one empty column, for a window whose contents are not the point.
fn empty(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::column().into_view().build(cx))
}

/// An application that plays every haptic on the menu action `play`.
fn application(haptics: bool) -> Harness<Runtime> {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.haptics = haptics;
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .with_exit_policy(ExitPolicy::Explicit)
        .with_context(|| {
            let windows = zgui_runtime::use_windows();
            let guard = zgui_runtime::desktop::on_menu(move |id| {
                if id.as_str() == "play" {
                    for haptic in EVERY {
                        windows.play_haptic(haptic);
                    }
                }
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
fn each_haptic_is_played_in_the_order_the_application_asked() {
    let mut harness = application(true);
    harness.settle(4);
    assert!(harness.platform().haptic_log().is_empty());

    pick(&mut harness, "play");
    assert_eq!(harness.platform().haptic_log(), EVERY);
}

#[test]
fn a_device_with_no_haptics_is_not_asked_to_play_one() {
    let mut harness = application(false);
    harness.settle(4);
    pick(&mut harness, "play");
    assert!(harness.platform().haptic_log().is_empty());
}
