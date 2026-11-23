//! The part of the window that an on-screen keyboard covers, how the keyboard moves, and how a
//! change reaches `env()`.
//!
//! The keyboard area is a level, like the safe area. The platform reports it in device pixels, and
//! the viewport holds it in CSS pixels for the style engine. The window keeps the device pixel
//! value, because a change of scale converts it again.

use zgui_geom::{Css, CssPx, Device, DevicePx, Rect, Scale};

use crate::window::Window;
use crate::window::preferences::keyboard_motion_from_platform;

impl Window {
    /// Reads the keyboard area and motion from the surface, and reports whether either moved.
    ///
    /// A platform sends a resize when the keyboard area changes, also when the size stays the
    /// same, so every resize reads it.
    pub(crate) fn take_virtual_keyboard(&mut self) -> bool {
        let area = self.surface.virtual_keyboard();
        let motion = self
            .surface
            .virtual_keyboard_motion()
            .and_then(keyboard_motion_from_platform);
        if area == self.virtual_keyboard && motion == self.viewport.keyboard_motion {
            return false;
        }

        self.virtual_keyboard = area;
        self.viewport = self
            .viewport
            .with_virtual_keyboard(css_area(area, self.scale))
            .with_keyboard_motion(motion);

        true
    }

    /// The keyboard area in the CSS pixels of a window at `scale` device pixels per CSS pixel.
    pub(crate) fn virtual_keyboard_at(&self, scale: f32) -> Option<Rect<CssPx, Css>> {
        css_area(self.virtual_keyboard, scale)
    }
}

/// `area` in the CSS pixels of a window at `scale` device pixels per CSS pixel.
pub(crate) fn css_area(
    area: Option<Rect<DevicePx, Device>>,
    scale: f32,
) -> Option<Rect<CssPx, Css>> {
    let to_css: Scale<Device, Css> = Scale::new(1.0 / scale);

    area.map(|area| area * to_css)
}
