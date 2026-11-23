//! Advancing everything that is moving on its own.

use core::time::Duration;

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point};
use zgui_layout::LayoutStore;

use crate::axis::Axis;
use crate::motion::{Momentum, Motion};
use crate::scroller::Scroller;

impl Scroller {
    /// Stops whatever one container is doing on its own, leaving it where it is.
    ///
    /// A smooth scroll, a glide and a fling are all a container travelling towards somewhere it is
    /// not yet, so a container that is asked to stop moving has to be taken off the motion as well
    /// as refused new ones — otherwise a page frozen half a glide keeps arriving.
    pub fn halt(&mut self, container: NodeKey) {
        self.motions.remove(&container);
    }

    /// Advances every running motion and relaxes every elastic displacement by `elapsed`.
    ///
    /// Returns the containers whose **composed** position changed, which is what the caller marks
    /// — and which is deliberately not the containers that *moved*. A displacement relaxing counts
    /// even though the clamped offset did not move, because the content is drawn somewhere else;
    /// and a clamped offset that moved by less than the device grid can express does **not**,
    /// because the content is drawn in exactly the same place.
    ///
    /// That second half is worth its own paragraph, because reporting it costs far more than the
    /// frame it belongs to. Marking a container scrolled is what makes the fragment pass descend
    /// through it, and a subtree whose offset is unchanged fails the test that lets the pass
    /// *translate* it rather than compose it again — so a frame that moved a list by three
    /// hundredths of a pixel re-composes and re-encodes the slice of the document on the screen,
    /// and then presents a picture identical to the last one. Every decelerating motion ends in a
    /// run of such frames, and on a fast output that run is most of the motion.
    pub fn advance(&mut self, store: &LayoutStore, elapsed: Duration) -> SmallVec<[NodeKey; 4]> {
        let mut touched: SmallVec<[NodeKey; 4]> = SmallVec::new();
        if elapsed.is_zero() {
            return touched;
        }

        let running: SmallVec<[NodeKey; 4]> = self.motions.keys().copied().collect();
        let mut bounced: SmallVec<[NodeKey; 2]> = SmallVec::new();
        for container in running {
            let at = self.offset_of(container);
            let drawn_at = self.composed.of(container);
            let Some(motion) = self.motions.get_mut(&container) else {
                continue;
            };
            let before = *motion;
            let step = motion.advance(at, elapsed);
            let landed = self.offsets.scroll_to(store, container, step.to);
            // A motion that has run into the end of its content stops there rather than spending
            // the rest of its speed asking for frames that move nothing.
            if step.done || landed == at {
                self.motions.remove(&container);
            }
            if let Motion::Fling(momentum) = before {
                let path = Path {
                    from: at,
                    to: step.to,
                    landed,
                };
                if self.reach_end(store, container, momentum, path, elapsed) {
                    bounced.push(container);
                }
            }
            if landed != at {
                self.compose(container);
                // The listener hears about the whole movement, on the frame it happened, whether
                // or not it moved a pixel: `scrollTop` is exact, and a virtualiser reading it must
                // not see a scroll arrive in steps of a pixel.
                self.record(container, at, landed);
            }
            if self.composed.of(container) != drawn_at {
                touched.push(container);
            }
        }

        let held: SmallVec<[(NodeKey, _); 2]> = self
            .elastic
            .iter()
            .filter(|(container, _)| !bounced.contains(container))
            .map(|(container, edge)| (*container, *edge))
            .collect();
        for (container, edge) in held {
            let sprung = edge.advanced(elapsed);
            if sprung == edge {
                continue;
            }
            let drawn_at = self.composed.of(container);
            self.displace(container, sprung);
            if self.composed.of(container) != drawn_at && !touched.contains(&container) {
                touched.push(container);
            }
        }

        touched
    }

    /// Ends a fling on each axis where `path` ran into the end of the content, and bounces the
    /// edge there when the fling permits it and the content overflows on that axis.
    ///
    /// The bounce starts at the moment the fling reached the end, at the speed it had left, and
    /// has run for the rest of `elapsed`. The other axis of the edge advances by `elapsed`. Returns
    /// whether an edge bounced, which is then advanced for this frame.
    fn reach_end(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        momentum: Momentum,
        path: Path,
        elapsed: Duration,
    ) -> bool {
        let overflow = self.overflows(self.hand_limit_for(store, container));
        let before = self.overscroll_of(container);
        let mut edge = before.advanced(elapsed);
        for axis in Axis::BOTH {
            let landed = axis.of_point(path.landed);
            if (axis.of_point(path.to) - landed).abs() < 0.01 {
                continue;
            }
            if let Some(Motion::Fling(running)) = self.motions.get_mut(&container) {
                *running = running.stopped_on(axis);
                if crate::motion::stopped(running.velocity()) {
                    self.motions.remove(&container);
                }
            }
            let overflows = match axis {
                Axis::Across => overflow.across,
                Axis::Down => overflow.down,
            };
            if !momentum.stretch().is_permitted() || !overflows {
                continue;
            }
            let speed = axis.of_size(momentum.velocity());
            let Some(reach) = Momentum::reaching(speed, landed - axis.of_point(path.from)) else {
                continue;
            };
            edge = edge.bounced(axis, reach.speed, elapsed.saturating_sub(reach.after));
        }
        if edge == before.advanced(elapsed) {
            return false;
        }
        self.displace(container, edge);

        true
    }
}

/// Where one step of a fling went.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Path {
    /// The offset it started from.
    from: Point<DevicePx, Device>,
    /// The offset its speed carried it to.
    to: Point<DevicePx, Device>,
    /// The offset it landed on, inside what the content allows.
    landed: Point<DevicePx, Device>,
}
