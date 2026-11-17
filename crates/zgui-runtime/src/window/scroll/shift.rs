//! Deciding whether this frame's scroll can be answered by moving pixels that are already drawn.
//!
//! A scroll moves every pixel of a scrollport, so it damages the whole port, so the emit walk
//! reaches every fragment in it and every primitive is pushed and re-ordered again — 77 % of a
//! scroll frame, measured in `docs/perf/scroll-frame.md`. None of that derives anything new. The
//! pixels are already in the renderer's composed target, one whole-pixel translation from where
//! they now belong.
//!
//! This is the gate. It says yes only for a frame that did one thing, and the fallback for every
//! other frame is the frame the window would have drawn anyway — so a refusal costs the saving and
//! never the picture.
//!
//! # The six questions
//!
//! 1. **Can the renderer do it at all?** A renderer without a persistent composed target cannot,
//!    and is never handed a damage set short of what it has to draw.
//! 2. **Did exactly one container move?** Two scrollers gliding at once are two translations, and
//!    one copy answers neither.
//! 3. **Did the walk service the frame incrementally, and did its moves agree?** Anything else the
//!    frame damaged is kept, in `RigidMoves::beyond`, and drawn on top of the moved pixels — which
//!    is what lets a scrollbar thumb move, or a row arrive at the edge, without costing the port.
//! 4. **Was the movement whole pixels, and settled?** A fraction cannot be copied, and an elastic
//!    bounce is deliberately fractional (`Scroller::compose`) so that it does not ratchet.
//! 5. **Is the port the container's alone, and opaque, and how much of it is shown?** Asked of the
//!    document, in [`zgui_paint::shiftable`].
//! 6. **Which pixels does it cover whole?** Only those are moved. A pixel the port covers in part
//!    also holds what is past its edge, so it is drawn again.

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_paint::shiftable;
use zgui_render::ScrollShift;

use crate::window::Window;

/// A scroll answered by moving pixels, and the pixels the port touches.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PortShift {
    /// The move, over the pixels the port covers whole.
    pub(crate) shift: ScrollShift,
    /// Every pixel the port touches, on the surface. What lies outside `shift.port` is covered in
    /// part and is owed as damage.
    pub(crate) touched: Rect<i32, Device>,
}

/// Why a frame's scroll was not answered by moving pixels.
///
/// Every arm is counted, because "scrolling is not being shifted" and "scrolling is not being
/// shifted *because this list has no background*" are different bugs and only one of them is the
/// framework's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The renderer keeps no composed target to move pixels within.
    NoComposedTarget,
    /// Nothing scrolled, or more than one thing did.
    NotOneContainer,
    /// The frame drew something the movement does not account for, or the scroll moved no content
    /// space.
    MoreThanAMove,
    /// The displacement is not a whole number of device pixels, or the container is bouncing.
    NotWholePixels,
    /// The document refused it. Carries which of its reasons.
    Document(shiftable::Refusal),
}

impl Window {
    /// This frame's scroll as a movement of pixels, when it is one.
    ///
    /// `viewport` is the surface's extent, which the port is cut to: nothing outside the surface is
    /// in the composed target to be moved.
    pub(crate) fn scroll_shift(
        &self,
        viewport: zgui_geom::Size<i32, Device>,
    ) -> Result<PortShift, Refused> {
        if !self.renderer.shifts_composed_pixels() {
            return Err(Refused::NoComposedTarget);
        }
        let [scrolled] = self.scrolled_this_frame.as_slice() else {
            return Err(Refused::NotOneContainer);
        };
        let moves = self.rigid_moves;
        if !moves.settled || moves.conflicted || moves.count == 0 {
            return Err(Refused::MoreThanAMove);
        }

        // Taken from the walk's own report rather than from the two offsets, because the walk is
        // what actually moved the fragments and the two must not be able to disagree.
        let Some((dx, dy)) = moves.by else {
            return Err(Refused::MoreThanAMove);
        };
        if dx.fract() != 0.0 || dy.fract() != 0.0 {
            return Err(Refused::NotWholePixels);
        }
        // An elastic bounce is fractional on purpose and is not finished moving; a container in one
        // is composed against a displacement this frame's whole-pixel copy cannot express.
        if !self
            .scroll
            .borrow()
            .elastic_of(scrolled.container)
            .is_empty()
        {
            return Err(Refused::NotWholePixels);
        }

        let port = {
            let layout = self.layout.borrow();
            shiftable::port_may_be_shifted(
                &layout,
                scrolled.container,
                &self.host.placements(),
                &self.scene.clips,
            )
            .map_err(Refused::Document)?
        };
        let surface = Rect::new(Point::new(0, 0), viewport);
        let Some(touched) = zgui_layout::fragment::diff::pixels(port).intersection(surface) else {
            return Err(Refused::NotOneContainer);
        };
        let Some(covered) = covered_pixels(port).and_then(|covered| covered.intersection(surface))
        else {
            return Err(Refused::NotOneContainer);
        };
        Ok(PortShift {
            shift: ScrollShift {
                port: covered,
                by: (dx as i32, dy as i32),
            },
            touched,
        })
    }
}

/// The whole device pixels inside `rect`, rounded inwards on every side.
fn covered_pixels(rect: Rect<DevicePx, Device>) -> Option<Rect<i32, Device>> {
    let left = rect.left().0.ceil() as i32;
    let top = rect.top().0.ceil() as i32;
    let right = rect.right().0.floor() as i32;
    let bottom = rect.bottom().0.floor() as i32;
    (right > left && bottom > top)
        .then(|| Rect::new(Point::new(left, top), Size::new(right - left, bottom - top)))
}
