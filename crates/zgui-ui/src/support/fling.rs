//! The curve of a motion that continues a finger's movement.

/// A `cubic-bezier()` for a motion of `distance` CSS pixels over `duration` seconds that starts at
/// `velocity` CSS pixels per second.
///
/// A `cubic-bezier()` starts at the slope of its first control point, so that point is set to match
/// `velocity`. `None` when the finger moves the other way or the motion is too short to matter: the
/// caller then uses its usual curve.
pub(crate) fn fling_ease(distance: f32, velocity: f32, duration: f32) -> Option<String> {
    const FIRST_X: f32 = 0.25;
    // Steeper than 1 overshoots the end.
    const STEEPEST_Y: f32 = 1.0;

    if distance.abs() < 1.0 || velocity * distance <= 0.0 {
        return None;
    }

    let slope = velocity.abs() * duration / distance.abs();
    let first_y = (slope * FIRST_X).min(STEEPEST_Y);

    Some(format!("cubic-bezier({FIRST_X}, {first_y:.3}, 0.25, 1)"))
}

#[cfg(test)]
mod tests {
    use super::fling_ease;

    #[test]
    fn the_curve_starts_at_the_speed_of_the_finger() {
        // 200 pixels in half a second at 400 pixels per second is a slope of one.
        assert_eq!(
            fling_ease(200.0, 400.0, 0.5).as_deref(),
            Some("cubic-bezier(0.25, 0.250, 0.25, 1)")
        );
        assert_eq!(
            fling_ease(-200.0, -400.0, 0.5).as_deref(),
            Some("cubic-bezier(0.25, 0.250, 0.25, 1)")
        );
    }

    #[test]
    fn a_finger_moving_the_other_way_gives_no_curve() {
        assert_eq!(fling_ease(200.0, -400.0, 0.5), None);
        assert_eq!(fling_ease(0.5, 400.0, 0.5), None);
    }
}
