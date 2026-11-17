//! The flattened clip a draw call binds.

use bytemuck::{Pod, Zeroable};
use zgui_atlas::AtlasTile;

/// One rounded-rectangle test, in the layout a shader reads it in.
///
/// The radii are eight floats — a horizontal and a vertical radius per corner, clockwise from the
/// top left — because CSS corners are elliptical and a scalar per corner cannot express
/// `border-radius: 80px / 20px`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct RoundedTest {
    /// The rectangle, as `[x, y, width, height]`.
    pub rect: [f32; 4],
    /// Elliptical radii, two per corner, clockwise from the top left.
    pub radii: [f32; 8],
    /// The superellipse exponent the corners are cut with; two is the ellipse.
    pub shape: f32,
}

impl RoundedTest {
    /// Whether the test changes no pixel inside `aabb`, which lies inside the test's rectangle.
    ///
    /// The shader samples pixel centres, applies `aabb` as a hard edge and antialiases the test
    /// over one pixel. A straight side of the test is inert when it falls on a pixel boundary, or
    /// when `aabb` stays at least half a pixel inside it. A corner is inert when `aabb` misses its
    /// `rx` by `ry` box grown by one pixel for the antialiasing band.
    pub fn is_inert(&self, aabb: [f32; 4]) -> bool {
        let [x, y, width, height] = self.rect;
        let (right, bottom) = (x + width, y + height);
        let (inner_left, inner_top) = (aabb[0], aabb[1]);
        let (inner_right, inner_bottom) = (aabb[0] + aabb[2], aabb[1] + aabb[3]);
        if aabb[2] <= 0.0 || aabb[3] <= 0.0 {
            return true;
        }
        let side = |edge: f32, inset: f32| edge == edge.round() || inset >= 0.5;
        if !(side(x, inner_left - x)
            && side(y, inner_top - y)
            && side(right, right - inner_right)
            && side(bottom, bottom - inner_bottom))
        {
            return false;
        }
        let r = self.radii;
        // Each corner's box as `[left, top, right, bottom]`, with its two radii.
        let corners = [
            ([x, y, x + r[0], y + r[1]], r[0], r[1]),
            ([right - r[2], y, right, y + r[3]], r[2], r[3]),
            ([right - r[4], bottom - r[5], right, bottom], r[4], r[5]),
            ([x, bottom - r[7], x + r[6], bottom], r[6], r[7]),
        ];
        corners.iter().all(|&([left, top, right, bottom], rx, ry)| {
            rx <= 0.0
                || ry <= 0.0
                || inner_left >= right + 1.0
                || inner_right <= left - 1.0
                || inner_top >= bottom + 1.0
                || inner_bottom <= top - 1.0
        })
    }
}

/// A whole clip chain, flattened into what one draw call can apply.
///
/// A chain of any length collapses to an intersection rectangle plus at most two rounded tests plus
/// at most one coverage mask. Rounded tests that change no pixel inside the rectangle are left out.
/// When more than two remain, the two innermost are kept and [`ResolvedClip::dropped`] counts the
/// others; [`ClipTable::needs_group_target`](crate::ClipTable::needs_group_target) names the chains
/// a target of their own would draw exactly.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ResolvedClip {
    /// The intersection of every rectangle in the chain, as `[x, y, width, height]`.
    pub aabb: [f32; 4],
    /// Up to two rounded-corner tests; only the first `rounded_count` are meaningful.
    pub rounded: [RoundedTest; 2],
    /// How many of `rounded` are meaningful.
    pub rounded_count: u32,
    /// How many rounded tests that change pixels could not be applied.
    pub dropped: u8,
    /// The coverage tile the chain samples, if it has one.
    pub mask: Option<AtlasTile>,
}

impl ResolvedClip {
    /// How far a chain that clips nothing extends, in device pixels.
    ///
    /// Finite rather than infinite so that intersecting with it is ordinary arithmetic: an infinite
    /// extent turns an empty intersection into a `NaN` and a clip test into a coin toss. Far larger
    /// than any surface, and far smaller than where `f32` loses integer precision.
    pub const UNBOUNDED: f32 = 1.0e7;

    /// The clip that admits everything.
    pub fn unbounded() -> Self {
        Self {
            aabb: [
                -Self::UNBOUNDED,
                -Self::UNBOUNDED,
                2.0 * Self::UNBOUNDED,
                2.0 * Self::UNBOUNDED,
            ],
            rounded: [RoundedTest::default(); 2],
            rounded_count: 0,
            dropped: 0,
            mask: None,
        }
    }

    /// The intersection rectangle's left edge.
    pub fn left(&self) -> f32 {
        self.aabb[0]
    }

    /// The intersection rectangle's top edge.
    pub fn top(&self) -> f32 {
        self.aabb[1]
    }

    /// The intersection rectangle's right edge.
    pub fn right(&self) -> f32 {
        self.aabb[0] + self.aabb[2]
    }

    /// The intersection rectangle's bottom edge.
    pub fn bottom(&self) -> f32 {
        self.aabb[1] + self.aabb[3]
    }

    /// Whether the chain admits no pixels at all.
    ///
    /// Content under an empty clip is dropped before it reaches the display list, which is what
    /// makes a thousand-row table inside a scrollport cost its visible rows rather than all of
    /// them.
    pub fn is_empty(&self) -> bool {
        self.aabb[2] <= 0.0 || self.aabb[3] <= 0.0
    }
}

/// A whole clip chain, flattened with the links of one coordinate system kept in its coordinates.
///
/// What [`ClipTable::resolve_kept`](crate::ClipTable::resolve_kept) gives. The links measured in
/// the kept system are moved by its translation when the clip is applied, so the system can move
/// and the flattened chain stays valid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeptClip {
    /// The chain, with [`ResolvedClip::aabb`] the intersection of the links measured elsewhere.
    ///
    /// The rounded tests are of both parts.
    pub clip: ResolvedClip,
    /// The intersection of the links measured in the kept system, in its coordinates.
    pub kept_aabb: [f32; 4],
    /// Bit `i` is set when `clip.rounded[i]` is measured in the kept system.
    pub kept_tests: u32,
}
