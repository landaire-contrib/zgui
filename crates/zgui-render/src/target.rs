//! Where a frame is composed.

use zgui_geom::{Css, Device, Scale, Size};

/// The surface a renderer draws for.
///
/// It is a description rather than a resource: the extent to compose at, and the ratio between the
/// coordinates an author writes and the pixels the output has. A renderer owns whatever resources
/// that implies and reallocates them when this changes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderTarget {
    /// The extent to compose at, in device pixels.
    pub size: Size<i32, Device>,
    /// How many device pixels one coordinate-space pixel is.
    pub scale: Scale<Css, Device>,
    /// How many device pixels one of the platform's own CSS pixels is: `scale` without the zoom
    /// the window draws its content at. What the platform declares the buffer at.
    pub surface_scale: Scale<Css, Device>,
    /// Whether the surface is opaque.
    ///
    /// It decides more than it looks like it does: subpixel-antialiased text writes per-channel
    /// coverage and no alpha, which is meaningless against a destination that is not opaque, so a
    /// translucent surface has to be drawn with ordinary coverage throughout.
    pub opaque: bool,
}

impl RenderTarget {
    /// A target of `size` at `scale`, opaque, on a surface at that same scale.
    pub fn new(size: Size<i32, Device>, scale: Scale<Css, Device>) -> Self {
        Self {
            size,
            scale,
            surface_scale: scale,
            opaque: true,
        }
    }

    /// The same target on a surface at `surface_scale`, whose content is zoomed to `scale`.
    pub fn on_surface_at(mut self, surface_scale: Scale<Css, Device>) -> Self {
        self.surface_scale = surface_scale;
        self
    }

    /// The same target, translucent.
    pub fn translucent(mut self) -> Self {
        self.opaque = false;
        self
    }

    /// How many device pixels the whole surface holds.
    pub fn area(&self) -> u64 {
        self.size.width.max(0) as u64 * self.size.height.max(0) as u64
    }
}
