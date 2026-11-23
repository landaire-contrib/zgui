//! The engine's bookkeeping bits, and the two records of style work below an element.
//!
//! # Work below an element
//!
//! The engine descends into an element's children when work is below the element. Two writers
//! record that work, and each has its own storage and its own retirement:
//!
//! * A mark records an obligation in the subtree half of every ancestor's invalidation word, and
//!   in every ancestor's dirty-child record. The document's walk after the restyle follows those
//!   records and retires the word.
//! * The engine raises its descent flag on the path to an element that an invalidation reached,
//!   for example an element that a selector about an ancestor's attribute matches. The flag is a
//!   bit of the bookkeeping word. The traversal clears it when it visits the element, in the same
//!   restyle that raised it.
//!
//! [`Node::has_style_work_below`] reads both.
//!
//! The flag must stay out of the invalidation word. No mark leads to the path the engine flags, so
//! no dirty-child record names that path and the document's walk never reaches it. A union raised
//! there stays raised. A mark stops at the first ancestor whose union already holds its bits, so a
//! later mark below the path stops on it: the ancestors above never learn of the mark, and no
//! traversal reaches the marked element again.
//!
//! The animation-only flag is the same kind of bit, raised and cleared by the animation-only
//! traversal.

use zgui_bits::Dirty;

use crate::node::atomics;
use crate::node::handle::Node;

/// The obligations that mean "the style engine has work at or below this node".
pub const STYLE_WORK: Dirty = Dirty::RESTYLE.union(Dirty::RECASCADE);

impl Node<'_> {
    /// Whether a snapshot of this element's pre-mutation identity is recorded and unconsumed.
    pub fn has_snapshot(self) -> bool {
        self.record().has_atomic(atomics::HAS_SNAPSHOT)
    }

    /// Whether the traversal has already consumed this element's snapshot.
    pub fn handled_snapshot(self) -> bool {
        self.record().has_atomic(atomics::SNAPSHOT_HANDLED)
    }

    /// Records that the traversal has consumed this element's snapshot.
    pub fn set_handled_snapshot(self) {
        self.record().set_atomic(atomics::SNAPSHOT_HANDLED);
    }

    /// Whether anything at or below this node owes the style engine work.
    ///
    /// True when a mark at or below this node is not yet retired, or when the engine's descent flag
    /// is raised on this node.
    pub fn has_style_work_below(self) -> bool {
        self.record().has_dirty_descendants(STYLE_WORK) || self.has_descent_flag()
    }

    /// Whether the engine's descent flag is raised on this node.
    ///
    /// No dirty-child record leads to the children the flag stands for, so a traversal that finds
    /// it visits every child.
    pub fn has_descent_flag(self) -> bool {
        self.record().has_atomic(atomics::DIRTY_DESCENDANTS)
    }

    /// Raises the engine's descent flag, so that the traversal descends into this element's
    /// children.
    ///
    /// The invalidation word stays as it is; the module documentation says why.
    pub fn note_style_work_below(self) {
        self.record().set_atomic(atomics::DIRTY_DESCENDANTS);
    }

    /// Clears the engine's descent flag, once the traversal has visited this element.
    pub fn clear_style_work_below(self) {
        self.record().clear_atomic(atomics::DIRTY_DESCENDANTS);
    }

    /// Whether the custom property map this element inherits from moved since its style was built.
    pub fn has_custom_map_changed(self) -> bool {
        self.record().has_atomic(atomics::CUSTOM_MAP_CHANGED)
    }

    /// Records that the custom property map this element inherits from has moved.
    pub fn note_custom_map_changed(self) {
        self.record().set_atomic(atomics::CUSTOM_MAP_CHANGED);
    }

    /// Takes the custom-map-changed note, answering whether it was set.
    pub fn take_custom_map_changed(self) -> bool {
        let was = self.has_custom_map_changed();
        if was {
            self.record().clear_atomic(atomics::CUSTOM_MAP_CHANGED);
        }
        was
    }

    /// Whether an animation-only restyle is pending below this node.
    pub fn has_animation_work_below(self) -> bool {
        self.record()
            .has_atomic(atomics::ANIMATION_DIRTY_DESCENDANTS)
    }

    /// Records that an animation-only restyle is pending below this node.
    pub fn note_animation_work_below(self) {
        self.record()
            .set_atomic(atomics::ANIMATION_DIRTY_DESCENDANTS);
    }

    /// Forgets that an animation-only restyle is pending below this node.
    pub fn clear_animation_work_below(self) {
        self.record()
            .clear_atomic(atomics::ANIMATION_DIRTY_DESCENDANTS);
    }
}

#[cfg(test)]
mod tests {
    use zgui_bits::Dirty;
    use zgui_interned::ElementName;

    use crate::arena::document::Document;
    use crate::node::kind::NodeKind;

    #[test]
    fn a_mark_below_a_node_is_work_below_it() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let child = document.append(root, NodeKind::Element, ElementName::new("item"));

        assert!(!document.node(root).has_style_work_below());
        crate::dirty::propagate::mark(document.store_mut(), child, Dirty::RESTYLE);
        assert!(document.node(root).has_style_work_below());
        assert!(
            document.node(child).has_style_work_below(),
            "a node's own obligation is part of its own subtree"
        );
    }

    #[test]
    fn the_descent_flag_is_work_below_and_keeps_out_of_the_invalidation_word() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let node = document.node(root);

        node.note_style_work_below();
        assert!(node.has_style_work_below());
        assert_eq!(
            document.store().core(root).dirty().subtree(),
            Dirty::empty(),
            "a union raised here would stop every later mark below this node"
        );
        node.clear_style_work_below();
        assert!(!node.has_style_work_below());
    }

    #[test]
    fn a_phase_that_is_not_the_style_engines_does_not_look_like_style_work() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        document.store().core(root).dirty().mark(Dirty::REPAINT);
        assert!(!document.node(root).has_style_work_below());
    }

    #[test]
    fn the_animation_flag_round_trips_because_it_has_one_owner() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let node = document.node(root);
        assert!(!node.has_animation_work_below());
        node.note_animation_work_below();
        assert!(node.has_animation_work_below());
        node.clear_animation_work_below();
        assert!(!node.has_animation_work_below());
    }
}
