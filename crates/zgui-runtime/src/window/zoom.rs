//! The zoom a window draws its content at.
//!
//! The zoom multiplies the platform scale factor, so every conversion from CSS pixels to device
//! pixels in the window uses the product. A change of zoom is therefore a change of that product,
//! and it takes the same path as a change of the platform scale.
//!
//! The platform measures input in its own CSS pixels, which do not include the zoom. Input is
//! moved into the document's CSS pixels where it is dispatched.

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{DragEvent, SurfaceEvent};
use zgui_vocab::ScrollDelta;

use crate::window::Window;
use crate::window::input::Queued;
use crate::windows::Zoom;

impl Window {
    /// How much the window scales its content.
    pub fn zoom(&self) -> Zoom {
        self.zoom
    }

    /// Scales the content by `zoom`, and reports whether the window wants a frame for it now.
    ///
    /// The layout is invalidated in full, scroll offsets are rescaled, the cascade's device is
    /// rebuilt and the renderer is reconfigured, as for a change of the platform scale.
    pub fn set_zoom(&mut self, zoom: Zoom) -> bool {
        if zoom == self.zoom {
            return false;
        }

        self.zoom = zoom;
        self.handle.note_zoom(zoom);

        self.resized(self.surface.size(), self.platform_scale)
    }

    /// Moves the positions `queued` carries from the platform's CSS pixels to the document's.
    pub(crate) fn unzoom(&self, queued: &mut Queued) {
        let zoom = self.zoom.get();
        let unzoom = |point: &mut Point<CssPx, Css>| {
            point.x.0 /= zoom;
            point.y.0 /= zoom;
        };

        for sample in &mut queued.samples {
            unzoom(&mut sample.position);
        }

        match &mut queued.event {
            SurfaceEvent::Pointer { event, .. } => unzoom(&mut event.position),
            SurfaceEvent::Wheel { event, .. } => {
                unzoom(&mut event.position);
                // A pixel delta follows the fingers on a touchpad, so it is a distance on the
                // display. A line delta is measured in the document's own line height.
                if let ScrollDelta::Pixels(pixels) = &mut event.delta {
                    pixels.width.0 /= zoom;
                    pixels.height.0 /= zoom;
                }
            }
            // The scale delta is a ratio, which a zoom does not change.
            SurfaceEvent::Pinch { event, .. } => unzoom(&mut event.position),
            SurfaceEvent::Drag(
                DragEvent::Entered { position, .. }
                | DragEvent::Moved { position }
                | DragEvent::Dropped { position, .. },
            ) => unzoom(position),
            // Every other event carries no position.
            _ => {}
        }
    }
}
