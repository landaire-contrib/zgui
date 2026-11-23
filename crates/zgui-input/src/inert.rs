//! Which elements input cannot reach.
//!
//! An element with the `inert` attribute, and every element below it, is out of reach of input.
//! A hit test passes through it to what is below, it is not focusable, and it gets no keys.

use zgui_dom::DocumentStore;
use zgui_dom::Node;
use zgui_dom::NodeIndex;
use zgui_dom::NodeKey;
use zgui_dom::side::attrs::INERT;

/// Whether `node` or an ancestor of it has the `inert` attribute.
///
/// A node that is not in the store is not inert.
pub fn is_inert(store: &DocumentStore, node: NodeKey) -> bool {
    let Some(mut index) = store.index_of(node) else {
        return false;
    };

    loop {
        if has_inert(store, index) {
            return true;
        }
        match store.core(index).parent() {
            Some(parent) => index = parent,
            None => return false,
        }
    }
}

/// Whether this node itself has the `inert` attribute.
pub(crate) fn has_inert(store: &DocumentStore, index: NodeIndex) -> bool {
    Node::new(store.core(index)).attr(INERT).is_some()
}

#[cfg(test)]
mod tests {
    use zgui_dom::Document;
    use zgui_dom::EverythingMatters;
    use zgui_interned::AttrName;
    use zgui_interned::ElementName;
    use zgui_vocab::SharedString;

    use super::is_inert;

    #[test]
    fn an_inert_element_makes_its_subtree_inert_and_leaves_its_siblings_alone() {
        let document = Document::new();
        let (page, field, sibling) = document
            .edit(&EverythingMatters, |edit| {
                let root = edit.create_element(ElementName::new("root"));
                edit.insert_before(document.document_index(), root, None);
                let page = edit.create_element(ElementName::new("page"));
                edit.insert_before(root, page, None);
                let field = edit.create_element(ElementName::new("field"));
                edit.insert_before(page, field, None);
                let sibling = edit.create_element(ElementName::new("page"));
                edit.insert_before(root, sibling, None);
                edit.set_attribute(page, AttrName::new("inert"), Some(SharedString::from("")));
                (page, field, sibling)
            })
            .expect("not poisoned");

        let store = document.store();
        assert!(is_inert(store, store.key_of(page)));
        assert!(is_inert(store, store.key_of(field)));
        assert!(!is_inert(store, store.key_of(sibling)));

        document
            .edit(&EverythingMatters, |edit| {
                edit.set_attribute(page, AttrName::new("inert"), None);
            })
            .expect("not poisoned");
        let store = document.store();
        assert!(!is_inert(store, store.key_of(field)));
    }
}
