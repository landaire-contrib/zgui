//! Trackpad pinches.
//!
//! `winit` reports a pinch as a change in magnification with the sign this framework uses:
//! positive when the fingers move apart. The delta crosses unchanged.

use winit::event::TouchPhase;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{GesturePhase, PinchEvent};

/// Where in the gesture this pinch sits.
pub const fn phase(phase: TouchPhase) -> GesturePhase {
    match phase {
        TouchPhase::Started => GesturePhase::Started,
        TouchPhase::Moved => GesturePhase::Moved,
        TouchPhase::Ended => GesturePhase::Ended,
        TouchPhase::Cancelled => GesturePhase::Cancelled,
    }
}

/// One pinch, at the pointer's last known place.
///
/// A pinch carries no position of its own, so the position is the one the pointer was last
/// reported at, as for a wheel turn.
pub fn event(delta: f64, phase: TouchPhase, position: Point<CssPx, Css>) -> PinchEvent {
    PinchEvent {
        scale_delta: delta as f32,
        phase: self::phase(phase),
        position,
    }
}

#[cfg(test)]
mod tests {
    use super::event;
    use winit::event::TouchPhase;
    use zgui_geom::{CssPx, Point};
    use zgui_vocab::GesturePhase;

    #[test]
    fn a_cancelled_pinch_stays_apart_from_an_ended_one() {
        let at = Point::new(CssPx(0.0), CssPx(0.0));
        assert_eq!(event(0.0, TouchPhase::Ended, at).phase, GesturePhase::Ended);
        assert_eq!(
            event(0.0, TouchPhase::Cancelled, at).phase,
            GesturePhase::Cancelled
        );
    }
}
