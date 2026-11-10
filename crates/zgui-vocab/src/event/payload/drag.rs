//! A drag from outside the window moving off an element.

use zgui_geom::{Css, CssPx, Point};

/// What a drag leave event carries: where the drag is, when it is still over the window.
///
/// A leave carries no paths. The element got them with the enter and with each move.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_vocab::DragLeaveEvent;
///
/// let onto_a_sibling = DragLeaveEvent {
///     position: Some(Point::new(CssPx(10.0), CssPx(20.0))),
/// };
/// let off_the_window = DragLeaveEvent { position: None };
/// assert_ne!(onto_a_sibling, off_the_window);
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DragLeaveEvent {
    /// Where the drag is, in CSS pixels from the window's top-left corner.
    ///
    /// This is `None` when the drag left the window or the platform cancelled it.
    pub position: Option<Point<CssPx, Css>>,
}
