//! The Reduce Motion setting.

use core::ptr::NonNull;
use std::cell::OnceCell;
use std::sync::Arc;

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSWorkspace, NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification};
use objc2_foundation::{NSNotification, NSObjectProtocol};
use zgui_platform::{MotionPreference, WakeReason, Waker};

thread_local! {
    /// The observer of the display options, for the life of the process.
    static OBSERVER: OnceCell<Retained<ProtocolObject<dyn NSObjectProtocol>>> =
        const { OnceCell::new() };
}

/// Tells `waker` when the user changes the accessibility display options.
///
/// Observing twice does nothing.
pub(crate) fn observe(waker: Arc<dyn Waker>) {
    OBSERVER.with(|observer| {
        observer.get_or_init(|| {
            let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                waker.wake(WakeReason::PreferencesChanged);
            });
            // AppKit posts the change on the workspace's own center.
            let center = NSWorkspace::sharedWorkspace().notificationCenter();
            // SAFETY: the name is AppKit's own, and the block only posts a wake, which any thread
            // may do.
            unsafe {
                center.addObserverForName_object_queue_usingBlock(
                    Some(NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification),
                    None,
                    None,
                    &block,
                )
            }
        });
    });
}

/// Whether the user turned Reduce Motion on.
pub(crate) fn motion_preference() -> MotionPreference {
    if NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion() {
        MotionPreference::Reduce
    } else {
        MotionPreference::NoPreference
    }
}
