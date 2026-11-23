//! The UIKit values winit does not expose.

pub(crate) mod text_view;

use core::ptr::NonNull;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol, NSValue};
use objc2_ui_kit::{
    NSValueUIGeometryExtensions, UIKeyboardFrameEndUserInfoKey,
    UIKeyboardWillChangeFrameNotification, UIKeyboardWillHideNotification, UIView,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use zgui_geom::{Device, DevicePx, Edges, Point, Rect, Size};
use zgui_platform::{Surface, SurfaceId};

use crate::surface::WinitSurface;

/// A registration with the notification center.
type Observer = Retained<ProtocolObject<dyn NSObjectProtocol>>;

thread_local! {
    /// The keyboard observers of each surface, removed with it.
    static OBSERVERS: RefCell<HashMap<SurfaceId, [Observer; 2]>> = RefCell::new(HashMap::new());
}

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

/// Gives `surface` the area of each frame the on-screen keyboard moves to.
///
/// UIKit posts the frame as the keyboard starts to move, so the area is the one at the end of the
/// animation.
pub(crate) fn observe_keyboard(surface: &Arc<WinitSurface>) {
    let center = NSNotificationCenter::defaultCenter();
    let changed = keyboard_block(Arc::downgrade(surface), false);
    let hidden = keyboard_block(Arc::downgrade(surface), true);

    // SAFETY: the names are UIKit's own, and UIKit posts them on the main thread, which is where
    // each block reads the view.
    let observers = unsafe {
        [
            center.addObserverForName_object_queue_usingBlock(
                Some(UIKeyboardWillChangeFrameNotification),
                None,
                None,
                &changed,
            ),
            center.addObserverForName_object_queue_usingBlock(
                Some(UIKeyboardWillHideNotification),
                None,
                None,
                &hidden,
            ),
        ]
    };
    OBSERVERS.with(|held| {
        held.borrow_mut()
            .insert(Surface::id(surface.as_ref()), observers)
    });
}

/// Stops giving the keyboard area to `surface`.
pub(crate) fn forget_keyboard(surface: SurfaceId) {
    let Some(observers) = OBSERVERS.with(|held| held.borrow_mut().remove(&surface)) else {
        return;
    };

    let center = NSNotificationCenter::defaultCenter();
    for observer in observers {
        // SAFETY: the observer is one the center handed out.
        unsafe { center.removeObserver(AsRef::<AnyObject>::as_ref(&*observer)) };
    }
}

/// A block that gives `surface` the keyboard area a notification states.
///
/// A block made with `hides` gives no area.
fn keyboard_block(
    surface: Weak<WinitSurface>,
    hides: bool,
) -> RcBlock<dyn Fn(NonNull<NSNotification>)> {
    RcBlock::new(move |notification: NonNull<NSNotification>| {
        let Some(surface) = surface.upgrade() else {
            return;
        };

        let area = if hides {
            None
        } else {
            // SAFETY: the center hands the block a live notification for the length of the call.
            keyboard_area(
                surface.window(),
                unsafe { notification.as_ref() },
                surface.scale(),
            )
        };
        surface.set_virtual_keyboard(area);
    })
}

/// The part of the view winit draws `window` into that the keyboard frame of `notification`
/// covers, in physical pixels.
///
/// `None` when the frame does not overlap the view, off the main thread, or when the notification
/// states no frame.
fn keyboard_area(
    window: &winit::window::Window,
    notification: &NSNotification,
    scale: f64,
) -> Option<Rect<DevicePx, Device>> {
    MainThreadMarker::new()?;
    let view = ui_view(window)?;
    let info = notification.userInfo()?;
    // SAFETY: the key is UIKit's own constant.
    let key: &AnyObject = AsRef::<AnyObject>::as_ref(unsafe { UIKeyboardFrameEndUserInfoKey });
    let value = info.objectForKey(key)?.downcast::<NSValue>().ok()?;
    // SAFETY: UIKit states the end frame as a rectangle value.
    let on_screen = unsafe { value.CGRectValue() };

    // The frame is in screen coordinates. A window of a scene that does not fill the screen has its
    // own origin.
    let on_window = view.window()?.convertRect_fromWindow(on_screen, None);
    let on_view = view.convertRect_fromView(on_window, None);
    let bounds = view.bounds();
    let physical = |points: f64| DevicePx((points * scale) as f32);
    let keyboard = Rect::new(
        Point::new(
            physical(on_view.origin.x - bounds.origin.x),
            physical(on_view.origin.y - bounds.origin.y),
        ),
        Size::new(physical(on_view.size.width), physical(on_view.size.height)),
    );
    let drawable = Rect::new(
        Point::new(DevicePx(0.0), DevicePx(0.0)),
        Size::new(physical(bounds.size.width), physical(bounds.size.height)),
    );

    keyboard
        .intersection(drawable)
        .filter(|area| !area.is_empty())
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
