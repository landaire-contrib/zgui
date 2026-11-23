//! The two axes a container scrolls along.

use zgui_geom::{Device, DevicePx, Point, Size};

/// One axis of a scroll.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Axis {
    /// The horizontal axis.
    Across,
    /// The vertical axis.
    Down,
}

impl Axis {
    /// Both axes.
    pub(crate) const BOTH: [Self; 2] = [Self::Across, Self::Down];

    /// The component of `point` along this axis.
    pub(crate) fn of_point(self, point: Point<DevicePx, Device>) -> f32 {
        match self {
            Self::Across => point.x.0,
            Self::Down => point.y.0,
        }
    }

    /// The component of `size` along this axis.
    pub(crate) fn of_size(self, size: Size<DevicePx, Device>) -> f32 {
        match self {
            Self::Across => size.width.0,
            Self::Down => size.height.0,
        }
    }
}
