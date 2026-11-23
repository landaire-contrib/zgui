//! The identity of everything a node's accessible description depends on.
//!
//! The same idea as the paint key one level over. A style key can be a set of addresses because
//! computed values are shared and immutable. An accessible name is not: editing a text node
//! changes what a screen reader would say without changing any style group and without changing
//! any semantics record — which is why the edit itself marks the element for the accessibility
//! phase, and the key does not have to see it.

/// Identity of what a node's accessible description is derived from.
///
/// Written once per node per restyle, and compared against the previous frame's value to decide
/// whether the accessibility projection has to be rebuilt for this node.
///
/// Text is edited in place, and the edit marks the element for the accessibility phase itself. The
/// key holds the semantics record and the one fact the projection reads from computed style:
/// whether the style hides the element.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub struct A11yKey {
    /// The node's semantics record, or zero when it has none.
    pub semantics: usize,
    /// Whether the node's computed style hides it.
    pub presence: Presence,
}

impl A11yKey {
    /// The key of a node whose accessible description has never been computed.
    pub const UNPROJECTED: Self = Self {
        semantics: 0,
        presence: Presence::Shown,
    };
}

/// Whether an element's computed style hides it from the accessibility tree.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Default, Debug)]
pub enum Presence {
    /// The style shows the element.
    #[default]
    Shown,
    /// `display: none`, or `visibility` other than `visible`.
    Hidden,
}

#[cfg(test)]
mod tests {
    use super::A11yKey;
    use super::Presence;

    /// The column is dense over every node slot, so its slot is pinned at two words.
    #[test]
    fn a_column_slot_is_two_words_wide() {
        assert_eq!(
            core::mem::size_of::<Option<A11yKey>>(),
            2 * core::mem::size_of::<usize>()
        );
    }

    #[test]
    fn the_unprojected_key_is_the_default() {
        assert_eq!(A11yKey::default(), A11yKey::UNPROJECTED);
    }

    #[test]
    fn a_new_semantics_record_changes_the_key() {
        let before = A11yKey {
            semantics: 0x80,
            presence: Presence::Shown,
        };
        let after = A11yKey {
            semantics: 0xc0,
            presence: Presence::Shown,
        };
        assert_ne!(before, after);
        assert_ne!(before, A11yKey::UNPROJECTED);
    }

    #[test]
    fn a_style_that_hides_the_node_changes_the_key() {
        let hidden = A11yKey {
            presence: Presence::Hidden,
            ..A11yKey::UNPROJECTED
        };
        assert_ne!(hidden, A11yKey::UNPROJECTED);
    }
}
