//! What the values UIKit reports mean, apart from the calls that read them.

use core::time::Duration;

use zgui_platform::{CubicBezier, KeyboardEasing, KeyboardMotion, Spring, TextScale};

/// A Dynamic Type text size, from the smallest to the largest accessibility size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ContentSizeCategory {
    /// `UIContentSizeCategoryExtraSmall`.
    ExtraSmall,
    /// `UIContentSizeCategorySmall`.
    Small,
    /// `UIContentSizeCategoryMedium`.
    Medium,
    /// `UIContentSizeCategoryLarge`, the size the system uses by default.
    Large,
    /// `UIContentSizeCategoryExtraLarge`.
    ExtraLarge,
    /// `UIContentSizeCategoryExtraExtraLarge`.
    ExtraExtraLarge,
    /// `UIContentSizeCategoryExtraExtraExtraLarge`.
    ExtraExtraExtraLarge,
    /// `UIContentSizeCategoryAccessibilityMedium`.
    AccessibilityMedium,
    /// `UIContentSizeCategoryAccessibilityLarge`.
    AccessibilityLarge,
    /// `UIContentSizeCategoryAccessibilityExtraLarge`.
    AccessibilityExtraLarge,
    /// `UIContentSizeCategoryAccessibilityExtraExtraLarge`.
    AccessibilityExtraExtraLarge,
    /// `UIContentSizeCategoryAccessibilityExtraExtraExtraLarge`.
    AccessibilityExtraExtraExtraLarge,
}

impl ContentSizeCategory {
    /// Every category, from the smallest to the largest.
    #[cfg(test)]
    const ALL: [Self; 12] = [
        Self::ExtraSmall,
        Self::Small,
        Self::Medium,
        Self::Large,
        Self::ExtraLarge,
        Self::ExtraExtraLarge,
        Self::ExtraExtraExtraLarge,
        Self::AccessibilityMedium,
        Self::AccessibilityLarge,
        Self::AccessibilityExtraLarge,
        Self::AccessibilityExtraExtraLarge,
        Self::AccessibilityExtraExtraExtraLarge,
    ];

    /// The size of the body text style in this category, in points.
    const fn body_points(self) -> u8 {
        match self {
            Self::ExtraSmall => 14,
            Self::Small => 15,
            Self::Medium => 16,
            Self::Large => 17,
            Self::ExtraLarge => 19,
            Self::ExtraExtraLarge => 21,
            Self::ExtraExtraExtraLarge => 23,
            Self::AccessibilityMedium => 28,
            Self::AccessibilityLarge => 33,
            Self::AccessibilityExtraLarge => 40,
            Self::AccessibilityExtraExtraLarge => 47,
            Self::AccessibilityExtraExtraExtraLarge => 53,
        }
    }

    /// The ratio of the body text size in this category to the one in the default category.
    pub(crate) fn text_scale(self) -> TextScale {
        TextScale::new(f32::from(self.body_points()) / f32::from(Self::Large.body_points()))
            .expect("every body size is positive")
    }
}

/// The `UIViewAnimationCurve` the keyboard notifications state for the system keyboard.
///
/// It is outside the public enumeration. UIKit animates it with a spring.
const KEYBOARD_CURVE: isize = 7;

/// How the on-screen keyboard moves, from the duration in seconds and the `UIViewAnimationCurve`
/// of a keyboard notification.
///
/// `None` for a curve UIKit does not document.
pub(crate) fn keyboard_motion(duration: f64, curve: isize) -> Option<KeyboardMotion> {
    let bezier = |x1, y1, x2, y2| KeyboardEasing::CubicBezier(CubicBezier { x1, y1, x2, y2 });
    let easing = match curve {
        // UIViewAnimationCurveEaseInOut, EaseIn, EaseOut and Linear, with the control points
        // of the CSS keywords of the same names.
        0 => bezier(0.42, 0.0, 0.58, 1.0),
        1 => bezier(0.42, 0.0, 1.0, 1.0),
        2 => bezier(0.0, 0.0, 0.58, 1.0),
        3 => bezier(0.0, 0.0, 1.0, 1.0),
        // UIKit builds this spring with a damping of 500, and Core Animation limits that to
        // critical damping, 2 * sqrt(3 * 1000). The spring settles in 0.5 s, whatever duration the
        // notification states.
        KEYBOARD_CURVE => {
            return Some(KeyboardMotion {
                duration: Duration::from_millis(500),
                easing: KeyboardEasing::Spring(Spring {
                    mass: 3.0,
                    stiffness: 1000.0,
                    damping: 109.54,
                }),
            });
        }
        _ => return None,
    };
    Some(KeyboardMotion {
        duration: Duration::try_from_secs_f64(duration).ok()?,
        easing,
    })
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use zgui_platform::{KeyboardEasing, TextScale};

    use super::{ContentSizeCategory, keyboard_motion};

    #[test]
    fn the_default_category_has_the_default_scale() {
        assert_eq!(ContentSizeCategory::Large.text_scale(), TextScale::DEFAULT);
    }

    #[test]
    fn the_scale_grows_with_each_category() {
        let scales = ContentSizeCategory::ALL.map(|category| category.text_scale().get());
        assert!(
            scales.windows(2).all(|pair| pair[0] < pair[1]),
            "{scales:?}"
        );
        assert_eq!(scales[0], 14.0 / 17.0);
        assert_eq!(scales[11], 53.0 / 17.0);
    }

    #[test]
    fn the_keyboard_spring_takes_half_a_second_whatever_the_notification_states() {
        let motion = keyboard_motion(0.25, 7).expect("the keyboard curve is known");
        assert_eq!(motion.duration, Duration::from_millis(500));
        let KeyboardEasing::Spring(spring) = motion.easing else {
            panic!("the keyboard curve is a spring: {motion:?}");
        };
        let critical = 2.0 * (spring.mass * spring.stiffness).sqrt();
        assert!((spring.damping - critical).abs() < 0.01, "{spring:?}");
    }

    #[test]
    fn a_documented_curve_keeps_the_stated_duration() {
        let motion = keyboard_motion(0.25, 0).expect("ease in and out is documented");
        assert_eq!(motion.duration, Duration::from_millis(250));
        assert!(matches!(motion.easing, KeyboardEasing::CubicBezier(_)));
    }

    #[test]
    fn an_undocumented_curve_or_duration_states_no_motion() {
        assert_eq!(keyboard_motion(0.25, 5), None);
        assert_eq!(keyboard_motion(-1.0, 0), None);
    }
}
