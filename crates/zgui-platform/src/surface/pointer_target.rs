//! The element under a hovering pointer, for a platform pointer that changes its shape to fit it.

use zgui_geom::{Device, DevicePx, Point, Rect};

/// The element a hovering pointer is over, as a platform pointer takes its shape from it.
///
/// iPadOS changes its pointer into a highlight over a control and into a text beam over text. The
/// platform asks for the area under the pointer as the pointer moves. A surface keeps the last
/// target it got and answers from it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerTarget {
    /// Which element this is. A different element has a different identifier.
    pub id: PointerTargetId,
    /// The border box of the element, in physical pixels from the top-left corner of the surface.
    pub area: Rect<DevicePx, Device>,
    /// What the pointer changes into over the element.
    pub shape: PointerShape,
}

impl PointerTarget {
    /// Whether `point`, in physical pixels from the top-left corner of the surface, is in the
    /// area of the element.
    pub fn contains(&self, point: Point<DevicePx, Device>) -> bool {
        self.area.contains(point)
    }
}

/// The identifier of the element a [`PointerTarget`] names.
///
/// The platform animates the pointer from one element to the next when the identifier changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointerTargetId(u64);

impl PointerTargetId {
    /// The identifier numbered `value`.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The identifier as an integer.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// What the pointer changes into over an element.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum PointerShape {
    /// A rounded rectangle on the area of the element, for a control that reacts to a press.
    Highlight {
        /// The corner radius of the element, in physical pixels.
        corner_radius: DevicePx,
    },
    /// A vertical text beam, for text that the pointer can put a caret in.
    Beam {
        /// The height of one line of the text, in physical pixels.
        line_height: DevicePx,
    },
}
