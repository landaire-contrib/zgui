//! The style engine's bookkeeping word.
//!
//! The bits share one atomic, because every one of them is read or written from a worker thread
//! while the traversal runs. They are a single word rather than several cells for the same reason:
//! the word is already paid for, and a bit that the engine only reads today is one release away
//! from being a bit it writes.
//!
//! The two descendants-dirty bits belong to the engine alone. The engine raises each of them during
//! a restyle, and a traversal of that same restyle clears it. Marks record their obligations in the
//! invalidation word, and [`flags`](crate::stylo::flags) says why the two records stay apart.

/// A snapshot of this element's pre-mutation state is recorded and not yet consumed.
pub const HAS_SNAPSHOT: u32 = 1 << 0;

/// The traversal has consumed this element's snapshot.
pub const SNAPSHOT_HANDLED: u32 = 1 << 1;

/// This element's style data has been established at least once, and not cleared since.
///
/// The style data itself is stored inline in every record, so "does this element have data?"
/// cannot be answered by its presence — the naive answer is always yes, which makes the engine's
/// subtree-clearing walk descend into subtrees it should have pruned. This bit is the real answer:
/// establishing data sets it, clearing data clears it *and* resets the inline data in the same
/// call, so the two answers cannot disagree.
pub const STYLED: u32 = 1 << 2;

/// Something below this element has an animation-only restyle pending.
///
/// Raised and cleared by the animation-only traversal, as [`DIRTY_DESCENDANTS`] is by an ordinary
/// one.
pub const ANIMATION_DIRTY_DESCENDANTS: u32 = 1 << 3;

/// The custom property map this element inherits from moved since its style was built.
///
/// Raised on every child of an element whose inherited map changed, by the traversal step that
/// changed it, and consumed by the child's own visit: the child either cascades again, because
/// something it declares reads the map, or takes the new map into its existing style.
pub const CUSTOM_MAP_CHANGED: u32 = 1 << 4;

/// The engine's traversal descends into this element's children.
///
/// Raised on the path to an element that an invalidation reached, and cleared when the traversal
/// visits this element.
pub const DIRTY_DESCENDANTS: u32 = 1 << 5;

/// Every bit this word defines.
pub const ALL: u32 = HAS_SNAPSHOT
    | SNAPSHOT_HANDLED
    | STYLED
    | ANIMATION_DIRTY_DESCENDANTS
    | CUSTOM_MAP_CHANGED
    | DIRTY_DESCENDANTS;

#[cfg(test)]
mod tests {
    use super::{
        ALL, ANIMATION_DIRTY_DESCENDANTS, CUSTOM_MAP_CHANGED, DIRTY_DESCENDANTS, HAS_SNAPSHOT,
        SNAPSHOT_HANDLED, STYLED,
    };

    #[test]
    fn the_bits_are_distinct_and_the_union_is_exactly_them() {
        let bits = [
            HAS_SNAPSHOT,
            SNAPSHOT_HANDLED,
            STYLED,
            ANIMATION_DIRTY_DESCENDANTS,
            CUSTOM_MAP_CHANGED,
            DIRTY_DESCENDANTS,
        ];
        assert_eq!(bits.iter().fold(0, |all, bit| all | bit), ALL);
        assert_eq!(ALL.count_ones(), 6);
    }
}
