//! How far the system interface covers each edge of the window, and how a change reaches `env()`.
//!
//! The safe area is a level, like the extent. The platform reports it in device pixels, and the
//! viewport holds it in CSS pixels for the style engine. The window keeps the device pixel value,
//! because a change of scale converts it again.

use zgui_geom::{CssPx, DevicePx, Edges};

use crate::window::Window;

impl Window {
    /// Reads the safe area from the surface, and reports whether it moved.
    ///
    /// A platform sends a resize when the safe area changes, also when the size stays the same, so
    /// every resize reads it.
    pub(crate) fn take_safe_area(&mut self) -> bool {
        let safe_area = self.surface.safe_area();
        if safe_area == self.safe_area {
            return false;
        }

        self.safe_area = safe_area;
        self.viewport = self
            .viewport
            .with_safe_area(css_safe_area(safe_area, self.scale));

        true
    }

    /// The safe area in the CSS pixels of a window at `scale` device pixels per CSS pixel.
    pub(crate) fn safe_area_at(&self, scale: f32) -> Edges<CssPx> {
        css_safe_area(self.safe_area, scale)
    }
}

/// `safe_area` in the CSS pixels of a window at `scale` device pixels per CSS pixel.
pub(crate) fn css_safe_area(safe_area: Edges<DevicePx>, scale: f32) -> Edges<CssPx> {
    safe_area.map(|inset| CssPx(inset.0 / scale))
}
