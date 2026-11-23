//! The settings a user makes in the system that change how an application presents itself.

/// The number the user's text size setting multiplies text by.
///
/// One is the size the system uses by default. On iOS this is the Dynamic Type content size
/// category, as the ratio of its body text size to the body text size of the default category. A
/// platform with no such setting reports [`TextScale::DEFAULT`].
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct TextScale(f32);

impl TextScale {
    /// The scale of text at the size the system uses by default.
    pub const DEFAULT: Self = Self(1.0);

    /// The scale `factor`, when it is finite and larger than zero.
    pub fn new(factor: f32) -> Option<Self> {
        (factor.is_finite() && factor > 0.0).then_some(Self(factor))
    }

    /// The factor.
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for TextScale {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Whether the user asked the system to reduce motion.
///
/// A platform that cannot be asked reports [`MotionPreference::NoPreference`], which is also what
/// `prefers-reduced-motion` means when nothing is known.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MotionPreference {
    /// The user has expressed no preference.
    #[default]
    NoPreference,
    /// The user asked for less motion.
    Reduce,
}

#[cfg(test)]
mod tests {
    use super::TextScale;

    #[test]
    fn a_text_scale_is_finite_and_positive() {
        assert_eq!(TextScale::new(0.0), None);
        assert_eq!(TextScale::new(f32::INFINITY), None);
        assert_eq!(TextScale::new(1.25).map(TextScale::get), Some(1.25));
        assert_eq!(TextScale::default(), TextScale::DEFAULT);
    }
}
