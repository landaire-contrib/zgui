//! The speed a gesture left behind, spending itself over the frames that follow.
//!
//! The decay is UIScrollView's. Its speed is multiplied by the deceleration rate once each
//! millisecond, so after `t` milliseconds a fling thrown at `v0` moves at `v0 * r^t` and has
//! travelled `v0 * (r^t - 1) / ln(r)`. A gentle flick and a hard one feel the same and go
//! different distances, which is what a flick means.

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Point, Size};

use crate::axis::Axis;
use crate::motion::{Step, stopped};
use crate::stretch::Stretch;

/// How much of a fling's speed survives one millisecond.
///
/// `UIScrollView.DecelerationRate.normal`.
const RATE_PER_MILLISECOND: f32 = 0.998;

/// How fast a fling's speed decays, per second: `-ln(r)` for a rate `r` per second.
fn decay() -> f32 {
    -1000.0 * RATE_PER_MILLISECOND.ln()
}

/// A container carrying the speed of the gesture that let go of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Momentum {
    /// Device pixels per second, in the direction the content is moving.
    velocity: Size<DevicePx, Device>,
    /// Whether the fling may carry the edge past the end it reaches.
    stretch: Stretch,
}

impl Momentum {
    /// A fling at `velocity`, in device pixels per second, that bounces at an end if `stretch`
    /// permits it.
    pub fn new(velocity: Size<DevicePx, Device>, stretch: Stretch) -> Self {
        Self { velocity, stretch }
    }

    /// The speed left, in device pixels per second.
    pub fn velocity(&self) -> Size<DevicePx, Device> {
        self.velocity
    }

    /// Whether the fling may carry the edge past the end it reaches.
    pub fn stretch(&self) -> Stretch {
        self.stretch
    }

    /// Spends `elapsed` of it, from wherever the container is now.
    pub fn advance(&mut self, at: Point<DevicePx, Device>, elapsed: Duration) -> Step {
        let seconds = elapsed.as_secs_f32();
        let factor = (-decay() * seconds).exp();
        // The integral of the decay over the step, which is the distance travelled. The start
        // speed times the step would overshoot by a frame's worth on every tick, and a flick would
        // travel further at 30 Hz than at 120 Hz.
        let travelled = (1.0 - factor) / decay();
        let to = Point::new(
            DevicePx(at.x.0 + self.velocity.width.0 * travelled),
            DevicePx(at.y.0 + self.velocity.height.0 * travelled),
        );
        self.velocity = Size::new(
            DevicePx(self.velocity.width.0 * factor),
            DevicePx(self.velocity.height.0 * factor),
        );
        Step {
            to,
            done: stopped(self.velocity),
        }
    }

    /// When a fling at `speed` has travelled `distance` along one axis, and the speed it has left
    /// then. Nothing when it stops short of `distance`.
    pub(crate) fn reaching(speed: f32, distance: f32) -> Option<Reach> {
        if distance == 0.0 {
            return Some(Reach {
                after: Duration::ZERO,
                speed,
            });
        }
        if speed == 0.0 || speed.signum() != distance.signum() {
            return None;
        }
        let left = 1.0 - distance * decay() / speed;
        if left <= 0.0 {
            return None;
        }
        Some(Reach {
            after: Duration::from_secs_f32(-left.ln() / decay()),
            speed: speed * left,
        })
    }

    /// The same fling with no speed left on `axis`.
    pub(crate) fn stopped_on(self, axis: Axis) -> Self {
        let velocity = match axis {
            Axis::Across => Size::new(DevicePx(0.0), self.velocity.height),
            Axis::Down => Size::new(self.velocity.width, DevicePx(0.0)),
        };
        Self { velocity, ..self }
    }
}

/// Where a fling reaches a distance along one axis.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Reach {
    /// How long it takes to get there.
    pub(crate) after: Duration,
    /// The speed it has left there, in device pixels per second.
    pub(crate) speed: f32,
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Device, DevicePx, Point, Size};

    use super::Momentum;
    use crate::stretch::Stretch;

    fn speed(y: f32) -> Size<DevicePx, Device> {
        Size::new(DevicePx(0.0), DevicePx(y))
    }

    fn origin() -> Point<DevicePx, Device> {
        Point::new(DevicePx(0.0), DevicePx(0.0))
    }

    /// How far a fling travels in total, and how many frames it takes.
    fn run(initial: f32) -> (f32, u32) {
        let mut momentum = Momentum::new(speed(initial), Stretch::Permitted);
        let mut at = origin();
        let mut frames = 0;
        loop {
            let step = momentum.advance(at, Duration::from_millis(16));
            at = step.to;
            frames += 1;
            if step.done || frames > 600 {
                return (at.y.0, frames);
            }
        }
    }

    #[test]
    fn a_harder_flick_travels_further() {
        let (gentle, _) = run(400.0);
        let (hard, _) = run(2_000.0);
        assert!(hard > gentle * 4.0, "{hard} against {gentle}");
    }

    #[test]
    fn a_fling_decelerates_as_uiscrollview_does() {
        // After `t` milliseconds at a rate of 0.998 per millisecond, a fling at `v0` pixels per
        // millisecond has travelled `v0 * (0.998^t - 1) / ln(0.998)`.
        let mut momentum = Momentum::new(speed(3_000.0), Stretch::Permitted);
        let mut at = origin();
        for frame in 1..=60_u16 {
            at = momentum.advance(at, Duration::from_millis(16)).to;
            let millis = f64::from(frame) * 16.0;
            let expected = 3.0 * (0.998_f64.powf(millis) - 1.0) / 0.998_f64.ln();
            assert!(
                (f64::from(at.y.0) - expected).abs() < 0.05,
                "after {millis} ms the fling was at {}, UIScrollView at {expected}",
                at.y.0
            );
        }
        let left = 3_000.0 * 0.998_f32.powf(960.0);
        assert!((momentum.velocity().height.0 - left).abs() < 0.1);
    }

    #[test]
    fn a_fling_comes_to_rest_where_apple_says_it_does() {
        // The end point from "Designing Fluid Interfaces": `v0 / 1000 * r / (1 - r)`.
        let (travelled, frames) = run(2_000.0);
        let expected = 2.0 * 0.998 / (1.0 - 0.998);
        assert!(
            (travelled - expected).abs() < expected * 0.01,
            "{travelled} against {expected}"
        );
        assert!(frames < 300, "it was still running after {frames} frames");
    }

    #[test]
    fn the_distance_does_not_depend_on_the_frame_rate() {
        let mut fast = Momentum::new(speed(1_000.0), Stretch::Permitted);
        let mut slow = Momentum::new(speed(1_000.0), Stretch::Permitted);
        let mut fast_at = origin();
        for _ in 0..4 {
            fast_at = fast.advance(fast_at, Duration::from_millis(8)).to;
        }
        let slow_at = slow.advance(origin(), Duration::from_millis(32)).to;
        assert!(
            (fast_at.y.0 - slow_at.y.0).abs() < 1.0,
            "{} against {}: a flick that travels further on a slower display is a flick whose \
             physics is a per-frame constant",
            fast_at.y.0,
            slow_at.y.0
        );
    }

    #[test]
    fn a_fling_that_was_never_thrown_is_already_finished() {
        let mut momentum = Momentum::new(speed(0.0), Stretch::Permitted);
        assert!(momentum.advance(origin(), Duration::from_millis(16)).done);
    }

    #[test]
    fn a_fling_reaches_a_distance_at_the_speed_its_decay_leaves() {
        let reach = Momentum::reaching(2_000.0, 300.0).expect("it travels 999 pixels");
        let mut momentum = Momentum::new(speed(2_000.0), Stretch::Permitted);
        let step = momentum.advance(origin(), reach.after);
        assert!((step.to.y.0 - 300.0).abs() < 0.05, "{:?}", step.to);
        assert!((momentum.velocity().height.0 - reach.speed).abs() < 0.05);
        assert_eq!(Momentum::reaching(2_000.0, 1_200.0), None, "it stops short");
        assert_eq!(
            Momentum::reaching(2_000.0, -10.0),
            None,
            "it goes the other way"
        );
    }
}
