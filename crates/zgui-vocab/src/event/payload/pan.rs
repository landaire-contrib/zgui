//! A pan: one contact that travelled past the slop and that an element took from the scrolling.

use zgui_geom::{Css, CssPx, Point, Size};

use crate::event::payload::pinch::GesturePhase;
use crate::event::payload::pointer::PointerKind;

/// How fast a contact travels, in CSS pixels per second.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Velocity {
    /// To the right.
    pub x: f32,
    /// Downwards.
    pub y: f32,
}

/// An edge of the window.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScreenEdge {
    /// The left edge, which is the leading edge in left-to-right text.
    Left,
    /// The right edge.
    Right,
    /// The top edge.
    Top,
    /// The bottom edge.
    Bottom,
}

impl ScreenEdge {
    /// How near to an edge a contact goes down to start at that edge, in CSS pixels.
    pub const BAND: CssPx = CssPx(20.0);

    /// The edge of a window `size` large that `at` is within [`ScreenEdge::BAND`] of, if any.
    ///
    /// In a corner the nearer edge counts, and the left or right edge on a tie.
    pub fn near(at: Point<CssPx, Css>, size: Size<CssPx, Css>) -> Option<Self> {
        let distances = [
            (Self::Left, at.x.0),
            (Self::Right, size.width.0 - at.x.0),
            (Self::Top, at.y.0),
            (Self::Bottom, size.height.0 - at.y.0),
        ];
        let mut nearest: Option<(Self, f32)> = None;
        for (edge, distance) in distances {
            if !(0.0..=Self::BAND.0).contains(&distance) {
                continue;
            }
            if nearest.is_none_or(|(_, best)| distance < best) {
                nearest = Some((edge, distance));
            }
        }

        nearest.map(|(edge, _)| edge)
    }
}

/// What a pan event carries.
///
/// All positions are in CSS pixels from the window's top-left corner. Each event carries the
/// position where the contact went down, so a listener reads the full travel from any one event.
///
/// ```
/// use zgui_geom::{CssPx, Point, Size};
/// use zgui_vocab::{GesturePhase, PanEvent, PointerKind, ScreenEdge, Velocity};
///
/// let moved = PanEvent {
///     phase: GesturePhase::Moved,
///     origin: Point::new(CssPx(10.0), CssPx(20.0)),
///     position: Point::new(CssPx(70.0), CssPx(25.0)),
///     velocity: Velocity { x: 600.0, y: 0.0 },
///     kind: PointerKind::Touch,
///     edge: Some(ScreenEdge::Left),
/// };
/// assert_eq!(moved.translation(), Size::new(CssPx(60.0), CssPx(5.0)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanEvent {
    /// Where the event sits in the pan.
    ///
    /// A pan that the window takes away, for example because a pointer capture arrives, ends with
    /// [`GesturePhase::Cancelled`].
    pub phase: GesturePhase,
    /// Where the contact went down.
    pub origin: Point<CssPx, Css>,
    /// Where the contact is now.
    pub position: Point<CssPx, Css>,
    /// How fast the contact travels now. On [`GesturePhase::Ended`], this is the speed of the
    /// lift, which tells a flick from a slow release.
    pub velocity: Velocity,
    /// Which kind of pointer pans.
    pub kind: PointerKind,
    /// The edge of the window that [`PanEvent::origin`] is within [`ScreenEdge::BAND`] of, if
    /// any. A pan from the left edge is how a person goes back on iOS.
    pub edge: Option<ScreenEdge>,
}

impl PanEvent {
    /// How far the contact travelled from where it went down.
    pub fn translation(&self) -> Size<CssPx, Css> {
        Size::new(
            CssPx(self.position.x.0 - self.origin.x.0),
            CssPx(self.position.y.0 - self.origin.y.0),
        )
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, Point, Size};

    use super::ScreenEdge;

    #[test]
    fn a_contact_near_an_edge_starts_at_that_edge() {
        let window = Size::new(CssPx(400.0), CssPx(800.0));
        let at = |x: f32, y: f32| ScreenEdge::near(Point::new(CssPx(x), CssPx(y)), window);
        assert_eq!(at(12.0, 400.0), Some(ScreenEdge::Left));
        assert_eq!(at(20.0, 400.0), Some(ScreenEdge::Left));
        assert_eq!(at(21.0, 400.0), None);
        assert_eq!(at(395.0, 400.0), Some(ScreenEdge::Right));
        assert_eq!(at(200.0, 5.0), Some(ScreenEdge::Top));
        assert_eq!(at(200.0, 790.0), Some(ScreenEdge::Bottom));
        assert_eq!(at(10.0, 4.0), Some(ScreenEdge::Top), "the nearer edge");
        assert_eq!(at(8.0, 8.0), Some(ScreenEdge::Left), "left on a tie");
    }
}
