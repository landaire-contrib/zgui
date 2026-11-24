//! A scroll travelling to a destination it already knows.

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Point, Size};

use crate::motion::Step;

/// The duration and curve of a smooth scroll.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Pace {
    /// [`Tween::DURATION`], fast at first and slow at the end.
    #[default]
    Standard,
    /// The duration and curve an on-screen keyboard moves with, so that the content moves with it.
    Keyboard(zgui_css::environment::KeyboardMotion),
}

impl Pace {
    /// The time the scroll takes.
    fn duration(self) -> Duration {
        match self {
            Self::Standard => Tween::DURATION,
            Self::Keyboard(motion) => motion.duration,
        }
    }

    /// How far the scroll has come `elapsed` after its start, where zero is the start and one is
    /// the end.
    fn progress(self, elapsed: Duration) -> f32 {
        match self {
            Self::Standard => ease(elapsed.as_secs_f32() / Tween::DURATION.as_secs_f32()),
            Self::Keyboard(motion) => motion.progress(elapsed),
        }
    }
}

/// A scroll interpolating from where it started to where it was asked to go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    /// Where it started.
    from: Point<DevicePx, Device>,
    /// Where it is going.
    to: Point<DevicePx, Device>,
    /// How much of the duration has passed.
    elapsed: Duration,
    /// The duration and curve.
    pace: Pace,
}

impl Tween {
    /// How long a smooth scroll takes.
    ///
    /// Long enough to be followed by the eye and short enough that a keyboard user holding the page
    /// key is not fighting the animation. It is a constant rather than a function of distance
    /// because a distance-proportional duration makes a long jump take a visibly silly amount of
    /// time, and the eye is tracking the arrival rather than the speed.
    pub const DURATION: Duration = Duration::from_millis(240);

    /// A scroll from `from` to `to` at the [`Pace::Standard`] pace.
    pub fn new(from: Point<DevicePx, Device>, to: Point<DevicePx, Device>) -> Self {
        Self {
            from,
            to,
            elapsed: Duration::ZERO,
            pace: Pace::Standard,
        }
    }

    /// The same scroll at `pace`.
    pub fn with_pace(mut self, pace: Pace) -> Self {
        self.pace = pace;
        self
    }

    /// Where it is heading.
    ///
    /// What a second detent adds to, which is the difference between two detents composing into one
    /// motion and the second one cancelling the first: a wheel turned twice in a third of a second
    /// asks to go twice as far, not to go half as far and start again.
    pub fn destination(&self) -> Point<DevicePx, Device> {
        self.to
    }

    /// Sends it somewhere else at the [`Pace::Standard`] pace, continuing from where it has
    /// actually reached.
    ///
    /// The ease starts again from `at`, and that is what makes a run of detents feel like one
    /// motion rather than a series of them. The alternative — keeping the elapsed time and moving
    /// only the destination — makes the second detent arrive almost instantly, because the ease is
    /// already most of the way through its curve and the whole of the new distance is covered in
    /// what remains of the old duration.
    ///
    /// ```
    /// use core::time::Duration;
    /// use zgui_geom::{Device, DevicePx, Point};
    /// use zgui_scroll::motion::Tween;
    ///
    /// let at = |y: f32| Point::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(y));
    /// let mut tween = Tween::new(at(0.0), at(100.0));
    /// let reached = tween.advance(Duration::from_millis(60)).to;
    /// tween.retarget(reached, at(200.0));
    /// assert_eq!(tween.destination(), at(200.0));
    /// assert!(tween.advance(Duration::from_millis(16)).to.y.0 > reached.y.0);
    /// ```
    pub fn retarget(&mut self, at: Point<DevicePx, Device>, to: Point<DevicePx, Device>) {
        self.from = at;
        self.to = to;
        self.elapsed = Duration::ZERO;
        self.pace = Pace::Standard;
    }

    /// Moves where it started and where it is going by `by`, keeping its progress.
    ///
    /// For content that moved under the scroll: the motion carries on as it was, over the moved
    /// content.
    ///
    /// ```
    /// use core::time::Duration;
    /// use zgui_geom::{Device, DevicePx, Point, Size};
    /// use zgui_scroll::motion::Tween;
    ///
    /// let at = |y: f32| Point::<DevicePx, Device>::new(DevicePx(0.0), DevicePx(y));
    /// let mut tween = Tween::new(at(0.0), at(100.0));
    /// let reached = tween.advance(Duration::from_millis(60)).to;
    /// tween.translate(Size::new(DevicePx(0.0), DevicePx(40.0)));
    /// assert_eq!(tween.destination(), at(140.0));
    /// let now = tween.advance(Duration::ZERO).to;
    /// assert!((now.y.0 - (reached.y.0 + 40.0)).abs() < 0.001);
    /// ```
    pub fn translate(&mut self, by: Size<DevicePx, Device>) {
        self.from = Point::new(
            DevicePx(self.from.x.0 + by.width.0),
            DevicePx(self.from.y.0 + by.height.0),
        );
        self.to = Point::new(
            DevicePx(self.to.x.0 + by.width.0),
            DevicePx(self.to.y.0 + by.height.0),
        );
    }

    /// Advances it by `elapsed`.
    pub fn advance(&mut self, elapsed: Duration) -> Step {
        let duration = self.pace.duration();
        self.elapsed = (self.elapsed + elapsed).min(duration);
        if self.elapsed >= duration {
            return Step {
                to: self.to,
                done: true,
            };
        }
        let eased = self.pace.progress(self.elapsed);
        Step {
            to: Point::new(
                DevicePx(lerp(self.from.x.0, self.to.x.0, eased)),
                DevicePx(lerp(self.from.y.0, self.to.y.0, eased)),
            ),
            done: false,
        }
    }
}

/// Fast at first and slow at the end, which is what makes an arrival read as an arrival.
fn ease(progress: f32) -> f32 {
    let remaining = 1.0 - progress;
    1.0 - remaining * remaining * remaining
}

/// One axis of the interpolation.
fn lerp(from: f32, to: f32, progress: f32) -> f32 {
    from + (to - from) * progress
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Device, DevicePx, Point};

    use super::{Pace, Tween};

    fn at(y: f32) -> Point<DevicePx, Device> {
        Point::new(DevicePx(0.0), DevicePx(y))
    }

    #[test]
    fn it_arrives_exactly_and_reports_that_it_has() {
        let mut tween = Tween::new(at(0.0), at(400.0));
        let step = tween.advance(Tween::DURATION);
        assert_eq!(step.to, at(400.0));
        assert!(step.done);
    }

    #[test]
    fn an_overlong_step_does_not_overshoot() {
        let mut tween = Tween::new(at(0.0), at(400.0));
        let step = tween.advance(Duration::from_secs(10));
        assert_eq!(step.to, at(400.0));
        assert!(step.done);
    }

    #[test]
    fn it_covers_more_ground_early_than_late() {
        let mut early = Tween::new(at(0.0), at(400.0));
        let first = early.advance(Duration::from_millis(60)).to.y.0;
        let second = early.advance(Duration::from_millis(60)).to.y.0 - first;
        assert!(
            first > second,
            "the first quarter covered {first} and the second {second}, which is not a deceleration"
        );
    }

    #[test]
    fn it_moves_on_the_very_first_frame() {
        // A tween that eased in as well as out would move a sub-pixel distance in the first frame,
        // and a scroll that visibly does nothing for two frames after a key press reads as a
        // dropped key press rather than as a smooth scroll.
        let mut tween = Tween::new(at(0.0), at(400.0));
        assert!(tween.advance(Duration::from_millis(16)).to.y.0 > 4.0);
    }

    #[test]
    fn a_keyboard_pace_takes_the_time_and_follows_the_curve_of_the_keyboard() {
        let linear = zgui_css::environment::KeyboardMotion {
            duration: Duration::from_millis(500),
            easing: zgui_css::environment::KeyboardEasing::CubicBezier(
                zgui_css::environment::CubicBezier {
                    x1: 0.0,
                    y1: 0.0,
                    x2: 1.0,
                    y2: 1.0,
                },
            ),
        };
        let mut tween = Tween::new(at(0.0), at(400.0)).with_pace(Pace::Keyboard(linear));
        let step = tween.advance(Tween::DURATION);
        assert!(
            !step.done,
            "the keyboard takes longer than a standard scroll"
        );
        assert!((step.to.y.0 - 192.0).abs() < 0.5, "{}", step.to.y.0);
        let step = tween.advance(Duration::from_millis(260));
        assert_eq!(step.to, at(400.0));
        assert!(step.done);
    }
}
