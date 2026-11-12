//! How much a window scales its content.

use crate::error::ZoomOutOfRange;

/// A window zoom factor.
///
/// Zoom multiplies the platform scale factor. At zoom 1.5 on a display with scale 2, one CSS pixel
/// is three device pixels. A factor above one enlarges the content, and a factor below one reduces
/// it. The CSS viewport changes by the inverse of the factor, so layout and media queries see the
/// new width, and text is rasterised at the new size.
///
/// ```
/// use zgui_runtime::windows::Zoom;
///
/// let zoom = Zoom::new(1.5).expect("1.5 is in range");
/// assert_eq!(zoom.get(), 1.5);
/// assert_eq!(Zoom::default(), Zoom::ONE);
/// assert!(Zoom::new(0.0).is_err());
/// assert!(Zoom::new(f32::NAN).is_err());
/// ```
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct Zoom(f32);

impl Zoom {
    /// No zoom.
    pub const ONE: Self = Self(1.0);

    /// The smallest zoom accepted.
    ///
    /// The range is Chromium's page zoom range, 25% to 500%, which users already know. The engine
    /// itself needs only a finite factor above zero, because pointer positions are divided by it.
    /// Past either end the page stops being usable: at 0.25 and a platform scale of 1, 16px text
    /// is 4 device pixels tall, and at 5 a window 1280 device pixels wide lays out 256 CSS pixels
    /// wide.
    pub const MIN: Self = Self(0.25);

    /// The largest zoom accepted. See [`Zoom::MIN`] for why the range is what it is.
    pub const MAX: Self = Self(5.0);

    /// A zoom of `factor`.
    ///
    /// # Errors
    ///
    /// Returns [`ZoomOutOfRange`] when `factor` is not in the range [`Zoom::MIN`] to
    /// [`Zoom::MAX`], or is not a number.
    pub fn new(factor: f32) -> Result<Self, ZoomOutOfRange> {
        if (Self::MIN.0..=Self::MAX.0).contains(&factor) {
            return Ok(Self(factor));
        }

        Err(ZoomOutOfRange {
            factor,
            min: Self::MIN.0,
            max: Self::MAX.0,
        })
    }

    /// The factor.
    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Default for Zoom {
    fn default() -> Self {
        Self::ONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_range_limits_are_accepted() {
        assert_eq!(Zoom::new(0.25), Ok(Zoom::MIN));
        assert_eq!(Zoom::new(5.0), Ok(Zoom::MAX));
    }

    #[test]
    fn a_factor_outside_the_range_reports_the_range() {
        assert_eq!(
            Zoom::new(5.5),
            Err(ZoomOutOfRange {
                factor: 5.5,
                min: 0.25,
                max: 5.0
            })
        );
        assert!(Zoom::new(-1.0).is_err());
        assert!(Zoom::new(f32::INFINITY).is_err());
    }
}
