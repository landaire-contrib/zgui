//! How an on-screen keyboard moves.

use core::time::Duration;

/// How an on-screen keyboard moves to the area it covers next.
///
/// Stylesheets read it as `env(keyboard-animation-duration)` and `env(keyboard-animation-easing)`,
/// so that content moves with the keyboard.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeyboardMotion {
    /// The time the movement takes.
    pub duration: Duration,
    /// The curve of the movement.
    pub easing: KeyboardEasing,
}

/// The curve an on-screen keyboard moves along.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum KeyboardEasing {
    /// A curve CSS `cubic-bezier()` gives.
    CubicBezier(CubicBezier),
    /// A spring released from rest at the start of the movement.
    Spring(Spring),
}

/// An easing curve, by the two control points of CSS `cubic-bezier()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    /// The horizontal position of the first control point, from 0 to 1.
    pub x1: f32,
    /// The vertical position of the first control point.
    pub y1: f32,
    /// The horizontal position of the second control point, from 0 to 1.
    pub x2: f32,
    /// The vertical position of the second control point.
    pub y2: f32,
}

/// A damped spring, by the coefficients of its equation of motion.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spring {
    /// The mass on the spring.
    pub mass: f32,
    /// The stiffness of the spring.
    pub stiffness: f32,
    /// The damping coefficient.
    pub damping: f32,
}
