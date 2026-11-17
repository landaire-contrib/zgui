//! How soon and how near a second press must follow the first to count with it.

use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};

use crate::time::Timestamp;

/// The desktop's double-click time and distance.
///
/// The platform backend supplies these from the desktop setting where the desktop has one. The
/// [`desktop`](ClickSettings::desktop) values apply where it has none.
///
/// ```
/// use std::time::Duration;
/// use zgui_geom::{CssPx, Point};
/// use zgui_vocab::{ClickSettings, Timestamp};
///
/// let settings = ClickSettings::desktop();
/// let first = (Timestamp::ORIGIN, Point::new(CssPx(10.0), CssPx(10.0)));
/// let soon = (Timestamp::ORIGIN + Duration::from_millis(200), Point::new(CssPx(12.0), CssPx(10.0)));
/// assert!(settings.follows(first, soon));
/// ```
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct ClickSettings {
    /// The longest time from one press to the next that still counts them together.
    pub interval: Duration,
    /// The farthest the next press may land from the last one, on each axis, in CSS pixels.
    pub slop: CssPx,
}

impl ClickSettings {
    /// 500 ms and 4 CSS pixels, for a desktop that states no setting.
    pub const fn desktop() -> Self {
        Self {
            interval: Duration::from_millis(500),
            slop: CssPx(4.0),
        }
    }

    /// These settings with the time between presses set to `interval`.
    #[must_use]
    pub const fn with_interval(mut self, interval: Duration) -> Self {
        self.interval = interval;
        self
    }

    /// These settings with the distance between presses set to `slop`.
    #[must_use]
    pub const fn with_slop(mut self, slop: CssPx) -> Self {
        self.slop = slop;
        self
    }

    /// Whether a press at `when` and `at` follows the press at `then` and `there` soon enough and
    /// near enough.
    pub fn follows(
        &self,
        (then, there): (Timestamp, Point<CssPx, Css>),
        (when, at): (Timestamp, Point<CssPx, Css>),
    ) -> bool {
        when.saturating_since(then) <= self.interval
            && (at.x.0 - there.x.0).abs() <= self.slop.0
            && (at.y.0 - there.y.0).abs() <= self.slop.0
    }
}

impl Default for ClickSettings {
    fn default() -> Self {
        Self::desktop()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use zgui_geom::{CssPx, Point};

    use super::ClickSettings;
    use crate::time::Timestamp;

    fn press(ms: u64, x: f32) -> (Timestamp, Point<zgui_geom::CssPx, zgui_geom::Css>) {
        (
            Timestamp::ORIGIN + Duration::from_millis(ms),
            Point::new(CssPx(x), CssPx(0.0)),
        )
    }

    #[test]
    fn a_longer_desktop_interval_counts_a_slower_second_press() {
        let fallback = ClickSettings::desktop();
        let slow = fallback.with_interval(Duration::from_millis(900));
        assert!(!fallback.follows(press(0, 0.0), press(800, 0.0)));
        assert!(slow.follows(press(0, 0.0), press(800, 0.0)));
    }

    #[test]
    fn a_wider_desktop_distance_counts_a_farther_second_press() {
        let fallback = ClickSettings::desktop();
        let wide = fallback.with_slop(CssPx(8.0));
        assert!(!fallback.follows(press(0, 0.0), press(10, 6.0)));
        assert!(wide.follows(press(0, 0.0), press(10, 6.0)));
    }
}
