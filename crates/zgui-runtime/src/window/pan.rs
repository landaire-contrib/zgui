//! Pans that an element takes from the scrolling.
//!
//! A touch contact that travels past the slop is a pan. The axis that it travelled farther along
//! when it became a pan is its axis, and the window decides once who gets it:
//!
//! * An element that listens for pans along that axis ([`EventKind::HorizontalPan`] or
//!   [`EventKind::VerticalPan`]) gets the pan when it is the element under the start point or one
//!   of its ancestors, and no scroll container between the two can scroll in the direction the
//!   contact moves. The window then dispatches the pan to the element under the start point, and
//!   the event bubbles to the listener.
//! * Each other pan scrolls.
//!
//! The decision holds until the contact lifts. A row that listens for horizontal pans inside a
//! vertical list thus gets the sideways pans that start on it, and the list keeps the vertical
//! ones. A horizontal carousel between the two keeps the sideways pans while it can scroll.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{
    EventKind, GesturePhase, PanEvent, Payload, PointerId, PointerKind, Timestamp, Velocity,
};

use crate::window::Window;

/// The pan in progress that an element took.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ElementPan {
    /// The contact that pans.
    pointer: PointerId,
    /// The element under the point where the contact went down. The events go to it.
    target: NodeKey,
    /// Which pan event the listener asked for.
    kind: EventKind,
    /// Where the contact went down.
    origin: Point<CssPx, Css>,
    /// Where the contact was at the last event.
    position: Point<CssPx, Css>,
}

/// The axes a pan can go along.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    /// The axis a contact that travelled from `from` to `to` goes along. A diagonal goes
    /// vertically, because vertical scrolling is the most frequent use of a pan.
    fn of(from: Point<CssPx, Css>, to: Point<CssPx, Css>) -> Self {
        let across = (to.x.0 - from.x.0).abs();
        let down = (to.y.0 - from.y.0).abs();
        if across > down {
            Self::Horizontal
        } else {
            Self::Vertical
        }
    }

    /// The event that a listener for pans along this axis registers.
    fn event(self) -> EventKind {
        match self {
            Self::Horizontal => EventKind::HorizontalPan,
            Self::Vertical => EventKind::VerticalPan,
        }
    }
}

impl Window {
    /// Gives a pan that just began to an element that listens for it, if one does.
    ///
    /// Returns whether an element took the pan. If not, the pan scrolls.
    pub(super) fn claim_pan(
        &mut self,
        pointer: PointerId,
        from: Point<CssPx, Css>,
        at: Point<CssPx, Css>,
        timestamp: Timestamp,
    ) -> bool {
        let Some(target) = self.node_at(from) else {
            return false;
        };
        let axis = Axis::of(from, at);
        let travel = match axis {
            Axis::Horizontal => at.x.0 - from.x.0,
            Axis::Vertical => at.y.0 - from.y.0,
        };
        if !self.listener_before_scroller(target, axis, travel) {
            return false;
        }

        // One element pan at a time, as for scrolling: a second contact takes over.
        if let Some(previous) = self.element_pan.take() {
            self.send_pan(
                previous,
                GesturePhase::Cancelled,
                Velocity::default(),
                timestamp,
            );
        }
        let pan = ElementPan {
            pointer,
            target,
            kind: axis.event(),
            origin: from,
            position: at,
        };
        self.element_pan = Some(pan);
        self.send_pan(pan, GesturePhase::Started, Velocity::default(), timestamp);

        true
    }

    /// Moves the element pan of `pointer`. Returns whether `pointer` pans an element.
    ///
    /// A capture that arrives during the pan cancels it, as it ends a scroll drag.
    pub(super) fn follow_element_pan(
        &mut self,
        pointer: PointerId,
        at: Point<CssPx, Css>,
        velocity: Velocity,
        timestamp: Timestamp,
    ) -> bool {
        let Some(mut pan) = self.element_pan.filter(|pan| pan.pointer == pointer) else {
            return false;
        };

        if self.router.capture().of(pointer).is_some() {
            self.element_pan = None;
            self.send_pan(pan, GesturePhase::Cancelled, velocity, timestamp);
            return true;
        }

        pan.position = at;
        self.element_pan = Some(pan);
        self.send_pan(pan, GesturePhase::Moved, velocity, timestamp);

        true
    }

    /// Ends the element pan of `pointer` with the speed of the lift. Returns whether `pointer`
    /// panned an element.
    pub(super) fn end_element_pan(
        &mut self,
        pointer: PointerId,
        at: Point<CssPx, Css>,
        velocity: Velocity,
        timestamp: Timestamp,
    ) -> bool {
        let Some(mut pan) = self.element_pan.filter(|pan| pan.pointer == pointer) else {
            return false;
        };

        self.element_pan = None;
        pan.position = at;
        self.send_pan(pan, GesturePhase::Ended, velocity, timestamp);

        true
    }

    /// Cancels the element pan of `pointer`, for a contact that the platform cancelled.
    pub(crate) fn cancel_element_pan(&mut self, pointer: PointerId, timestamp: Timestamp) {
        let Some(pan) = self.element_pan.filter(|pan| pan.pointer == pointer) else {
            return;
        };

        self.element_pan = None;
        self.send_pan(pan, GesturePhase::Cancelled, Velocity::default(), timestamp);
    }

    /// Whether `from` or one of its ancestors listens for pans along `axis` and no scroll container
    /// below that listener can scroll for a contact that moved `travel` CSS pixels along it.
    fn listener_before_scroller(&self, from: NodeKey, axis: Axis, travel: f32) -> bool {
        let containers = self.scroll_chain(from);
        let event = axis.event();
        let document = self.document.borrow();
        let store = document.store();
        let Some(mut index) = store.index_of(from) else {
            return false;
        };

        loop {
            let node = store.core(index).key();
            let listens = store
                .columns()
                .listeners
                .get(node)
                .is_some_and(|set| set.listens_for(event));
            if listens {
                return true;
            }
            if containers.contains(&node) && self.can_scroll(node, axis, travel) {
                return false;
            }
            match store.core(index).parent() {
                Some(parent) => index = parent,
                None => return false,
            }
        }
    }

    /// Whether a person can scroll `container` along `axis` for a contact that moved `travel` CSS
    /// pixels along it. The content follows the contact, so the offset moves against it.
    fn can_scroll(&self, container: NodeKey, axis: Axis, travel: f32) -> bool {
        // Half a device pixel, so that an offset that rounding left beside an end counts as at
        // the end.
        const EDGE: f32 = 0.5;

        let region = {
            let layout = self.layout.borrow();
            zgui_layout::scroll_region::region_of_element(&layout, container)
        };
        let Some(region) = region else {
            return false;
        };
        let limit = region.hand_limit();
        let offset = self.scroll.borrow().offset_of(container);
        let (offset, limit) = match axis {
            Axis::Horizontal => (offset.x.0, limit.x.0),
            Axis::Vertical => (offset.y.0, limit.y.0),
        };

        if travel > 0.0 {
            offset > EDGE
        } else {
            offset < limit - EDGE
        }
    }

    /// Dispatches one event of `pan` to its target.
    fn send_pan(
        &mut self,
        pan: ElementPan,
        phase: GesturePhase,
        velocity: Velocity,
        timestamp: Timestamp,
    ) {
        let event = PanEvent {
            phase,
            origin: pan.origin,
            position: pan.position,
            velocity,
            kind: PointerKind::Touch,
        };
        self.dispatch_synthetic(pan.target, pan.kind, Payload::Pan(event), timestamp);
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{CssPx, Point};

    use super::Axis;

    #[test]
    fn the_axis_is_the_one_the_contact_travelled_farther_along() {
        let from = Point::new(CssPx(100.0), CssPx(100.0));
        assert_eq!(
            Axis::of(from, Point::new(CssPx(80.0), CssPx(105.0))),
            Axis::Horizontal
        );
        assert_eq!(
            Axis::of(from, Point::new(CssPx(105.0), CssPx(70.0))),
            Axis::Vertical
        );
        assert_eq!(
            Axis::of(from, Point::new(CssPx(110.0), CssPx(110.0))),
            Axis::Vertical,
            "a diagonal scrolls"
        );
    }
}
