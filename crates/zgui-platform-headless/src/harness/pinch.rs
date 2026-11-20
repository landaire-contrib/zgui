//! Scripted trackpad pinches.

use zgui_platform::{AppHandler, SurfaceEvent};
use zgui_vocab::{Modifiers, PinchEvent, Timestamp};

use crate::harness::Harness;

impl<A: AppHandler> Harness<A> {
    /// Delivers one stage of a pinch to the first surface, with no modifiers held.
    ///
    /// A gesture is a [`GesturePhase::Started`](zgui_vocab::GesturePhase::Started) event, any
    /// number of [`GesturePhase::Moved`](zgui_vocab::GesturePhase::Moved) events and one
    /// [`GesturePhase::Ended`](zgui_vocab::GesturePhase::Ended) or
    /// [`GesturePhase::Cancelled`](zgui_vocab::GesturePhase::Cancelled) event, as a trackpad
    /// reports it.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, as [`Harness::deliver_to_first`] does.
    pub fn pinch(&mut self, event: PinchEvent) {
        self.deliver_to_first(SurfaceEvent::Pinch {
            event,
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, Point};
    use zgui_platform::{
        AppHandler, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
    };
    use zgui_vocab::{GesturePhase, PinchEvent};

    use crate::harness::Harness;

    /// An application that records each pinch it receives.
    #[derive(Default)]
    struct Recorder {
        /// The pinches in arrival order.
        pinches: Vec<PinchEvent>,
    }

    impl AppHandler for Recorder {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            cx.create_surface(&SurfaceAttributes::new("pinch target"))
                .expect("headless surfaces are always creatable");
        }

        fn surface_event(&mut self, _cx: &dyn PlatformCx, _id: SurfaceId, event: SurfaceEvent) {
            if let SurfaceEvent::Pinch { event, .. } = event {
                self.pinches.push(event);
            }
        }

        fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}
    }

    #[test]
    fn each_stage_reaches_the_first_surface_in_order() {
        let mut harness = Harness::new(Recorder::default());
        let at = Point::new(CssPx(5.0), CssPx(6.0));
        let stages = [
            (0.0, GesturePhase::Started),
            (0.1, GesturePhase::Moved),
            (0.0, GesturePhase::Ended),
        ]
        .map(|(scale_delta, phase)| PinchEvent {
            scale_delta,
            phase,
            position: at,
        });
        for stage in stages {
            harness.pinch(stage);
        }

        assert_eq!(harness.app().pinches, stages);
    }
}
