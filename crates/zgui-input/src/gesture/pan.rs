//! One contact, from touch-down to lift, and the speed it left with.
//!
//! The speed follows UIScrollView, which carries on at a speed lower than its pan recogniser
//! reports, because a finger slows down as it lifts. The weights of the last three steps are the
//! ones Flutter measured to match it (`IOSScrollViewFlingVelocityTracker`): the oldest of the
//! three counts most and the newest least.

use core::time::Duration;

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::{Timestamp, Velocity};

use crate::gesture::longpress::LONG_PRESS;
use crate::gesture::tap::travelled;

/// What one move of a tracked contact produced.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Movement {
    /// How far it moved since the previous reading.
    pub by: Size<CssPx, Css>,
    /// How fast it travels, measured over the window that ends with this move.
    pub velocity: Velocity,
    /// Whether this is the move that turned it into a pan.
    pub began_panning: bool,
}

/// How much each of the last three steps counts in a speed, newest first.
const WEIGHTS: [f32; 3] = [0.05, 0.35, 0.6];

/// A contact that has not moved for longer than this has stopped.
///
/// A finger that stops and then lifts has no speed left, whatever it moved at before it stopped.
const AT_REST: Duration = Duration::from_millis(40);

/// How many recent positions a contact keeps: one more than the steps in a speed.
const SAMPLES: usize = WEIGHTS.len() + 1;

/// Two positions closer together in time than this are one position.
///
/// A platform that delivers a batch of moves at once stamps them nearly together, and a step over
/// no time reads as a speed of thousands of pixels per second.
const TOGETHER: Duration = Duration::from_millis(1);

/// Where a contact was, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    /// The position.
    at: Point<CssPx, Css>,
    /// When it was there.
    when: Timestamp,
}

/// One contact that is down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    /// Where it went down.
    origin: Point<CssPx, Css>,
    /// Where it is now.
    at: Point<CssPx, Css>,
    /// When it went down.
    started: Timestamp,
    /// Its most recent positions, newest first.
    samples: [Option<Sample>; SAMPLES],
    /// Whether it has travelled past the slop.
    panning: bool,
    /// Whether it has already produced something that rules a tap out.
    spent: bool,
    /// Whether an element took its long press, so that its later travel does not pan.
    claimed: bool,
}

impl Track {
    /// A contact that has just gone down at `at`.
    pub fn started(at: Point<CssPx, Css>, when: Timestamp) -> Self {
        let mut samples = [None; SAMPLES];
        samples[0] = Some(Sample { at, when });
        Self {
            origin: at,
            at,
            started: when,
            samples,
            panning: false,
            spent: false,
            claimed: false,
        }
    }

    /// Where it went down.
    pub fn origin(self) -> Point<CssPx, Css> {
        self.origin
    }

    /// Where it is now.
    pub fn at(self) -> Point<CssPx, Css> {
        self.at
    }

    /// Whether it is dragging.
    pub fn is_panning(self) -> bool {
        self.panning
    }

    /// Whether it has already meant something that a tap cannot follow.
    pub fn is_spent(self) -> bool {
        self.spent
    }

    /// Whether a long press is still possible for it.
    pub fn may_become_long_press(self) -> bool {
        !self.panning && !self.spent
    }

    /// Records that an element took its long press. It then does not become a pan.
    pub fn claim(&mut self) {
        self.claimed = true;
    }

    /// Records that a pinch took it. It then stops panning and does not tap or pan again.
    pub fn hold(&mut self) {
        self.claimed = true;
        self.spent = true;
        self.panning = false;
    }

    /// Records that it moved to `to`.
    pub fn moved(&mut self, to: Point<CssPx, Css>, when: Timestamp) -> Movement {
        let by = Size::new(CssPx(to.x.0 - self.at.x.0), CssPx(to.y.0 - self.at.y.0));
        self.at = to;
        self.passed(to, when);
        let velocity = self.velocity(when);
        let began_panning = !self.panning && !self.claimed && travelled(self.origin, to);
        if began_panning {
            self.panning = true;
            self.spent = true;
        }
        Movement {
            by,
            velocity,
            began_panning,
        }
    }

    /// Records that it passed through `at` on its way to its next move, which counts towards its
    /// speed only.
    ///
    /// A queue that folds several moves into one keeps the positions of the folded ones, and each
    /// is a step of the speed.
    pub fn passed(&mut self, at: Point<CssPx, Css>, when: Timestamp) {
        // A position that arrives together with the newest sample takes its place and keeps its
        // stamp, so a fast run of moves still steps on at least once a millisecond.
        match self.samples[0] {
            Some(newest) if when.saturating_since(newest.when) < TOGETHER => {
                self.samples[0] = Some(Sample {
                    at,
                    when: newest.when,
                });
            }
            _ => {
                self.samples.rotate_right(1);
                self.samples[0] = Some(Sample { at, when });
            }
        }
    }

    /// Whether it has now been held long enough to be a long press, marking it spent if so.
    ///
    /// Answers true exactly once per contact: a held finger means "open the menu", not "open the
    /// menu again on every frame for as long as it is held".
    pub fn becomes_long_press(&mut self, now: Timestamp) -> bool {
        if !self.may_become_long_press() || now.saturating_since(self.started) < LONG_PRESS {
            return false;
        }
        self.spent = true;
        true
    }

    /// How long until it becomes a long press, if it still can.
    pub fn long_press_in(self, now: Timestamp) -> Option<Duration> {
        self.may_become_long_press()
            .then(|| LONG_PRESS.saturating_sub(now.saturating_since(self.started)))
    }

    /// How fast it is travelling at `now`, from its last three steps.
    ///
    /// A step that is missing, or that took no time, counts as no speed. A step that took longer
    /// than 40 ms spans a stop, and it and the steps before it count as no speed.
    pub fn velocity(self, now: Timestamp) -> Velocity {
        let Some(newest) = self.samples[0] else {
            return Velocity::default();
        };
        if now.saturating_since(newest.when) > AT_REST {
            return Velocity::default();
        }

        let mut velocity = Velocity::default();
        for (step, weight) in WEIGHTS.iter().enumerate() {
            let (Some(end), Some(start)) = (self.samples[step], self.samples[step + 1]) else {
                break;
            };
            let took = end.when.saturating_since(start.when);
            if took > AT_REST {
                break;
            }
            let seconds = took.as_secs_f32();
            if seconds <= 0.0 {
                continue;
            }
            velocity.x += weight * (end.at.x.0 - start.at.x.0) / seconds;
            velocity.y += weight * (end.at.y.0 - start.at.y.0) / seconds;
        }

        velocity
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::{Timestamp, Velocity};

    use super::Track;

    fn at(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    fn stamp(millis: u64) -> Timestamp {
        Timestamp::from_origin(Duration::from_millis(millis))
    }

    #[test]
    fn a_contact_becomes_a_pan_once_and_stays_one() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        assert!(!track.moved(at(0.0, 4.0), stamp(10)).began_panning);
        assert!(track.moved(at(0.0, 40.0), stamp(20)).began_panning);
        assert!(
            !track.moved(at(0.0, 80.0), stamp(30)).began_panning,
            "a pan begins once, or every move of a drag opens a new drag"
        );
        assert!(track.is_panning());
    }

    #[test]
    fn a_claimed_long_press_does_not_become_a_pan() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        assert!(track.becomes_long_press(stamp(1000)));
        track.claim();
        assert!(!track.moved(at(0.0, 80.0), stamp(1016)).began_panning);
        assert!(!track.is_panning());
    }

    #[test]
    fn an_unclaimed_long_press_can_still_pan() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        assert!(track.becomes_long_press(stamp(1000)));
        assert!(track.moved(at(0.0, 80.0), stamp(1016)).began_panning);
    }

    #[test]
    fn a_move_reports_the_step_and_not_the_position() {
        let mut track = Track::started(at(10.0, 10.0), stamp(0));
        let moved = track.moved(at(10.0, 50.0), stamp(16));
        assert_eq!(moved.by.height, CssPx(40.0));
        let again = track.moved(at(10.0, 60.0), stamp(32));
        assert_eq!(again.by.height, CssPx(10.0));
    }

    #[test]
    fn a_steady_finger_reads_its_speed() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        let mut moved = track.moved(at(8.0, 0.0), stamp(16));
        for step in 2..=6 {
            moved = track.moved(at(step as f32 * 8.0, 0.0), stamp(step * 16));
        }
        assert!(
            (moved.velocity.x - 500.0).abs() < 0.01 && moved.velocity.y == 0.0,
            "a finger travelling steadily read {:?}",
            moved.velocity
        );
    }

    #[test]
    fn moves_stamped_together_are_one_position() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in 1..=3 {
            track.moved(at(0.0, step as f32 * 10.0), stamp(step * 10));
        }
        // A batch of two moves in the same millisecond.
        let micros = |micros: u64| Timestamp::from_origin(Duration::from_micros(micros));
        track.moved(at(0.0, 35.0), micros(40_100));
        track.moved(at(0.0, 40.0), micros(40_200));
        let velocity = track.velocity(stamp(41));
        assert!(
            (velocity.y - 1000.0).abs() < 5.0,
            "a finger at 1000 px/s read {} px/s",
            velocity.y
        );
    }

    #[test]
    fn the_oldest_of_the_last_three_steps_counts_most() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        // Steps at 3000, 2000, 1000 and then 500 pixels per second.
        track.moved(at(0.0, 30.0), stamp(10));
        track.moved(at(0.0, 50.0), stamp(20));
        track.moved(at(0.0, 60.0), stamp(30));
        track.moved(at(0.0, 65.0), stamp(40));
        let velocity = track.velocity(stamp(40));
        let expected = 0.05 * 500.0 + 0.35 * 1000.0 + 0.6 * 2000.0;
        assert!(
            (velocity.y - expected).abs() < 0.01,
            "read {velocity:?}, expected {expected}"
        );
    }

    #[test]
    fn a_flick_at_the_end_of_a_slow_drag_reads_as_a_flick() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        // Half a second of slow dragging.
        for step in 1..=10 {
            track.moved(at(0.0, step as f32 * 5.0), stamp(step * 50));
        }
        // Then four steps of 20 pixels in 10 milliseconds each.
        for step in 1..=4 {
            track.moved(at(0.0, 50.0 + step as f32 * 20.0), stamp(500 + step * 10));
        }
        let velocity = track.velocity(stamp(540));
        assert!(
            (velocity.y - 2000.0).abs() < 0.01,
            "the flick read {} px/s; the slow drag before it counted",
            velocity.y
        );
    }

    #[test]
    fn a_finger_that_stops_and_twitches_before_it_lifts_has_no_speed() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in 1..=3 {
            track.moved(at(0.0, step as f32 * 20.0), stamp(step * 10));
        }
        // Held for 300 ms, then one pixel as it lifts.
        track.moved(at(0.0, 61.0), stamp(330));
        let velocity = track.velocity(stamp(330));
        assert!(
            velocity.y.abs() < 1.0,
            "a finger that stopped read {} px/s",
            velocity.y
        );
    }

    #[test]
    fn a_finger_that_stops_before_it_lifts_has_no_speed() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in 1..=4 {
            track.moved(at(0.0, step as f32 * 20.0), stamp(step * 10));
        }
        assert!(track.velocity(stamp(70)).y > 0.0, "held for 30 ms");
        assert_eq!(
            track.velocity(stamp(90)),
            Velocity::default(),
            "held for 50 ms"
        );
    }

    #[test]
    fn a_contact_that_never_moved_has_no_speed() {
        let track = Track::started(at(0.0, 0.0), stamp(0));
        assert_eq!(track.velocity(stamp(0)), Velocity::default());
        assert_eq!(track.velocity(stamp(200)), Velocity::default());
    }

    #[test]
    fn a_panning_contact_can_no_longer_become_a_long_press() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        track.moved(at(0.0, 40.0), stamp(20));
        assert!(!track.may_become_long_press());
        assert!(!track.becomes_long_press(stamp(900)));
        assert_eq!(track.long_press_in(stamp(0)), None);
    }
}
