//! Pans that an element takes from the scrolling.
//!
//! A touch contact that travels past the slop is a pan. The axis that it travelled farther along
//! when it became a pan is its axis, and the window decides once who gets it. It walks from the
//! element under the start point outwards, and offers the pan to each element on the way that
//! listens for pans along that axis ([`EventKind::HorizontalPan`] or [`EventKind::VerticalPan`]):
//!
//! * A listener registered in the capture phase is offered the pan wherever it is. It claims the
//!   pan before the scroll containers inside it.
//! * A listener registered in the bubble phase is offered the pan only when no scroll container
//!   between it and the start point can scroll in the direction the contact moves.
//! * The offer is the [`GesturePhase::Started`] event. A listener that calls `prevent_default` on
//!   it refuses the pan, and the walk goes on outwards.
//! * A pan that no listener takes scrolls.
//!
//! The decision holds until the contact lifts. Each event of a pan that an element took goes to
//! that element alone, with no capture or bubble phase through the others. A row that listens for
//! horizontal pans inside a vertical list thus gets the sideways pans that start on it, and the
//! list keeps the vertical ones. A horizontal carousel between the two keeps the sideways pans
//! while it can scroll.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::{
    EventKind, GesturePhase, PanEvent, Payload, PointerId, PointerKind, ScreenEdge, Timestamp,
    Velocity,
};

use crate::window::Window;

/// The pan in progress that an element took.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ElementPan {
    /// The contact that pans.
    pointer: PointerId,
    /// The element that took the pan. The events go to it.
    listener: NodeKey,
    /// Which pan event the listener asked for.
    kind: EventKind,
    /// Where the contact went down.
    origin: Point<CssPx, Css>,
    /// Where the contact was at the last event.
    position: Point<CssPx, Css>,
    /// The window edge the contact went down at, if any.
    edge: Option<ScreenEdge>,
}

/// How an element listens for pans along one axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Listening {
    /// It does not.
    No,
    /// In the bubble phase: it takes pans that no scroll container inside it can take.
    Bubble,
    /// In the capture phase: it is offered pans before the scroll containers inside it.
    Capture,
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
        let window = Size::new(self.viewport.width, self.viewport.height);
        let edge = ScreenEdge::near(from, window);
        let containers = self.scroll_chain(target);

        let mut walk = Some(target);
        let mut blocked = false;
        while let Some(node) = walk {
            walk = self.parent_of(node);
            let offered = match self.listening(node, axis) {
                Listening::No => false,
                Listening::Bubble => !blocked,
                Listening::Capture => true,
            };
            if offered {
                let pan = ElementPan {
                    pointer,
                    listener: node,
                    kind: axis.event(),
                    origin: from,
                    position: at,
                    edge,
                };
                if self.offer_pan(pan, timestamp) {
                    return true;
                }
            }
            blocked |= containers.contains(&node) && self.can_scroll(node, axis, travel);
        }

        false
    }

    /// Offers `pan` to its listener with its first event. Returns whether the listener took it.
    fn offer_pan(&mut self, pan: ElementPan, timestamp: Timestamp) -> bool {
        // One element pan at a time, as for scrolling: a second contact takes over.
        if let Some(previous) = self.element_pan.take() {
            self.send_pan(
                previous,
                GesturePhase::Cancelled,
                Velocity::default(),
                timestamp,
            );
        }
        let refused = self
            .send_pan(pan, GesturePhase::Started, Velocity::default(), timestamp)
            .is_none_or(|dispatched| !dispatched.default_allowed);
        if refused {
            return false;
        }
        self.element_pan = Some(pan);

        true
    }

    /// How `node` listens for pans along `axis`.
    fn listening(&self, node: NodeKey, axis: Axis) -> Listening {
        let event = axis.event();
        let document = self.document.borrow();
        let Some(set) = document.store().columns().listeners.get(node) else {
            return Listening::No;
        };
        let mut listening = Listening::No;
        for listener in set.iter().filter(|listener| listener.kind == event) {
            if listener.options.capture {
                return Listening::Capture;
            }
            listening = Listening::Bubble;
        }

        listening
    }

    /// The parent element of `node`, if it has one.
    fn parent_of(&self, node: NodeKey) -> Option<NodeKey> {
        let document = self.document.borrow();
        let store = document.store();
        let index = store.index_of(node)?;
        let parent = store.core(index).parent()?;

        Some(store.core(parent).key())
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

    /// Cancels the element pan in progress of any contact, for a pinch that took the contacts.
    pub(super) fn cancel_any_element_pan(&mut self, timestamp: Timestamp) {
        let Some(pan) = self.element_pan.take() else {
            return;
        };

        self.send_pan(pan, GesturePhase::Cancelled, Velocity::default(), timestamp);
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

    /// Dispatches one event of `pan` to its listener alone. Returns what the dispatch did, or
    /// `None` when the listener left the document.
    fn send_pan(
        &mut self,
        pan: ElementPan,
        phase: GesturePhase,
        velocity: Velocity,
        timestamp: Timestamp,
    ) -> Option<crate::dispatch::Dispatched> {
        let event = PanEvent {
            phase,
            origin: pan.origin,
            position: pan.position,
            velocity,
            kind: PointerKind::Touch,
            edge: pan.edge,
        };
        self.dispatch_alone(pan.listener, pan.kind, Payload::Pan(event), timestamp)
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
