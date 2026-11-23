//! Going back by dragging the top page aside, as on iOS.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::StoredValue;
use zgui::vocab::GesturePhase;
use zgui::vocab::PanEvent;
use zgui::vocab::ScreenEdge;

use crate::nav_stack::path::NavPath;
use crate::nav_stack::path::SwipeEnd;
use crate::support::fling::fling_ease;

/// Which listener took a swipe.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Taker {
    /// The page area, for a pan that starts at its leading edge. It is offered the pan before the
    /// scroll containers in the page.
    Edge,
    /// The top page, for a pan anywhere on it that no scroll container takes.
    Content,
}

/// A swipe that a listener holds.
#[derive(Copy, Clone)]
struct Held {
    /// Which listener holds it.
    by: Taker,
    /// The width of the page area, in CSS pixels.
    width: f32,
}

/// The swipe back of one stack.
pub(crate) struct SwipeBack<P: 'static> {
    /// The stack.
    path: NavPath<P>,
    /// The page area.
    pages: NodeRef,
    /// The swipe in progress.
    held: StoredValue<Option<Held>, LocalStorage>,
}

impl<P: 'static> Clone for SwipeBack<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: 'static> Copy for SwipeBack<P> {}

impl<P> SwipeBack<P>
where
    P: Clone + 'static,
{
    pub(crate) fn new(path: NavPath<P>, pages: NodeRef) -> Self {
        Self {
            path,
            pages,
            held: StoredValue::new_local(None),
        }
    }

    /// Handles one event of a pan offered to `by`. Returns false to refuse the pan.
    pub(crate) fn handle(self, pan: PanEvent, by: Taker) -> bool {
        if pan.phase == GesturePhase::Started {
            return self.offer(pan, by);
        }

        let Some(held) = self.held.get_value() else {
            return true;
        };
        if held.by != by {
            return true;
        }

        let travel = pan.translation().width.0;
        let core = self.path.core();
        match pan.phase {
            GesturePhase::Moved => core.drag_to(travel / held.width),
            GesturePhase::Ended => {
                self.held.set_value(None);
                core.drag_to(travel / held.width);
                let end = swipe_end(core.progress_untracked(), pan.velocity.x);
                self.finish(end, held.width, pan.velocity.x);
            }
            _ => {
                self.held.set_value(None);
                self.finish(SwipeEnd::Cancel, held.width, 0.0);
            }
        }

        true
    }

    /// Takes a pan that moves toward the trailing edge, when the stack can go back.
    fn offer(self, pan: PanEvent, by: Taker) -> bool {
        if pan.translation().width.0 <= 0.0 {
            return false;
        }

        let Some(bounds) = self.pages.window_bounds() else {
            return false;
        };
        let scale = self.pages.scale();
        let width = bounds.size.width.0 / scale;
        if width <= 0.0 {
            return false;
        }
        let from_edge = pan.edge == Some(ScreenEdge::Left)
            || is_at_edge(pan.origin.x.0 - bounds.origin.x.0 / scale);
        if by == Taker::Edge && !from_edge {
            return false;
        }
        if !self.path.begin_swipe() {
            return false;
        }

        self.held.set_value(Some(Held { by, width }));
        true
    }

    /// Lets go of the page, which slides to where `end` puts it at the speed of the finger.
    fn finish(self, end: SwipeEnd, width: f32, velocity: f32) {
        const DURATION: f32 = 0.5;

        let progress = self.path.core().progress_untracked();
        let remaining = match end {
            SwipeEnd::Complete => (1.0 - progress) * width,
            SwipeEnd::Cancel => -progress * width,
        };
        let ended = self
            .path
            .end_swipe(end, fling_ease(remaining, velocity, DURATION));

        // A page that is already where it ends has no transition to wait for.
        if ended && remaining.abs() < 1.0 {
            self.path.settle();
        }
    }
}

/// Whether a pan that starts `x` CSS pixels from the leading edge of the page area starts at that
/// edge.
fn is_at_edge(x: f32) -> bool {
    const EDGE: f32 = 20.0;

    (0.0..=EDGE).contains(&x)
}

/// How a swipe released at `progress` of the width, moving at `velocity`, ends.
///
/// A swipe completes past half the width, or when the finger moves toward the trailing edge faster
/// than about 300 pixels per second. A finger that moves back as fast cancels it.
fn swipe_end(progress: f32, velocity: f32) -> SwipeEnd {
    const FLICK: f32 = 300.0;

    if velocity > FLICK || (progress > 0.5 && velocity > -FLICK) {
        SwipeEnd::Complete
    } else {
        SwipeEnd::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swipe_completes_past_half_or_on_a_flick() {
        assert_eq!(swipe_end(0.6, 0.0), SwipeEnd::Complete);
        assert_eq!(swipe_end(0.4, 0.0), SwipeEnd::Cancel);
        assert_eq!(swipe_end(0.1, 400.0), SwipeEnd::Complete);
        assert_eq!(swipe_end(0.8, -400.0), SwipeEnd::Cancel);
    }

    #[test]
    fn only_a_pan_near_the_leading_edge_is_an_edge_swipe() {
        assert!(is_at_edge(4.0));
        assert!(is_at_edge(20.0));
        assert!(!is_at_edge(21.0));
        assert!(!is_at_edge(-1.0));
    }
}
