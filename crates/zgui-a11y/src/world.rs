//! Everything a projection reads, in one borrow.
//!
//! An accessibility node is derived from four places at once — what the document says a node
//! means, where layout put it, where the coordinate systems that geometry is expressed in ended up,
//! and which node holds focus — and none of them can be looked up from the others. Passing them as
//! one value is what keeps the projection a function of the frame rather than of whatever happened
//! to be reachable.

use std::collections::HashSet;

use zgui_dom::side::a11y_key::Presence;
use zgui_dom::side::attrs::INERT;
use zgui_dom::{Document, NodeKey, NodeKind};
use zgui_layout::scroll_region::ScrollOffsets;
use zgui_layout::tree::store::LayoutStore;
use zgui_scene::Placements;
use zgui_vocab::SemanticFlags;
use zgui_vocab::Semantics;

/// The document, its geometry and its focus, as one frame's projection reads them.
pub struct World<'a> {
    /// The tree.
    pub document: &'a Document,
    /// Where its boxes ended up.
    pub layout: &'a LayoutStore,
    /// What each coordinate system those boxes are measured in resolved to.
    ///
    /// A fragment's rectangle is in its own space and the name of that space says nothing about
    /// where the space is, so geometry cannot be read from the layout store alone. These are the
    /// answers for the frame that was drawn, because a consumer is told where a control is on the
    /// screen it is looking at.
    pub placements: &'a Placements,
    /// How many device pixels there are to a CSS pixel.
    ///
    /// Bounds are reported in CSS pixels and the root node carries the scale, because a consumer
    /// that had to be told the scale separately would be told it one frame late on every display
    /// change.
    pub scale: f32,
    /// Which node holds keyboard focus, if any does.
    pub focus: Option<NodeKey>,
    /// Where each scroll container is scrolled to, when any is.
    ///
    /// A scroll container's children are measured from the origin its content would have
    /// unscrolled, so that a scroll rewrites the container's node and none of the children's.
    /// `None` reads as no container scrolled.
    pub scroll: Option<&'a ScrollOffsets>,
    /// The subtrees a reader is confined to, when there are any.
    pub modal: Option<Modal>,
}

impl World<'_> {
    /// The node every other node hangs below.
    pub fn root(&self) -> NodeKey {
        self.document.store().key_of(self.document.document_index())
    }

    /// What `node` declares about itself, when it declares anything.
    pub fn semantics(&self, node: NodeKey) -> Option<&Semantics> {
        self.document
            .store()
            .columns()
            .semantics
            .get(node)
            .and_then(|slot| slot.as_deref())
    }

    /// Whether `node` declares anything beyond being a box on the screen, or holds static text.
    ///
    /// The same test the fragment pass applies before recording that a moved element owes a
    /// rectangle — see
    /// [`FrameDirty::is_semantic`](zgui_layout::fragment::diff::FrameDirty::is_semantic) — and it
    /// is here for the same reason: a moved *layout box* changes nothing an assistive technology
    /// was told, and a document is mostly layout boxes.
    pub fn declares_semantics(&self, node: NodeKey) -> bool {
        if self
            .semantics(node)
            .is_some_and(|semantics| !semantics.is_trivial())
        {
            return true;
        }
        let store = self.document.store();
        store.index_of(node).is_some_and(|index| {
            store.core(index).kind() == NodeKind::Element
                && zgui_dom::text::node::holds_text(store, index)
        })
    }

    /// The text `node` holds, when it is a text node holding any.
    pub fn text(&self, node: NodeKey) -> Option<&str> {
        let index = self.document.store().index_of(node)?;
        zgui_dom::text::node::text_of(self.document.store(), index)
    }

    /// Whether `node` hides itself and everything below it from the tree.
    ///
    /// An element is concealed when it declares itself hidden, carries the `inert` attribute, has
    /// a computed style of `display: none` or a `visibility` other than `visible`, or is outside
    /// the [`Modal`] subtree. Each of these except the last marks the element for the
    /// accessibility phase when it changes. The builder answers a change of the modal subtree
    /// itself.
    ///
    /// A concealed element is projected as a hidden node with no children. It stays in the tree so
    /// that a relation that names it, such as a dialog labelled by a title that is hidden, keeps a
    /// target. Nothing below it is projected.
    pub fn conceals(&self, node: NodeKey) -> bool {
        if self
            .modal
            .as_ref()
            .is_some_and(|modal| modal.excludes(self.document, node))
        {
            return true;
        }
        if self
            .semantics(node)
            .is_some_and(|semantics| semantics.flags.contains(SemanticFlags::HIDDEN))
        {
            return true;
        }
        let store = self.document.store();
        let columns = store.columns();
        let inert = columns
            .attrs
            .get(node)
            .and_then(Option::as_deref)
            .is_some_and(|attrs| attrs.get_by_str(INERT).is_some());
        if inert {
            return true;
        }
        columns
            .a11y_key
            .get(node)
            .is_some_and(|key| key.presence == Presence::Hidden)
    }

    /// Whether `node` appears in the projected tree at all.
    ///
    /// This is the existence test every relation is filtered through, and it is deliberately much
    /// stricter than "names a live node". An identifier naming a marker resolves to nothing,
    /// because a marker holds a place and announces nothing. So does an identifier naming a node
    /// that has been taken out of the document: a removed node is still in the arena until the
    /// frame ends, and a projection that took "still allocated" for "still in the tree" would keep
    /// writing relations into a subtree the consumer has already dropped — which a consumer
    /// resolves with an unchecked lookup, on a thread this process does not own. A node below a
    /// [concealed](World::conceals) ancestor also resolves to nothing.
    pub fn is_projected(&self, node: NodeKey) -> bool {
        let store = self.document.store();
        let Some(index) = store.index_of(node) else {
            return false;
        };
        if !matches!(
            store.core(index).kind(),
            NodeKind::Document | NodeKind::Element
        ) {
            return false;
        }
        // Walked rather than read off a flag: attachment is recorded on the node an insertion or a
        // removal names, and a subtree taken out in one call leaves every node below it saying it
        // is attached. The walk is the only answer that is true of a descendant.
        let document = self.document.document_index();
        let mut current = index;
        loop {
            if current == document {
                return true;
            }
            let Some(parent) = store.core(current).parent() else {
                return false;
            };
            if self.conceals(store.key_of(parent)) {
                return false;
            }
            current = parent;
        }
    }
}

/// The subtrees a reader is confined to while a modal surface is open.
///
/// A reader reaches the modal surface, the elements that stay readable beside it, everything
/// inside these, and the elements that hold them. Every other element is
/// [concealed](World::conceals), as for UIKit's `accessibilityViewIsModal`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Modal {
    /// The elements a reader is confined to.
    roots: Vec<NodeKey>,
    /// Every node that holds one of them, up to the document node.
    holders: HashSet<NodeKey>,
}

impl Modal {
    /// The confinement to `surface` and to each element of `beside`, when `surface` is in
    /// `document`.
    ///
    /// An element of `beside` that is not in `document` is left out. So is an element inside
    /// another root, because a reader reaches it with that root.
    pub fn new(document: &Document, surface: NodeKey, beside: &[NodeKey]) -> Option<Self> {
        let mut paths = vec![(surface, holders_of(document, surface)?)];
        for root in beside {
            if let Some(holders) = holders_of(document, *root) {
                paths.push((*root, holders));
            }
        }

        let all: Vec<NodeKey> = paths.iter().map(|(root, _)| *root).collect();
        paths.retain(|(_, holders)| !holders.iter().any(|holder| all.contains(holder)));

        let roots = paths.iter().map(|(root, _)| *root).collect();
        let holders = paths.into_iter().flat_map(|(_, holders)| holders).collect();

        Some(Self { roots, holders })
    }

    /// Every node that holds one of the elements a reader is confined to.
    pub fn holders(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.holders.iter().copied()
    }

    /// Whether `node` is outside the confinement and is the top of what it conceals: a sibling of
    /// one of the roots or of one of their holders.
    fn excludes(&self, document: &Document, node: NodeKey) -> bool {
        if self.roots.contains(&node) || self.holders.contains(&node) {
            return false;
        }

        let store = document.store();
        let Some(index) = store.index_of(node) else {
            return false;
        };

        store
            .core(index)
            .parent()
            .is_some_and(|parent| self.holders.contains(&store.key_of(parent)))
    }
}

/// Every node that holds `node`, from its parent to the document node, when `node` is in
/// `document`.
fn holders_of(document: &Document, node: NodeKey) -> Option<Vec<NodeKey>> {
    let store = document.store();
    let root = document.document_index();
    let mut current = store.index_of(node)?;
    let mut holders = Vec::new();
    while current != root {
        current = store.core(current).parent()?;
        holders.push(store.key_of(current));
    }

    Some(holders)
}

#[cfg(test)]
mod tests {
    use zgui_dom::{Document, EverythingMatters, NodeKind};
    use zgui_interned::AttrName;
    use zgui_interned::ElementName;
    use zgui_layout::tree::store::LayoutStore;
    use zgui_vocab::A11y;
    use zgui_vocab::Role;
    use zgui_vocab::SharedString;

    use super::{Modal, World};

    #[test]
    fn a_concealed_element_stays_and_everything_below_it_leaves() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let hidden = document.append(root, NodeKind::Element, ElementName::new("title"));
        let inside = document.append(hidden, NodeKind::Element, ElementName::new("box"));
        let inert = document.append(root, NodeKind::Element, ElementName::new("page"));
        let below_inert = document.append(inert, NodeKind::Element, ElementName::new("box"));
        document
            .edit(&EverythingMatters, |edit| {
                edit.set_semantics(hidden, Some(A11y::new(Role::Label).hidden(true).into()));
                edit.set_attribute(inert, AttrName::new("inert"), Some(SharedString::from("")));
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

        let store = document.store();
        assert!(world.conceals(store.key_of(hidden)));
        assert!(world.conceals(store.key_of(inert)));
        assert!(
            world.is_projected(store.key_of(hidden)),
            "a relation may name a hidden title, so the title itself stays as a target"
        );
        assert!(!world.is_projected(store.key_of(inside)));
        assert!(!world.is_projected(store.key_of(below_inert)));
    }

    #[test]
    fn a_modal_surface_conceals_the_page_and_keeps_what_is_beside_it() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let page = document.append(root, NodeKind::Element, ElementName::new("page"));
        let row = document.append(page, NodeKind::Element, ElementName::new("row"));
        let overlay = document.append(root, NodeKind::Element, ElementName::new("overlay"));
        let menus = document.append(overlay, NodeKind::Element, ElementName::new("layer"));
        let menu = document.append(menus, NodeKind::Element, ElementName::new("menu"));
        let first = document.append(menu, NodeKind::Element, ElementName::new("item"));
        let second = document.append(menu, NodeKind::Element, ElementName::new("item"));
        let notices = document.append(overlay, NodeKind::Element, ElementName::new("layer"));
        let toast = document.append(notices, NodeKind::Element, ElementName::new("toast"));
        let layout = LayoutStore::new(document.store().document());

        let store = document.store();
        let key = |index| store.key_of(index);
        let world = World {
            document: &document,
            layout: &layout,
            placements: &zgui_scene::Placements::EMPTY,
            scale: 1.0,
            focus: None,
            scroll: None,
            modal: Modal::new(&document, key(menu), &[key(toast), key(first)]),
        };

        assert!(world.conceals(key(page)));
        assert!(!world.is_projected(key(row)));
        assert!(world.is_projected(key(menu)));
        assert!(
            world.is_projected(key(second)),
            "a root inside the modal surface conceals nothing beside it"
        );
        assert!(world.is_projected(key(toast)));
        assert!(!world.conceals(key(notices)));
    }

    #[test]
    fn a_marker_is_never_part_of_the_projected_tree() {
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let marker = document.append(root, NodeKind::Marker, ElementName::new("#marker"));
        let text = document.append(root, NodeKind::Text, ElementName::new("#text"));
        let layout = LayoutStore::new(document.store().document());
        let placements = zgui_scene::Placements::new();
        let world = World {
            document: &document,
            layout: &layout,
            placements: &placements,
            scale: 1.0,
            focus: None,
            scroll: None,
            modal: None,
        };

        let store = document.store();
        assert!(world.is_projected(store.key_of(root)));
        assert!(!world.is_projected(store.key_of(marker)));
        assert!(
            !world.is_projected(store.key_of(text)),
            "the characters inside an element are that element's name, not a node of their own"
        );
    }

    #[test]
    fn a_subtree_taken_out_this_frame_is_out_of_the_tree_before_the_arena_recycles_it() {
        // The window between a removal and the end of the frame, which is exactly when the
        // accessibility update for that frame is built. Every node below the one that was removed
        // is still allocated, still an element, and still says it is in the document — and none of
        // them is in the tree any assistive technology is holding.
        let mut document = Document::new();
        let root = document.append(
            document.document_index(),
            NodeKind::Element,
            ElementName::new("root"),
        );
        let holder = document.append(root, NodeKind::Element, ElementName::new("box"));
        let deep = document.append(holder, NodeKind::Element, ElementName::new("label"));
        let keys = (
            document.store().key_of(holder),
            document.store().key_of(deep),
        );

        document
            .edit(&zgui_dom::EverythingMatters, |edit| edit.remove(holder))
            .expect("not poisoned");

        let layout = LayoutStore::new(document.store().document());
        let placements = zgui_scene::Placements::new();
        let world = World {
            document: &document,
            layout: &layout,
            placements: &placements,
            scale: 1.0,
            focus: None,
            scroll: None,
            modal: None,
        };
        assert!(!world.is_projected(keys.0));
        assert!(
            !world.is_projected(keys.1),
            "a node below the one that was removed is still allocated and still an element, and a \
             relation written into it is one the consumer resolves against nothing"
        );
        assert!(world.is_projected(document.store().key_of(root)));
    }
}
