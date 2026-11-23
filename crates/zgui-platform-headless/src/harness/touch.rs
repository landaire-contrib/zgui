//! Scripted contacts on a touch screen.

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{AppHandler, SurfaceEvent};
use zgui_vocab::{
    Modifiers, PointerAction, PointerButton, PointerEvent, PointerId, PointerKind, Timestamp,
};

use crate::harness::Harness;

impl<A: AppHandler> Harness<A> {
    /// Delivers one event of the touch contact `finger` to the first surface, with no modifiers
    /// held.
    ///
    /// A contact is a [`PointerAction::Pressed`] event, any number of [`PointerAction::Moved`]
    /// events and one [`PointerAction::Released`] or [`PointerAction::Cancelled`] event, as a touch
    /// screen reports it. `at` is the stamp of the event, which a gesture measures speed with.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, as [`Harness::deliver_to_first`] does.
    pub fn touch(
        &mut self,
        action: PointerAction,
        finger: PointerId,
        position: Point<CssPx, Css>,
        at: Timestamp,
    ) {
        self.deliver_to_first(SurfaceEvent::Pointer {
            action,
            event: PointerEvent {
                id: finger,
                kind: PointerKind::Touch,
                primary: true,
                position,
                button: Some(PointerButton::Primary),
                pressure: None,
            },
            modifiers: Modifiers::NONE,
            timestamp: at,
        });
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{CssPx, Point};
    use zgui_platform::{
        AppHandler, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
    };
    use zgui_vocab::{PointerAction, PointerId, PointerKind, Timestamp};

    use crate::harness::Harness;

    /// An application that records each touch event it receives.
    #[derive(Default)]
    struct Recorder {
        /// The action, kind and stamp of each pointer event, in arrival order.
        touches: Vec<(PointerAction, PointerKind, Timestamp)>,
    }

    impl AppHandler for Recorder {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            cx.create_surface(&SurfaceAttributes::new("touch target"))
                .expect("headless surfaces are always creatable");
        }

        fn surface_event(&mut self, _cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
            if let SurfaceEvent::Pointer {
                action,
                event,
                timestamp,
                ..
            } = event
            {
                self.touches.push((action, event.kind, timestamp));
            }
        }

        fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}
    }

    #[test]
    fn each_event_reaches_the_first_surface_as_a_touch_with_its_stamp() {
        let mut harness = Harness::new(Recorder::default());
        let finger = PointerId::new(4);
        let stamp = |millis: u64| Timestamp::from_origin(Duration::from_millis(millis));
        let at = Point::new(CssPx(5.0), CssPx(6.0));
        harness.touch(PointerAction::Pressed, finger, at, stamp(0));
        harness.touch(PointerAction::Moved, finger, at, stamp(8));
        harness.touch(PointerAction::Released, finger, at, stamp(16));

        assert_eq!(
            harness.app().touches,
            [
                (PointerAction::Pressed, PointerKind::Touch, stamp(0)),
                (PointerAction::Moved, PointerKind::Touch, stamp(8)),
                (PointerAction::Released, PointerKind::Touch, stamp(16)),
            ]
        );
    }
}
