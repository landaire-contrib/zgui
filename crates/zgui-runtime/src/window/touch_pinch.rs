//! Pinches that two fingers on a touch screen make.
//!
//! The recogniser reads a pinch from the first two contacts that are down. The window offers it to
//! the element under the point between them when the pinch starts: when that element or an
//! ancestor listens for pinches, the pinch goes to it until it ends, and the contacts stop panning
//! and scrolling. A pinch that no element listens for leaves the contacts to pan as before.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{EventKind, GesturePhase, Modifiers, Payload, PinchEvent, Timestamp};

use crate::window::Window;

impl Window {
    /// Carries out one reading of a touch pinch: `step` is the ratio of the distance between the
    /// fingers to their distance at the reading before, and `at` is the point between them.
    pub(super) fn carry_out_pinch(
        &mut self,
        phase: GesturePhase,
        step: f32,
        at: Point<CssPx, Css>,
        timestamp: Timestamp,
    ) {
        if phase == GesturePhase::Started {
            self.touch_pinch = self.node_at(at).filter(|node| self.hears_pinch(*node));
            if self.touch_pinch.is_none() {
                return;
            }

            self.gestures.hold_pinch();
            self.cancel_any_element_pan(timestamp);
            self.stop_scroll_pan();
        }

        let Some(node) = self.touch_pinch else {
            return;
        };
        if phase.is_last() {
            self.touch_pinch = None;
        }

        let event = PinchEvent {
            scale_delta: step - 1.0,
            phase,
            position: at,
        };
        self.dispatch_with(
            node,
            EventKind::Pinch,
            Payload::Pinch(event),
            Modifiers::NONE,
            timestamp,
        );
    }

    /// Whether `node` or an ancestor listens for pinches.
    fn hears_pinch(&self, node: NodeKey) -> bool {
        let document = self.document.borrow();
        let chain = zgui_input::HitChain::to_root(document.store(), node);
        let mut plan = zgui_input::dispatch::Plan::default();
        zgui_input::dispatch::resolve(document.store(), &chain, EventKind::Pinch, &mut plan);

        !plan.steps().is_empty()
    }
}
