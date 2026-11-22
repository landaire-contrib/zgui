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

/// What a pan event carries.
///
/// All positions are in CSS pixels from the window's top-left corner. Each event carries the
/// position where the contact went down, so a listener reads the full travel from any one event.
///
/// ```
/// use zgui_geom::{CssPx, Point, Size};
/// use zgui_vocab::{GesturePhase, PanEvent, PointerKind, Velocity};
///
/// let moved = PanEvent {
///     phase: GesturePhase::Moved,
///     origin: Point::new(CssPx(10.0), CssPx(20.0)),
///     position: Point::new(CssPx(70.0), CssPx(25.0)),
///     velocity: Velocity { x: 600.0, y: 0.0 },
///     kind: PointerKind::Touch,
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
