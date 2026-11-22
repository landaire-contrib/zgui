//! Telling elements that something dragged from outside the window is over them.
//!
//! The platform reports a drag entering the surface, moving over it, leaving it and dropping on
//! it. The drop is dispatched like any other input. The motion before it becomes three events and
//! one state, so an element can show that it will take the drop:
//!
//! * `drag_enter` and `drag_leave`, one dispatch per element that the drag moved onto or off, as
//!   `pointer_enter` and `pointer_leave` are. They do not bubble.
//! * `drag_over` on the innermost element under the drag, on every move. It bubbles.
//! * the drag-over state on every element under the drag: [`UiState::DRAG_OVER`] for code, and
//!   the custom state `drag-over` for style sheets, matched by `:state(drag-over)`. Servo's
//!   selector parser has no pseudo-class for the bit, so the state is the selector's way in.
//!
//! `drag_enter` and `drag_over` carry the dragged paths, which the platform gives only on entry,
//! and where the drag is. `drag_leave` carries only where the drag is, and no position when the
//! drag left the window. A drop clears the state without a `drag_leave`: the drop is the drag's
//! end.

use std::path::PathBuf;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_interned::Ident;
use zgui_platform::DragEvent;
use zgui_vocab::{DragLeaveEvent, DropEvent, EventKind, Payload, Timestamp, UiState};

use crate::window::Window;

/// The custom state an element carries while a drag is over it.
pub(crate) const DRAG_OVER_STATE: &str = "drag-over";

/// The elements a drag is over, root first, and what it carries.
#[derive(Debug, Default)]
pub(crate) struct DragOver {
    path: Vec<NodeKey>,
    paths: Vec<PathBuf>,
}

impl Window {
    /// Follows one drag event from the platform.
    pub(crate) fn drag_moved(&mut self, event: &DragEvent, timestamp: Timestamp) {
        match event {
            DragEvent::Entered { paths, position } => {
                self.drag.paths.clone_from(paths);
                self.drag_to(Some(*position), true, timestamp);
            }
            DragEvent::Moved { position } => self.drag_to(Some(*position), true, timestamp),
            DragEvent::Left => {
                self.drag_to(None, true, timestamp);
                self.drag.paths.clear();
            }
            DragEvent::Dropped { .. } => {
                self.drag_to(None, false, timestamp);
                self.drag.paths.clear();
            }
            // A kind of drag this window does not know yet changes nothing it shows.
            _ => {}
        }
    }

    /// Moves the drag onto whatever is under `position`, or off everything.
    fn drag_to(
        &mut self,
        position: Option<Point<CssPx, Css>>,
        announce: bool,
        timestamp: Timestamp,
    ) {
        let (moved, target) = {
            let document = self.document.borrow();
            let layout = self.layout.borrow();
            let filter = self.engine.filter();
            let chain = position.map(|position| {
                let world = zgui_input::World {
                    document: &document,
                    layout: &layout,
                    hit: &self.hit,
                    clips: &self.scene.clips,
                    spatial: &self.scene.spatial,
                    scale: zgui_geom::Scale::new(self.scale),
                    filter: &filter,
                };
                world.chain_at(Point::new(
                    zgui_geom::DevicePx(position.x.0 * self.scale),
                    zgui_geom::DevicePx(position.y.0 * self.scale),
                ))
            });
            let to: Vec<NodeKey> = chain
                .as_ref()
                .map(|chain| chain.path().to_vec())
                .unwrap_or_default();
            let moved =
                zgui_input::move_bit(&document, &filter, UiState::DRAG_OVER, &self.drag.path, &to);
            if !moved.is_empty() {
                document
                    .edit(&filter, |edit| {
                        let changes = moved.left.iter().map(|key| (key, false));
                        for (key, on) in changes.chain(moved.entered.iter().map(|key| (key, true)))
                        {
                            if let Some(index) = edit.document().store().index_of(*key) {
                                edit.set_custom_state(index, Ident::new(DRAG_OVER_STATE), on);
                            }
                        }
                    })
                    .expect("the document is not poisoned");
            }
            self.drag.path = to;
            (moved, chain.and_then(|chain| chain.target()))
        };
        if !announce {
            return;
        }
        // Departures from the innermost outwards, then arrivals from the outermost inwards, as
        // pointer crossings are announced.
        let leave = Payload::DragLeave(DragLeaveEvent { position });
        for node in moved.left.iter().rev() {
            self.dispatch_synthetic(*node, EventKind::DragLeave, leave.clone(), timestamp);
        }
        // Only a drag over the window is over an element, so arrivals and moves have a position.
        let Some(position) = position else {
            return;
        };
        let over = Payload::Drop(DropEvent {
            paths: self.drag.paths.clone(),
            position,
        });
        for node in &moved.entered {
            self.dispatch_synthetic(*node, EventKind::DragEnter, over.clone(), timestamp);
        }
        if let Some(target) = target {
            self.dispatch_synthetic(target, EventKind::DragOver, over, timestamp);
        }
    }
}
