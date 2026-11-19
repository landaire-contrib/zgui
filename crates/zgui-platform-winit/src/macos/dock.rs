//! Whether the application shows in the Dock and the application switcher.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};

/// Shows the application in the Dock and the application switcher, or takes it out of both.
///
/// A hidden icon makes the application an accessory: it keeps its windows and its menu bar
/// extras, and has no menu bar.
pub(crate) fn set_visible(visible: bool) {
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    let application = NSApplication::sharedApplication(main);
    let policy = if visible {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    if application.activationPolicy() == policy {
        return;
    }
    if !application.setActivationPolicy(policy) {
        tracing::warn!(target: "zgui::platform", ?policy, "the activation policy was refused");
    }
}
