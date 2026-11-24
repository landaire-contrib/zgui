//! One contact, from touch-down to lift, and the speed it left with.
//!
//! The speed is the slope of a weighted least-squares line through the positions of the last
//! 100 ms, in which the weight of a position halves every 10 ms. A fit over several positions
//! reads the speed of the finger when the stamps of the events are irregular, as they are on a
//! platform that stamps an event when its busy thread gets to it.

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

/// A contact that has not moved for longer than this has stopped.
///
/// A finger that stops and then lifts has no speed left, whatever it moved at before it stopped.
const AT_REST: Duration = Duration::from_millis(40);

/// How many recent positions a contact keeps: 100 ms of them at 240 Hz.
const SAMPLES: usize = 24;

/// Where a contact was, and when.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    /// The position.
    at: Point<CssPx, Css>,
    /// When it was there.
    when: Timestamp,
}

/// A position in a fit, with how much it counts.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Weighted {
    /// The position.
    at: Point<CssPx, Css>,
    /// How long before the newest position it was there, in seconds.
    age: f32,
    /// How much it counts.
    weight: f32,
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

    /// Its most recent positions, in a ring whose newest entry is at `newest`.
    samples: [Sample; SAMPLES],
    /// The index of the newest entry of `samples`.
    newest: usize,
    /// How many entries of `samples` hold a position.
    filled: usize,
    /// When the last position arrived.
    arrived: Timestamp,

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
        Self {
            origin: at,
            at,
            started: when,
            samples: [Sample { at, when }; SAMPLES],
            newest: 0,
            filled: 1,
            arrived: when,
            panning: false,
            spent: false,
            claimed: false,
        }
    }

    /// Where it went down.
    pub fn origin(&self) -> Point<CssPx, Css> {
        self.origin
    }

    /// Where it is now.
    pub fn at(&self) -> Point<CssPx, Css> {
        self.at
    }

    /// Whether it is dragging.
    pub fn is_panning(&self) -> bool {
        self.panning
    }

    /// Whether it has already meant something that a tap cannot follow.
    pub fn is_spent(&self) -> bool {
        self.spent
    }

    /// Whether a long press is still possible for it.
    pub fn may_become_long_press(&self) -> bool {
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

    /// Records that it passed through `at` on its way to its next move, or lifted there, which
    /// counts towards its speed only.
    ///
    /// A queue that folds several moves into one keeps the positions of the folded ones, and each
    /// is a sample of the speed. A position equal to the newest one is not a sample, so the time
    /// since the contact last moved is the age of the newest sample.
    pub fn passed(&mut self, at: Point<CssPx, Css>, when: Timestamp) {
        // A touch screen samples a contact every 4 ms at the most often. Positions that arrive
        // less than 1 ms apart were held in a queue, and their stamps tell when the queue was
        // emptied: they are one position, at the first of those stamps.
        const QUEUED: Duration = Duration::from_millis(1);

        let queued = when.saturating_since(self.arrived) < QUEUED;
        self.arrived = when;
        if self.samples[self.newest].at == at {
            return;
        }
        if queued {
            self.samples[self.newest].at = at;
            return;
        }
        self.newest = (self.newest + 1) % SAMPLES;
        self.samples[self.newest] = Sample { at, when };
        self.filled = (self.filled + 1).min(SAMPLES);
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
    pub fn long_press_in(&self, now: Timestamp) -> Option<Duration> {
        self.may_become_long_press()
            .then(|| LONG_PRESS.saturating_sub(now.saturating_since(self.started)))
    }

    /// How fast it is travelling at `now`, from its recent positions.
    ///
    /// The speed is the slope of a weighted least-squares line through the positions since the
    /// contact last stopped, back to 100 ms before the newest one, and at least two of them. A
    /// contact that has not moved for 40 ms has no speed. A position far from the line is an
    /// outlier, and the line is fitted again without it.
    pub fn velocity(&self, now: Timestamp) -> Velocity {
        // A position farther from the line than this many times the spread of the fit is an
        // outlier.
        const OUTLIER: f32 = 3.0;
        // The least distance from the line, in CSS pixels, at which a position is an outlier. A
        // fit through positions on one line has no spread.
        const LEAST_OUTLIER: f32 = 1.0;

        let newest = self.samples[self.newest];
        if now.saturating_since(newest.when) > AT_REST {
            return Velocity::default();
        }

        let mut recent = [Weighted {
            at: newest.at,
            age: 0.0,
            weight: 0.0,
        }; SAMPLES];
        let recent = self.recent(&mut recent);
        let Some(fit) = Fit::through(recent.iter().copied()) else {
            return Velocity::default();
        };
        let tolerance = (OUTLIER * fit.spread).max(LEAST_OUTLIER);
        let kept = recent
            .iter()
            .copied()
            .filter(|position| fit.miss(position) <= tolerance);

        Fit::through(kept).unwrap_or(fit).velocity
    }

    /// Fills `into` with the positions a speed is measured from, newest first, and returns them.
    ///
    /// They are the positions back to the horizon, and at least two, so that a contact whose
    /// events stalled for longer than the horizon still has a speed. They stop at a gap of more
    /// than 40 ms over which the contact moved less than the slop: it stood still there.
    fn recent<'a>(&self, into: &'a mut [Weighted; SAMPLES]) -> &'a [Weighted] {
        // How far back the positions go.
        const HORIZON: Duration = Duration::from_millis(100);
        // The age at which a position counts half as much as the newest one. Short enough that a
        // slow drag that ended in a flick reads as a flick, and long enough that one late stamp
        // does not.
        const HALF_LIFE: Duration = Duration::from_millis(10);

        let decay = core::f32::consts::LN_2 / HALF_LIFE.as_secs_f32();
        let newest = self.samples[self.newest];
        let mut later = newest;
        let mut count = 0;
        for back in 0..self.filled {
            let sample = self.samples[(self.newest + SAMPLES - back) % SAMPLES];
            let age = newest.when.saturating_since(sample.when);
            if count >= 2 && age > HORIZON {
                break;
            }
            let held = later.when.saturating_since(sample.when) > AT_REST
                && !travelled(sample.at, later.at);
            if held {
                break;
            }
            let age = age.as_secs_f32();
            into[count] = Weighted {
                at: sample.at,
                age,
                weight: (-decay * age).exp(),
            };
            count += 1;
            later = sample;
        }

        &into[..count]
    }
}

/// A weighted least-squares line through positions against their ages.
#[derive(Clone, Copy, Debug)]
struct Fit {
    /// The slope of the line, forwards in time.
    velocity: Velocity,
    /// The weighted mean age, in seconds, at which the line passes through `centre`.
    age: f32,
    /// The weighted mean position.
    centre: Point<CssPx, Css>,
    /// The weighted root mean square distance of the positions from the line, in CSS pixels.
    spread: f32,
}

impl Fit {
    /// The line through `positions`. Nothing when they span less than a millisecond, over which
    /// no speed can be told.
    fn through(positions: impl Iterator<Item = Weighted> + Clone) -> Option<Self> {
        // The least span of the ages, in seconds.
        const LEAST_SPAN: f32 = 0.001;

        let mut weight = 0.0;
        let (mut age, mut x, mut y) = (0.0, 0.0, 0.0);
        let (mut newest, mut oldest) = (f32::INFINITY, f32::NEG_INFINITY);
        for position in positions.clone() {
            newest = newest.min(position.age);
            oldest = oldest.max(position.age);
            weight += position.weight;
            age += position.weight * position.age;
            x += position.weight * position.at.x.0;
            y += position.weight * position.at.y.0;
        }
        if weight <= 0.0 || oldest - newest < LEAST_SPAN {
            return None;
        }
        let (age, x, y) = (age / weight, x / weight, y / weight);

        let (mut ages, mut across, mut down) = (0.0, 0.0, 0.0);
        for position in positions.clone() {
            let off = position.age - age;
            ages += position.weight * off * off;
            across += position.weight * off * (position.at.x.0 - x);
            down += position.weight * off * (position.at.y.0 - y);
        }
        if ages <= 0.0 {
            return None;
        }
        // Age runs backwards in time, so the slope over age is the velocity reversed.
        let velocity = Velocity {
            x: -across / ages,
            y: -down / ages,
        };

        let mut fit = Self {
            velocity,
            age,
            centre: Point::new(CssPx(x), CssPx(y)),
            spread: 0.0,
        };
        let squares: f32 = positions
            .map(|position| position.weight * fit.miss(&position).powi(2))
            .sum();
        fit.spread = (squares / weight).sqrt();

        Some(fit)
    }

    /// How far, in CSS pixels, `position` is from the line.
    fn miss(&self, position: &Weighted) -> f32 {
        let later = self.age - position.age;
        let x = self.centre.x.0 + self.velocity.x * later;
        let y = self.centre.y.0 + self.velocity.y * later;
        (position.at.x.0 - x).hypot(position.at.y.0 - y)
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
    fn moves_stamped_together_still_read_the_speed() {
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
    fn a_burst_after_a_stall_longer_than_the_horizon_reads_the_speed_over_the_stall() {
        // A finger at 1500 px/s. Its samples from 48 to 240 ms wait for a busy thread, and arrive
        // 0.3 ms apart from 240 ms on.
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in 1..=5_u64 {
            track.moved(at(0.0, step as f32 * 12.0), stamp(step * 8));
        }
        let micros = |micros: u64| Timestamp::from_origin(Duration::from_micros(micros));
        let mut when = stamp(240);
        for (index, step) in (6..=30_u64).enumerate() {
            when = micros(240_000 + index as u64 * 300);
            track.moved(at(0.0, step as f32 * 12.0), when);
        }
        let velocity = track.velocity(when);
        assert!(
            (velocity.y - 1500.0).abs() < 1500.0 * 0.05,
            "a finger at 1500 px/s read {} px/s",
            velocity.y
        );
    }

    #[test]
    fn late_stamps_still_read_the_speed() {
        // A finger at 1500 px/s sampled every 8 ms, each sample stamped up to 7 ms late.
        let lateness = [0, 6, 1, 7, 0, 5, 7, 2, 0, 6, 3, 7, 1];
        for shift in 0..lateness.len() {
            let mut track = Track::started(at(0.0, 0.0), stamp(0));
            let mut when = stamp(0);
            for step in 1..=12_u64 {
                when = stamp(step * 8 + lateness[(step as usize + shift) % lateness.len()]);
                track.moved(at(0.0, step as f32 * 12.0), when);
            }
            let velocity = track.velocity(when);
            assert!(
                (velocity.y - 1500.0).abs() < 1500.0 * 0.15,
                "a finger at 1500 px/s read {} px/s",
                velocity.y
            );
        }
    }

    #[test]
    fn a_long_gap_in_which_the_finger_moved_counts() {
        // The samples between 40 and 88 ms arrive as one, at 88 ms.
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in (1..=5).chain(11..=13) {
            track.moved(at(0.0, step as f32 * 12.0), stamp(step * 8));
        }
        let velocity = track.velocity(stamp(104));
        assert!(
            (velocity.y - 1500.0).abs() < 1.0,
            "a finger at 1500 px/s read {} px/s",
            velocity.y
        );
    }

    #[test]
    fn a_position_far_off_the_line_is_left_out() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in 1..=12_u64 {
            let jump = if step == 10 { 25.0 } else { 0.0 };
            track.moved(at(0.0, step as f32 * 12.0 + jump), stamp(step * 8));
        }
        let velocity = track.velocity(stamp(96));
        assert!(
            (velocity.y - 1500.0).abs() < 1500.0 * 0.1,
            "a finger at 1500 px/s with one stray position read {} px/s",
            velocity.y
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
            (velocity.y - 2000.0).abs() < 2000.0 * 0.05,
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
    fn a_finger_that_stops_and_lifts_beside_where_it_stopped_has_no_speed() {
        let mut track = Track::started(at(0.0, 0.0), stamp(0));
        for step in 1..=6_u64 {
            track.moved(at(0.0, step as f32 * 12.0), stamp(step * 8));
        }
        // Held for 60 ms, then lifted half a pixel away.
        track.passed(at(0.0, 72.5), stamp(108));
        assert_eq!(track.velocity(stamp(108)), Velocity::default());
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
