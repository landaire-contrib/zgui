//! The flag that gets the animation-only traversal from the root down to one element.
//!
//! The animation-only traversal is not the ordinary one and does not descend for the same reasons.
//! It never propagates a selector-matching hint, so the only thing that takes it below an element
//! is a per-element flag saying "an animation-only restyle is pending somewhere under me". An
//! element carrying an animation hint and no such flag on any of its ancestors is unreachable: the
//! traversal starts at the root, asks the root that question, is told no, and stops — the element
//! never restyles, and the hint it is still carrying is what the *ordinary* traversal then refuses
//! to process.
//!
//! The restyle raises the flag from every element marked since the last restyle, from where the
//! element stands at that moment, so an element a view moved in between is reached in its new
//! place. It is cleared by the traversal that reads it, which is what makes storing it safe: one
//! storage, one retirement, inside one traversal.

use rustc_hash::FxHashSet;
use style::dom::TNode;
use zgui_dom::{Node, NodeIndex};

/// Records on every ancestor of `node` that an animation-only restyle is pending below it.
///
/// The chain is walked to the root rather than to the nearest styled ancestor, because the
/// traversal starts at the root: a chain broken anywhere above the element is a chain the descent
/// stops at.
pub fn raise_to_root(node: Node<'_>) {
    let mut ancestor = node.traversal_parent();
    while let Some(element) = ancestor {
        element.note_animation_work_below();
        ancestor = element.traversal_parent();
    }
}

/// Where an element stands for the animation-only traversal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// In the document and below no `display: none` ancestor. The traversal reaches it.
    Reached,
    /// In the document and below an ancestor whose style is `display: none`. The traversal stops
    /// at that ancestor.
    Hidden,
    /// Out of the document, or not an element.
    Detached,
}

/// The ancestors one settling pass has raised the descent flag on.
///
/// Each one is in the document and below no `display: none` ancestor, and so is every ancestor
/// above it. Marked elements share most of their ancestors, so a walk stops at the first one
/// already held, and each ancestor is read once per pass.
#[derive(Debug, Default)]
pub struct Raised {
    /// The ancestors raised this pass.
    held: FxHashSet<NodeIndex>,
}

impl Raised {
    /// Starts a pass. The capacity stays for the next pass.
    pub fn clear(&mut self) {
        self.held.clear();
    }

    /// Where `node` stands for the animation-only traversal that starts at `root`.
    ///
    /// When the traversal reaches `node`, raises the descent flag on each of its ancestors, as
    /// [`raise_to_root`] does.
    pub fn settle(&mut self, node: Node<'_>, root: Option<NodeIndex>) -> Standing {
        let Some(mut top) = node.as_element() else {
            return Standing::Detached;
        };

        let mut hidden = false;
        let mut held = false;
        while let Some(parent) = top.traversal_parent() {
            if self.held.contains(&parent.index()) {
                held = true;
                break;
            }
            hidden |= parent
                .borrow_style_data()
                .is_some_and(|data| data.styles.is_display_none());
            top = parent;
        }

        let standing = if !held && Some(top.index()) != root {
            Standing::Detached
        } else if hidden {
            Standing::Hidden
        } else {
            Standing::Reached
        };
        if standing != Standing::Reached {
            return standing;
        }

        // An ancestor already held has the flag, and so has every ancestor above it.
        let mut ancestor = node.traversal_parent();
        while let Some(element) = ancestor {
            if !self.held.insert(element.index()) {
                break;
            }
            element.note_animation_work_below();
            ancestor = element.traversal_parent();
        }

        standing
    }
}

/// Forgets that an animation-only restyle is pending below `element`.
///
/// Called by the animation-only traversal on each element it has finished visiting, after the
/// engine has read the flag to decide whether to descend and after it has noted the children it is
/// descending to. What is left is a flag nothing this frame will read again, and leaving it raised
/// would take the next frame's traversal down a subtree with nothing in it to do.
pub fn clear(element: Node<'_>) {
    element.clear_animation_work_below();
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, NodeKind};
    use zgui_interned::ElementName;

    use super::{Raised, Standing, clear, raise_to_root};

    #[test]
    fn the_flag_is_raised_on_every_ancestor_and_not_on_the_element_itself() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let middle = document.append(root, NodeKind::Element, ElementName::new("box"));
        let leaf = document.append(middle, NodeKind::Element, ElementName::new("box"));

        raise_to_root(document.node(leaf));

        assert!(
            document.node(root).has_animation_work_below(),
            "the traversal starts at the root, so a root without the flag reaches nothing"
        );
        assert!(document.node(middle).has_animation_work_below());
        assert!(
            !document.node(leaf).has_animation_work_below(),
            "the element's own hint is what makes it visited; a flag on it would claim work below \
             it that is not there"
        );

        clear(document.node(middle));
        assert!(!document.node(middle).has_animation_work_below());
    }

    #[test]
    fn one_pass_raises_the_flag_on_the_ancestors_of_every_reached_element() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let middle = document.append(root, NodeKind::Element, ElementName::new("box"));
        let first = document.append(middle, NodeKind::Element, ElementName::new("box"));
        let side = document.append(middle, NodeKind::Element, ElementName::new("box"));
        let second = document.append(side, NodeKind::Element, ElementName::new("box"));
        let top = document.root().map(|root| root.index());

        let mut raised = Raised::default();
        assert_eq!(raised.settle(document.node(first), top), Standing::Reached);
        assert_eq!(raised.settle(document.node(second), top), Standing::Reached);

        for ancestor in [root, middle, side] {
            assert!(document.node(ancestor).has_animation_work_below());
        }
        assert!(!document.node(first).has_animation_work_below());
        assert!(!document.node(second).has_animation_work_below());
    }
}
