//! The return that carries a displaced edge back to its end.
//!
//! Two curves, one for each kind of input. Each is a function of the time since the return
//! started, so the frame rate cannot change its shape.
//!
//! * [`Curve::Decay`] is the one AppKit and WebKit use for a touchpad: `(x0 + A*v0*t) * e^(-w*t)`.
//!   With no speed it is an exponential decay, so the edge starts back fast and slows as it
//!   arrives. With a speed it first travels on past the end, then comes back.
//! * [`Curve::Spring`] is the bounce of UIScrollView for a finger on a screen: a critically
//!   damped spring, `(x0 + (v0 + w*x0)*t) * e^(-w*t)`. A released edge starts back from rest and
//!   eases in. A thrown edge travels on past the end, then comes back without a second bounce.

/// How fast the return decays, in radians per second.
///
/// AppKit's rubber-band stiffness of 20 over its period of 1.6. An edge released 100 device pixels
/// out is within 2 pixels after about 0.3 seconds.
const RATE: f32 = 12.5;

/// How much of a thrown speed becomes travel past the end.
///
/// AppKit's rubber-band amplitude. A momentum scroll at 3000 device pixels per second that reaches
/// an end travels about 27 pixels past it.
const AMPLITUDE: f32 = 0.31;

/// How fast UIScrollView's spring returns, in radians per second.
///
/// A critically damped spring with a mass of 1 and a stiffness of 100, so `sqrt(100 / 1)`. An edge
/// released 100 device pixels out is within 2 pixels after about 0.6 seconds.
const SPRING_RATE: f32 = 10.0;

/// Below this the displacement has arrived, in device pixels.
///
/// A quarter of a device pixel is below what the device-grid snap can show, so the last step is
/// invisible. Without a floor the return never reaches zero and asks for a frame for ever.
const ARRIVED: f32 = 0.25;

/// Below this the edge is still, in device pixels per second.
const STILL: f32 = 4.0;

/// The curve a return follows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Curve {
    /// AppKit's rubber band, for a touchpad.
    #[default]
    Decay,
    /// UIScrollView's critically damped spring, for a finger on a screen.
    Spring,
}

/// One axis of a return: where it started, the speed it started at, and how long it has run.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Return {
    /// The displacement when the return started, in device pixels.
    from: f32,
    /// The speed away from the end when the return started, in device pixels per second.
    thrown: f32,
    /// The time since the return started, in seconds.
    elapsed: f32,
    /// The curve it follows.
    curve: Curve,
}

impl Return {
    /// A touchpad's return that starts at `from`, moving at `thrown`.
    pub(crate) fn start(from: f32, thrown: f32) -> Self {
        Self {
            from,
            thrown,
            elapsed: 0.0,
            curve: Curve::Decay,
        }
    }

    /// A finger's return that starts at `from`, moving at `thrown`.
    pub(crate) fn spring(from: f32, thrown: f32) -> Self {
        Self {
            curve: Curve::Spring,
            ..Self::start(from, thrown)
        }
    }

    /// The displacement now.
    pub(crate) fn at(self) -> f32 {
        let t = self.elapsed;
        match self.curve {
            Curve::Decay => (self.from + AMPLITUDE * self.thrown * t) * (-RATE * t).exp(),
            Curve::Spring => {
                let lead = self.thrown + SPRING_RATE * self.from;
                (self.from + lead * t) * (-SPRING_RATE * t).exp()
            }
        }
    }

    /// The speed now, in device pixels per second.
    fn speed(self) -> f32 {
        let t = self.elapsed;
        match self.curve {
            Curve::Decay => {
                let lead = AMPLITUDE * self.thrown;
                (lead - RATE * (self.from + lead * t)) * (-RATE * t).exp()
            }
            Curve::Spring => {
                let lead = self.thrown + SPRING_RATE * self.from;
                (lead - SPRING_RATE * (self.from + lead * t)) * (-SPRING_RATE * t).exp()
            }
        }
    }

    /// The same return, `seconds` later.
    pub(crate) fn advanced(self, seconds: f32) -> Self {
        let advanced = Self {
            elapsed: self.elapsed + seconds,
            ..self
        };
        if advanced.arrived() {
            Self::default()
        } else {
            advanced
        }
    }

    /// The same return measured in units `by` times smaller.
    pub(crate) fn scaled(self, by: f32) -> Self {
        Self {
            from: self.from * by,
            thrown: self.thrown * by,
            ..self
        }
    }

    /// Whether the edge is back at its end.
    ///
    /// An edge near zero on its way out has not arrived: it must also move back towards zero, or
    /// be still.
    pub(crate) fn arrived(self) -> bool {
        let at = self.at();
        let speed = self.speed();
        at.abs() < ARRIVED && (at * speed < 0.0 || speed.abs() < STILL)
    }
}

#[cfg(test)]
mod tests {
    use super::Return;

    /// One frame at sixty hertz, in seconds.
    const FRAME: f32 = 1.0 / 60.0;

    /// The displacements of a return at sixty hertz, until it arrives.
    fn run(mut edge: Return) -> Vec<f32> {
        let mut seen = vec![edge.at()];
        while !edge.arrived() && seen.len() < 600 {
            edge = edge.advanced(FRAME);
            seen.push(edge.at());
        }
        seen
    }

    #[test]
    fn a_released_edge_comes_back_in_about_half_a_second() {
        let seen = run(Return::start(100.0, 0.0));
        let frames = seen.len() - 1;
        assert!(
            (20..=40).contains(&frames),
            "the return took {frames} frames at sixty hertz"
        );
        let visible = seen.iter().position(|at| at.abs() < 2.0).expect("arrived");
        assert!(
            (15..=22).contains(&visible),
            "the edge was within two pixels after {visible} frames"
        );
    }

    #[test]
    fn a_released_edge_starts_back_fast_and_never_crosses_its_end() {
        let seen = run(Return::start(100.0, 0.0));
        assert!(seen[1] < 85.0, "the first frame moved only to {}", seen[1]);
        assert!(
            seen.windows(2)
                .all(|pair| pair[1] <= pair[0] && pair[1] >= 0.0),
            "{seen:?}"
        );
    }

    #[test]
    fn a_thrown_edge_travels_past_its_end_then_comes_back() {
        let seen = run(Return::start(0.0, 3000.0));
        let peak = seen.iter().copied().fold(0.0, f32::max);
        assert!(
            (15.0..=40.0).contains(&peak),
            "a throw at 3000 pixels per second travelled {peak}"
        );
        assert_eq!(*seen.last().expect("frames"), 0.0, "it never came back");
        assert!(seen.len() < 60, "the bounce took {} frames", seen.len());
    }

    #[test]
    fn the_frame_rate_does_not_change_where_the_edge_is() {
        let whole = Return::start(80.0, 0.0).advanced(0.048);
        let mut steps = Return::start(80.0, 0.0);
        for _ in 0..6 {
            steps = steps.advanced(0.008);
        }
        assert!((whole.at() - steps.at()).abs() < 1e-3);
    }

    #[test]
    fn a_long_frame_brings_the_edge_home_rather_than_throwing_it() {
        let after = Return::start(120.0, 0.0).advanced(2.0);
        assert_eq!(after.at(), 0.0);
        assert!(after.arrived());
    }

    #[test]
    fn a_finger_lets_go_of_an_edge_that_follows_the_critically_damped_spring() {
        for millis in [0_u16, 50, 100, 200, 400] {
            let t = f32::from(millis) / 1000.0;
            let edge = Return::spring(100.0, 0.0).advanced(t);
            let expected = (100.0 + 1000.0 * t) * (-10.0 * t).exp();
            assert!(
                (edge.at() - expected).abs() < 1e-3,
                "at {millis} ms the edge was at {}, the spring at {expected}",
                edge.at()
            );
        }
    }

    #[test]
    fn a_finger_lets_go_of_an_edge_that_eases_back_and_never_crosses_its_end() {
        let seen = run(Return::spring(100.0, 0.0));
        assert!(seen[1] > 98.0, "the first frame jumped to {}", seen[1]);
        assert!(
            seen.windows(2)
                .all(|pair| pair[1] <= pair[0] && pair[1] >= 0.0),
            "{seen:?}"
        );
        let frames = seen.len() - 1;
        assert!(
            (36..=60).contains(&frames),
            "the return took {frames} frames at sixty hertz"
        );
    }

    #[test]
    fn a_fling_that_reaches_the_end_peaks_where_the_spring_says() {
        // A spring thrown from its rest point peaks at `v / (w * e)`, after `1 / w` seconds.
        let peak = Return::spring(0.0, 3000.0).advanced(0.1).at();
        let expected = 3000.0 / (10.0 * core::f32::consts::E);
        assert!((peak - expected).abs() < 1e-2, "{peak} against {expected}");
        let seen = run(Return::spring(0.0, 3000.0));
        assert!(seen.iter().all(|at| *at >= 0.0), "it bounced twice");
        assert_eq!(*seen.last().expect("frames"), 0.0, "it never came back");
    }
}
