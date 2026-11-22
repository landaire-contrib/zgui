//! The UIKit values winit does not expose.

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_ui_kit::UIView;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use zgui_geom::{DevicePx, Edges};

/// The safe area insets of the view winit draws `window` into, in physical pixels.
///
/// `None` off the main thread, or when the window has no UIKit view.
pub(crate) fn safe_area(window: &winit::window::Window, scale: f64) -> Option<Edges<DevicePx>> {
    MainThreadMarker::new()?;
    let view = ui_view(window)?;
    let insets = view.safeAreaInsets();
    let physical = |points: f64| DevicePx((points * scale) as f32);

    Some(Edges::new(
        physical(insets.top),
        physical(insets.right),
        physical(insets.bottom),
        physical(insets.left),
    ))
}

/// The view winit draws `window` into.
fn ui_view(window: &winit::window::Window) -> Option<Retained<UIView>> {
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return None;
    };
    // SAFETY: winit hands out its own view, which lives as long as the window, and the window is
    // borrowed for this call. Retaining it keeps it alive past the borrow.
    unsafe { Retained::retain(handle.ui_view.as_ptr().cast()) }
}
