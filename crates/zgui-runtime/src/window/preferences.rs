//! The user's text size setting and motion preference, and how a change reaches the cascade.
//!
//! Both are levels the platform reports for the whole application. The window keeps them in its
//! viewport: the text scale reaches `env(preferred-text-scale)`, and the motion preference decides
//! what a `prefers-reduced-motion` query answers.

use crate::window::Window;

impl Window {
    /// Styles this window for the user's `text_scale` and `motion` preference, and reports whether
    /// either moved.
    pub(crate) fn set_platform_preferences(
        &mut self,
        text_scale: zgui_platform::TextScale,
        motion: zgui_platform::MotionPreference,
    ) -> bool {
        let next = self
            .viewport
            .with_text_scale(text_scale_from_platform(text_scale))
            .with_motion(motion_from_platform(motion));
        if next == self.viewport {
            return false;
        }

        self.viewport = next;

        true
    }
}

/// The style engine's spelling of the platform's text scale.
fn text_scale_from_platform(scale: zgui_platform::TextScale) -> zgui_style::TextScale {
    zgui_style::TextScale::new(scale.get())
        .expect("both types hold a finite factor larger than zero")
}

/// The style engine's spelling of the platform's motion preference.
fn motion_from_platform(motion: zgui_platform::MotionPreference) -> zgui_style::MotionPreference {
    match motion {
        zgui_platform::MotionPreference::Reduce => zgui_style::MotionPreference::Reduce,
        _ => zgui_style::MotionPreference::NoPreference,
    }
}

/// The style engine's spelling of how the platform's keyboard moved, or `None` for a curve this
/// build does not know.
pub(crate) fn keyboard_motion_from_platform(
    motion: zgui_platform::KeyboardMotion,
) -> Option<zgui_style::KeyboardMotion> {
    let easing = match motion.easing {
        zgui_platform::KeyboardEasing::CubicBezier(zgui_platform::CubicBezier {
            x1,
            y1,
            x2,
            y2,
        }) => zgui_style::KeyboardEasing::CubicBezier(zgui_style::CubicBezier { x1, y1, x2, y2 }),
        zgui_platform::KeyboardEasing::Spring(zgui_platform::Spring {
            mass,
            stiffness,
            damping,
        }) => zgui_style::KeyboardEasing::Spring(zgui_style::Spring {
            mass,
            stiffness,
            damping,
        }),
        _ => return None,
    };

    Some(zgui_style::KeyboardMotion {
        duration: motion.duration,
        easing,
    })
}

#[cfg(test)]
mod tests {
    use super::motion_from_platform;
    use super::text_scale_from_platform;

    #[test]
    fn both_preferences_survive_the_crossing() {
        assert_eq!(
            motion_from_platform(zgui_platform::MotionPreference::Reduce),
            zgui_style::MotionPreference::Reduce
        );
        assert_eq!(
            motion_from_platform(zgui_platform::MotionPreference::NoPreference),
            zgui_style::MotionPreference::NoPreference
        );
        let scale = zgui_platform::TextScale::new(1.25).expect("a positive scale");
        assert_eq!(text_scale_from_platform(scale).get(), 1.25);
    }
}
