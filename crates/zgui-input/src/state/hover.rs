//! What the pointer is over.

use smallvec::SmallVec;
use zgui_dom::{Document, NodeKey, StyleFilter};
use zgui_vocab::UiState;

use crate::hit::HitChain;
use crate::state::within::{Moved, crossed, move_bit, set_bit};

/// Which elements a pointer is over, and the writes that keep `:hover` true for them.
///
/// A finger cannot hover: it is either touching or it is not there. A touch contact therefore
/// moves the path, so that elements still hear the contact arrive and leave, and writes no
/// `:hover`. A control whose only affordance appears on hover is unreachable by touch rather than
/// shown by every tap and every scroll that starts on it.
#[derive(Clone, Debug, Default)]
pub struct Hover {
    /// The path the pointer is over, root first.
    path: SmallVec<[NodeKey; 8]>,
    /// Whether `path` carries the bit, which it does when a pointer that can hover put it there.
    marked: bool,
}

impl Hover {
    /// The elements the pointer is over, root first.
    pub fn path(&self) -> &[NodeKey] {
        &self.path
    }

    /// The innermost element the pointer is over.
    pub fn target(&self) -> Option<NodeKey> {
        self.path.last().copied()
    }

    /// Moves a pointer that can hover onto `chain`, writing only the elements that changed.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn move_to(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
    ) -> Moved {
        self.follow(document, filter, chain, true)
    }

    /// Moves a contact that cannot hover onto `chain`. The path moves and no element carries
    /// `:hover`.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn touch(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
    ) -> Moved {
        self.follow(document, filter, chain, false)
    }

    /// Moves the path onto `chain` again for the same pointer, which keeps its kind.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn retest(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
    ) -> Moved {
        self.follow(document, filter, chain, self.marked)
    }

    /// Clears the hover, which is what a pointer leaving the surface does.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn clear(&mut self, document: &Document, filter: &dyn StyleFilter) -> Moved {
        let moved = if self.marked {
            move_bit(document, filter, UiState::HOVER, &self.path, &[])
        } else {
            crossed(document, &self.path, &[])
        };
        self.path.clear();
        self.marked = false;
        moved
    }

    /// Moves the path onto `chain`, with the bit on it when `marks`.
    fn follow(
        &mut self,
        document: &Document,
        filter: &dyn StyleFilter,
        chain: &HitChain,
        marks: bool,
    ) -> Moved {
        let moved = match (self.marked, marks) {
            (true, true) => move_bit(document, filter, UiState::HOVER, &self.path, chain.path()),
            (true, false) => {
                set_bit(document, filter, UiState::HOVER, &self.path, false);
                crossed(document, &self.path, chain.path())
            }
            (false, true) => {
                set_bit(document, filter, UiState::HOVER, chain.path(), true);
                crossed(document, &self.path, chain.path())
            }
            (false, false) => crossed(document, &self.path, chain.path()),
        };
        self.path = chain.path().iter().copied().collect();
        self.marked = marks;
        moved
    }
}
