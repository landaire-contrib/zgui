//! Whether a window is zoomed, asked without changing its frame.

use objc2::MainThreadMarker;
use objc2_app_kit::NSWindowStyleMask;

use crate::macos::window::ns_window;

/// Whether `window` is zoomed, or `None` off the main thread or when it has no AppKit window.
///
/// `isZoomed` answers only for a titled, resizable window. winit makes any other window titled and
/// resizable for the question by replacing its style mask, which drops a full-size content view:
/// the frame grows by a title bar while the question is asked, and AppKit pushes a window near the
/// menu bar down by as much. Adding the styles to the window's own instead keeps the content view
/// as large as the frame, so the window stays where it is. winit's `set_maximized` still asks its
/// own way first.
pub(crate) fn is_zoomed(window: &winit::window::Window) -> Option<bool> {
    MainThreadMarker::new()?;
    let ns_window = ns_window(window)?;
    let mask = ns_window.styleMask();
    if mask.contains(NSWindowStyleMask::Titled | NSWindowStyleMask::Resizable) {
        return Some(ns_window.isZoomed());
    }

    // A borderless window gains a title bar for the question, and a full-size content view keeps
    // its content as large as its frame.
    let asked = if mask.contains(NSWindowStyleMask::Titled) {
        mask | NSWindowStyleMask::Resizable
    } else {
        mask | NSWindowStyleMask::Titled
            | NSWindowStyleMask::Resizable
            | NSWindowStyleMask::FullSizeContentView
    };

    // A new style mask resets the first responder, and keys go nowhere until it is put back.
    let responder = ns_window.firstResponder();
    ns_window.setStyleMask(asked);
    let zoomed = ns_window.isZoomed();
    ns_window.setStyleMask(mask);
    ns_window.makeFirstResponder(responder.as_deref());

    Some(zoomed)
}
