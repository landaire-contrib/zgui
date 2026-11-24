//! The role and name an element gets from the text inside it.
//!
//! An element that declared no label of its own is named by its own text, exactly as
//! `aria-labelledby` pointing at a paragraph is: the text *is* the name. Without this, relating a
//! field to the element that names it produces a field with no name at all — a consumer reading an
//! explicit relation asks the target for its label and gets nothing, because the target is a box
//! and the words are one level further down.
//!
//! An element that declared no role and holds text is static text, a [`Role::Label`]. A consumer
//! drops a [`Role::GenericContainer`] and promotes its children, so text left in one is text a
//! screen reader never reaches. A button with no label of its own takes its name from the static
//! text below it.
//!
//! Only the element's **own** text children take part. A name assembled from a whole subtree would
//! make one deep element pay for every text node beneath it, and the invalidation key this
//! projection is scheduled by hashes exactly these children and no others — so widening the rule
//! here would silently stop the name from being rebuilt when the text changed.

use accesskit::Node;
use accesskit::Role;
use zgui_dom::{NodeKey, NodeKind};
use zgui_vocab::EventKind;
use zgui_vocab::Semantics;

use crate::world::World;

/// The roles whose own text is what they hold rather than what they are called.
///
/// An editable element's text *is* its value, and the only place that value exists is the document
/// — the editing model writes it there and nothing else keeps a copy. A field that declared one as
/// well would be a second answer to the same question, drifting from the first the moment somebody
/// types; so a field declares no value and this reads the real one.
const HOLDS_ITS_TEXT_AS_A_VALUE: [Role; 3] =
    [Role::TextInput, Role::MultilineTextInput, Role::SearchInput];

/// The character a password field reports in place of each character it holds.
const MASK: char = '\u{2022}';

/// Gives `into` the role and name the element's own text implies.
///
/// Which property the text lands in depends on the role, and the difference is not cosmetic: a
/// consumer reads the text of a [`Role::Label`] node from its **value** and every other node's from
/// its **label**, so writing to the wrong one produces a node that is in the tree and says nothing.
/// A field is read the same way as a label and for the same reason — the words inside it are what
/// it holds — while a field whose text were written to its label would be announced by whatever the
/// user last typed instead of by what it is for.
///
/// A password field reports one bullet for each character it holds, whatever it declared, so its
/// secret never reaches the tree.
pub fn apply(world: &World<'_>, node: NodeKey, declared: &Semantics, into: &mut Node) {
    if declared.role == Role::PasswordInput {
        into.clear_value();
        if let Some(text) = from_content(world, node) {
            into.set_value(text.chars().map(|_| MASK).collect::<String>());
        }
        return;
    }

    let Some(text) = from_content(world, node) else {
        return;
    };

    let role = if declared.role == Role::GenericContainer
        && !text.trim().is_empty()
        && is_static_text(world, node)
    {
        into.set_role(Role::Label);
        Role::Label
    } else {
        declared.role
    };

    if role == Role::Label || HOLDS_ITS_TEXT_AS_A_VALUE.contains(&role) {
        if declared.value.is_none() {
            into.set_value(text);
        }
        return;
    }
    if declared.label.is_none() {
        into.set_label(text);
    }
}

/// Whether a container holding text is static text.
///
/// A concealed element keeps its text as a label, which a button's name skips. An element with a
/// shown element child stays a container, so that the child stays reachable. An element that
/// listens for clicks stays a container, which a web page reads as a button.
fn is_static_text(world: &World<'_>, node: NodeKey) -> bool {
    if world.conceals(node) || listens_for_click(world, node) {
        return false;
    }
    crate::project::children::element_keys(world, node)
        .into_iter()
        .all(|child| world.conceals(child))
}

/// Whether `node` has a click listener.
fn listens_for_click(world: &World<'_>, node: NodeKey) -> bool {
    world
        .document
        .store()
        .columns()
        .listeners
        .get(node)
        .is_some_and(|set| set.iter().any(|listener| listener.kind == EventKind::Click))
}

/// Names `into` from its placeholder when nothing else names it.
///
/// The last step of the accessible name computation a browser applies to a field. The placeholder
/// moves to the label, so a consumer that reads both does not announce it twice.
pub fn from_placeholder(into: &mut Node) {
    if into.label().is_some() || !into.labelled_by().is_empty() {
        return;
    }
    let Some(placeholder) = into.placeholder().map(str::to_owned) else {
        return;
    };
    into.clear_placeholder();
    into.set_label(placeholder);
}

/// The text directly inside `node`, joined with single spaces, or nothing when there is none.
pub fn from_content(world: &World<'_>, node: NodeKey) -> Option<String> {
    let store = world.document.store();
    let index = store.index_of(node)?;
    let mut name = String::new();
    let mut child = store.core(index).first_child();
    while let Some(current) = child {
        if store.core(current).kind() == NodeKind::Text
            && let Some(text) = zgui_dom::text::node::text_of(store, current)
            && !text.is_empty()
        {
            if !name.is_empty() {
                name.push(' ');
            }
            name.push_str(text);
        }
        child = store.core(current).next_sibling();
    }
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use accesskit::Node;
    use accesskit::Role;
    use zgui_dom::{Document, EverythingMatters, NodeIndex, NodeKind};
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;
    use zgui_vocab::Semantics;

    use super::{apply, from_content, from_placeholder};
    use crate::world::World;

    /// One element holding `text`, below the document node.
    fn holding(text: &str) -> (Document, NodeIndex) {
        let mut document = Document::new();
        let element = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("field"),
        );
        let node = document.append(element, NodeKind::Text, ElementName::new("#text"));
        document
            .edit(&EverythingMatters, |edit| edit.set_text(node, text))
            .expect("not poisoned");
        (document, element)
    }

    /// What `declared` on the element of `document` projects to, as far as this module says.
    fn named(document: &Document, element: NodeIndex, declared: &Semantics) -> Node {
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
            scroll: None,
            modal: None,
        };
        let mut node = Node::new(declared.role);
        crate::project::semantics::apply(declared, &mut node);
        apply(
            &world,
            document.store().key_of(element),
            declared,
            &mut node,
        );
        node
    }

    #[test]
    fn a_password_field_reports_a_bullet_per_character_and_no_name_from_its_text() {
        let (document, field) = holding("abc");
        let mut declared = Semantics::new(Role::PasswordInput);
        declared.value = Some("abc".into());
        let node = named(&document, field, &declared);
        assert_eq!(node.value(), Some("\u{2022}\u{2022}\u{2022}"));
        assert_eq!(node.label(), None, "a password field is named by its label");
    }

    #[test]
    fn text_in_an_element_that_declared_no_role_is_static_text() {
        let (document, element) = holding("User name");
        let node = named(&document, element, &Semantics::default());
        assert_eq!(node.role(), Role::Label);
        assert_eq!(node.value(), Some("User name"));
    }

    #[test]
    fn blank_text_leaves_a_container_a_container() {
        let (document, element) = holding("  ");
        let node = named(&document, element, &Semantics::default());
        assert_eq!(node.role(), Role::GenericContainer);
    }

    #[test]
    fn a_field_with_no_name_is_named_by_its_placeholder_once() {
        let mut node = Node::new(Role::PasswordInput);
        node.set_placeholder("Master password");
        from_placeholder(&mut node);
        assert_eq!(node.label(), Some("Master password"));
        assert_eq!(node.placeholder(), None);

        let mut labelled = Node::new(Role::TextInput);
        labelled.set_label("Database");
        labelled.set_placeholder("Path to .kdbx database");
        from_placeholder(&mut labelled);
        assert_eq!(labelled.label(), Some("Database"));
        assert_eq!(labelled.placeholder(), Some("Path to .kdbx database"));
    }

    #[test]
    fn an_element_is_named_by_the_text_directly_inside_it_and_not_by_a_subtree() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let own = document.append(root, NodeKind::Text, ElementName::new("#text"));
        let nested = document.append(root, NodeKind::Element, ElementName::new("box"));
        let deep = document.append(nested, NodeKind::Text, ElementName::new("#text"));
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_text(own, "Full name");
                edit.set_text(deep, "not part of the name");
            })
            .expect("not poisoned");

        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
            scroll: None,
            modal: None,
        };
        assert_eq!(
            from_content(&world, document.store().key_of(root)).as_deref(),
            Some("Full name")
        );
    }

    #[test]
    fn an_element_with_no_text_of_its_own_is_named_by_nothing() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let layout = LayoutStore::new(document.store().document());
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
            scroll: None,
            modal: None,
        };
        assert_eq!(from_content(&world, document.store().key_of(root)), None);
    }
}
