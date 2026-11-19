//! The presses of a status item that reports them, and the menu it opens on a secondary click.

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send, sel};
use objc2_app_kit::{
    NSApplication, NSEvent, NSEventMask, NSEventModifierFlags, NSEventType, NSMenu,
    NSStatusBarButton, NSStatusItem,
};
use objc2_core_graphics::{CGDisplayBounds, CGMainDisplayID};
use objc2_foundation::{NSObject, NSRect};
use zgui_geom::{Css, CssPx, Point, Rect, Size};
use zgui_platform::{AppEvent, StatusAnchor, WakeReason, Waker};

/// What the target of a status item's button answers a press with.
struct Target {
    /// The item whose button sends the presses.
    status: Retained<NSStatusItem>,
    /// The menu a secondary click opens, when the item has one.
    menu: Option<Retained<NSMenu>>,
    /// Where a primary click is reported.
    waker: Arc<dyn Waker>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `PressTarget` does not implement
    // `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZguiStatusPressTarget"]
    #[ivars = Target]
    struct PressTarget;

    impl PressTarget {
        /// The action of the item's button, sent on a left or a right mouse up.
        #[unsafe(method(pressed:))]
        fn pressed(&self, _sender: Option<&AnyObject>) {
            // The application may replace the item while its menu is open, which releases this
            // target. The button does not retain its target, so this call does.
            let this = self.retain();
            let main = self.mtm();

            let secondary = NSApplication::sharedApplication(main)
                .currentEvent()
                .is_some_and(|event| is_secondary(&event));
            if secondary {
                this.open_menu(main);
            } else {
                this.report(main);
            }
        }
    }
);

impl PressTarget {
    /// A target that answers the presses of `status` as `target` says.
    fn new(main: MainThreadMarker, target: Target) -> Retained<Self> {
        let this = Self::alloc(main).set_ivars(target);
        // SAFETY: `init` is NSObject's designated initializer, and the ivars are set.
        unsafe { msg_send![super(this), init] }
    }

    /// Reports a primary click with where the item is.
    fn report(&self, main: MainThreadMarker) {
        let target = self.ivars();
        let Some(anchor) = target
            .status
            .button(main)
            .and_then(|button| anchor(&button))
        else {
            return;
        };
        target
            .waker
            .wake(WakeReason::App(AppEvent::StatusItemPressed(anchor)));
    }

    /// Opens the item's menu under it, and returns once the menu closes.
    fn open_menu(&self, main: MainThreadMarker) {
        let target = self.ivars();
        let Some(menu) = &target.menu else {
            return;
        };
        let Some(button) = target.status.button(main) else {
            return;
        };

        // A button whose item holds a menu opens the menu on a click instead of sending its
        // action. The menu is held only for this click, so the next primary click reports again.
        target.status.setMenu(Some(menu));
        // SAFETY: the button belongs to the item, which the target retains for this call.
        unsafe { button.performClick(None) };

        // The application may have given the item another menu while this one was open.
        let held = target.status.menu(main);
        if held.is_some_and(|held| Retained::as_ptr(&held) == Retained::as_ptr(menu)) {
            target.status.setMenu(None);
        }
    }
}

/// What reports the presses of a status item.
///
/// Dropping it stops the item's button from sending them, unless a newer target took the button
/// over first.
pub(crate) struct Presses {
    /// The target of the button, which the button does not retain.
    target: Retained<PressTarget>,
}

impl Drop for Presses {
    fn drop(&mut self) {
        let Some(main) = MainThreadMarker::new() else {
            return;
        };
        let Some(button) = self.target.ivars().status.button(main) else {
            return;
        };
        let ours = button.target().is_some_and(|held| {
            Retained::as_ptr(&held).cast::<()>() == Retained::as_ptr(&self.target).cast::<()>()
        });
        if !ours {
            return;
        }
        // SAFETY: an absent target and an absent action send nothing.
        unsafe {
            button.setTarget(None);
            button.setAction(None);
        }
    }
}

/// Sends the presses of `status` to `waker`, and opens `menu` on a secondary click, for as long
/// as the returned value is kept.
///
/// The item holds no menu of its own afterwards, so its button sends every click.
pub(crate) fn report(
    status: &Retained<NSStatusItem>,
    menu: Option<Retained<NSMenu>>,
    waker: Arc<dyn Waker>,
    main: MainThreadMarker,
) -> Presses {
    let target = PressTarget::new(
        main,
        Target {
            status: Retained::clone(status),
            menu,
            waker,
        },
    );
    status.setMenu(None);
    if let Some(button) = status.button(main) {
        // SAFETY: the target answers `pressed:` with the signature a control action has, and the
        // returned value clears the button's target before it lets the target go.
        unsafe {
            button.setTarget(Some(&target));
            button.setAction(Some(sel!(pressed:)));
        }
        button.sendActionOn(NSEventMask::LeftMouseUp | NSEventMask::RightMouseUp);
    }
    Presses { target }
}

/// Whether `event` is a secondary click: a right click, or a left click with Control held.
fn is_secondary(event: &NSEvent) -> bool {
    match event.r#type() {
        NSEventType::RightMouseDown | NSEventType::RightMouseUp => true,
        NSEventType::LeftMouseDown | NSEventType::LeftMouseUp => event
            .modifierFlags()
            .contains(NSEventModifierFlags::Control),
        _ => false,
    }
}

/// Where `button` is, and the part of its screen windows may cover.
fn anchor(button: &NSStatusBarButton) -> Option<StatusAnchor> {
    let window = button.window()?;
    let screen = window.screen()?;
    let item = window.convertRectToScreen(button.convertRect_toView(button.bounds(), None));
    let height = CGDisplayBounds(CGMainDisplayID()).size.height;

    Some(StatusAnchor::new(
        flipped(item, height),
        flipped(screen.visibleFrame(), height),
    ))
}

/// `rect`, measured from the top left of a main display `height` points tall and growing down.
///
/// AppKit measures from the bottom left of the main display and grows up. winit flips a window's
/// position against the height of the main display, whichever display the window is on, so this
/// flips the same way and a window placed at the result lands on `rect`.
fn flipped(rect: NSRect, height: f64) -> Rect<CssPx, Css> {
    let top = height - rect.size.height - rect.origin.y;
    Rect::new(
        Point::new(CssPx(rect.origin.x as f32), CssPx(top as f32)),
        Size::new(
            CssPx(rect.size.width as f32),
            CssPx(rect.size.height as f32),
        ),
    )
}

#[cfg(test)]
mod tests {
    use objc2_foundation::{NSPoint, NSSize};

    use super::*;

    #[test]
    fn a_rect_at_the_top_of_the_main_display_is_flipped_to_its_top_edge() {
        let item = NSRect::new(NSPoint::new(1200.0, 876.0), NSSize::new(30.0, 24.0));
        assert_eq!(
            flipped(item, 900.0),
            Rect::new(
                Point::new(CssPx(1200.0), CssPx(0.0)),
                Size::new(CssPx(30.0), CssPx(24.0)),
            )
        );
    }

    #[test]
    fn a_rect_on_a_display_above_the_main_display_is_flipped_to_a_negative_top() {
        let item = NSRect::new(NSPoint::new(-400.0, 1876.0), NSSize::new(30.0, 24.0));
        assert_eq!(flipped(item, 900.0).origin.y, CssPx(-1000.0));
    }
}
