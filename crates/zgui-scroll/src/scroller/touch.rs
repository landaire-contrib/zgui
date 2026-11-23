//! A finger on a touch screen, which moves the content as UIScrollView does.
//!
//! A finger that comes down catches whatever the containers under it are doing: a fling stops
//! where it is and a returning edge stays where it is. A pan then latches and rails as a touchpad
//! gesture does, and holds each edge it pulls past the end on the rubber band. When the finger
//! lifts, an edge it holds returns on UIScrollView's spring, and each other axis flings with
//! UIScrollView's deceleration and bounces on the same spring at the end it reaches.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Size};
use zgui_layout::LayoutStore;

use crate::elastic::Overscroll;
use crate::motion::Motion;
use crate::scroller::Scroller;
use crate::scroller::gesture::Gesture;
use crate::stretch::Stretch;

impl Scroller {
    /// A finger came down over `chain`.
    ///
    /// Stops every motion on the chain and grips each displaced edge on it, so the finger holds
    /// the content where it is. Returns whether the finger caught content that moved visibly: a
    /// fling faster than 60 CSS pixels per second, a smooth scroll, or an edge on its way back. A
    /// touch on such content stops it and is not a tap.
    pub fn press(&mut self, chain: &[NodeKey]) -> bool {
        let caught_speed = CAUGHT_SPEED * self.scale;
        let mut moving = false;
        for container in chain {
            moving |= match self.motions.remove(container) {
                Some(Motion::Fling(momentum)) => {
                    let velocity = momentum.velocity();
                    velocity.width.0.hypot(velocity.height.0) > caught_speed
                }
                Some(Motion::Smooth(_)) => true,
                None => false,
            };
            if let Some(edge) = self.elastic.get_mut(container) {
                moving |= !edge.is_gripped();
                *edge = edge.gripped();
            }
        }
        self.gesture = Some(Gesture::touching());

        moving
    }

    /// Chooses the axes the pan of the finger moves, from `travel`: how far the finger went before
    /// it began to pan.
    ///
    /// A pan begins only once the finger has travelled past the slop, and its first movement is
    /// what remains of the step that crossed it. The whole travel says more about the direction.
    pub fn steer(&mut self, travel: Size<DevicePx, Device>) {
        if let Some(gesture) = self.gesture.as_mut() {
            gesture.aim(travel);
        }
    }

    /// The finger lifted, moving the content at `velocity` device pixels per second.
    ///
    /// Each edge the finger holds returns on UIScrollView's spring, and the latched container's
    /// edge keeps the speed the finger pulled it outwards with. Each other axis of the latched
    /// container flings at `velocity`, along the rail of the pan. `panned` is the container under
    /// the start of the pan, which flings when another input took the gesture away during the
    /// pan. Returns the containers whose edges now return.
    pub fn release(
        &mut self,
        store: &LayoutStore,
        panned: Option<NodeKey>,
        velocity: Size<DevicePx, Device>,
        stretch: Stretch,
    ) -> SmallVec<[NodeKey; 2]> {
        let gesture = self.gesture.take();
        let latched = gesture.and_then(|gesture| gesture.latched).or(panned);
        let velocity = gesture.map_or(velocity, |gesture| gesture.along(velocity));
        let still = Size::new(DevicePx(0.0), DevicePx(0.0));

        let gripped: SmallVec<[(NodeKey, Overscroll); 2]> = self
            .elastic
            .iter()
            .filter(|(_, edge)| edge.is_gripped())
            .map(|(container, edge)| (*container, *edge))
            .collect();
        let mut returning: SmallVec<[NodeKey; 2]> = SmallVec::new();
        for (container, edge) in gripped {
            let speed = if latched == Some(container) {
                velocity
            } else {
                still
            };
            self.displace(
                container,
                edge.let_go(speed, self.extent_for(store, container)),
            );
            returning.push(container);
        }

        let Some(container) = latched else {
            return returning;
        };
        let held = self.elastic_of(container);
        let thrown = Size::new(
            if held.width.0 == 0.0 {
                velocity.width
            } else {
                DevicePx(0.0)
            },
            if held.height.0 == 0.0 {
                velocity.height
            } else {
                DevicePx(0.0)
            },
        );
        self.fling(store, container, thrown, stretch);

        returning
    }
}

/// The slowest fling that a touch catches, in CSS pixels per second. A touch on a slower fling is
/// a tap.
///
/// One CSS pixel per frame at sixty hertz. A fling slower than that is near its end and hard to
/// see move.
const CAUGHT_SPEED: f32 = 60.0;
