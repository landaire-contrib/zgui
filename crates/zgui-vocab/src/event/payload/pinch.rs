//! A pinch on a trackpad or a touch screen.

use zgui_geom::{Css, CssPx, Point};

/// Where a continuous gesture sits in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum GesturePhase {
    /// The gesture has begun.
    Started,
    /// The gesture is continuing.
    Moved,
    /// The user ended the gesture.
    Ended,
    /// The platform stopped the gesture before the user ended it.
    Cancelled,
}

impl GesturePhase {
    /// Whether this is the last event of the gesture.
    pub const fn is_last(self) -> bool {
        matches!(self, Self::Ended | Self::Cancelled)
    }
}

/// What a pinch event carries.
///
/// Each event of a gesture carries the change since the previous event. Multiply the current
/// scale by [`PinchEvent::factor`] for each event to follow the fingers.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_vocab::{GesturePhase, PinchEvent};
///
/// let apart = PinchEvent {
///     scale_delta: 0.25,
///     phase: GesturePhase::Moved,
///     position: Point::new(CssPx(10.0), CssPx(20.0)),
/// };
/// assert_eq!(apart.factor(), 1.25);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinchEvent {
    /// The change in magnification since the previous event of the gesture.
    ///
    /// Positive when the fingers move apart and the content should grow, negative when they move
    /// together.
    pub scale_delta: f32,
    /// Where the event sits in the gesture.
    pub phase: GesturePhase,
    /// Where the pointer was, in CSS pixels from the window's top-left corner.
    pub position: Point<CssPx, Css>,
}

impl PinchEvent {
    /// The factor to multiply the current scale by.
    pub fn factor(&self) -> f32 {
        1.0 + self.scale_delta
    }
}
