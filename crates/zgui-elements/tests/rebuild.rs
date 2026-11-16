//! An element described again in place, read back off the element it describes.
//!
//! Every description of one element name is the same view type, so a closure that switches between
//! two of them rebuilds the one node. What the node then carries has to be the current description
//! and nothing left over from the other one.

extern crate zgui_elements as zgui;

use std::rc::Rc;

use zgui_reactive::prelude::*;
use zgui_reactive::{Mounted, RwSignal, flush, install};
use zgui_view::stub::{StubDom, StubHost};
use zgui_view::{
    Anchor, AnyView, AttrName, BuildCxOwned, ClassName, CustomPropertyName, DocumentId, DomHandle,
    ElementName, HostHandle, Ident, IntoView, NodeId, NodeRef, PropKey, Role, UiState, View,
};
use zgui_view_macro::view;

/// A stub document, a window scope and a root element to mount into.
struct Stage {
    backend: Rc<StubDom>,
    dom: DomHandle,
    window: Mounted,
    cx: BuildCxOwned,
    root: NodeId,
}

impl Stage {
    fn new() -> Self {
        install().ok();
        let backend = Rc::new(StubDom::new(DocumentId::FIRST));
        let dom = DomHandle::from_rc(backend.clone());
        let window = Mounted::new();
        let cx = BuildCxOwned::new(
            dom.clone(),
            HostHandle::new(StubHost::default()),
            window.owner().clone(),
            DocumentId::FIRST,
        );
        let root = dom.create_element(ElementName::new("root"));
        Self {
            backend,
            dom,
            window,
            cx,
            root,
        }
    }

    /// Builds `view` in the window's scope and mounts it under the root.
    fn mount<V: IntoView>(&self, view: V) -> <V::Output as View>::State {
        let mut state = self
            .window
            .with(|| view.into_view().build(&mut self.cx.cx()));
        state.mount(&self.dom, self.root, None);
        state
    }
}

/// The shape that found this: a search that matches nothing shows a `text` with a test id, and
/// clearing the search shows a `text` without one in the same place.
#[test]
fn an_attribute_the_new_description_does_not_mention_is_removed() {
    let stage = Stage::new();
    let test_id = AttrName::new("data-testid");
    let matched = stage.window.with(|| RwSignal::new(false));

    let mut state = stage.mount(move || {
        if matched.get() {
            AnyView::new(view! { text(class = "title") { "Analysis" } })
        } else {
            AnyView::new(view! { text(class = "none", attr:data-testid = "no-match") { "None" } })
        }
    });
    let node = state.first_node().expect("the closure built an element");
    assert_eq!(
        stage.backend.attribute(node, test_id).as_deref(),
        Some("no-match")
    );

    matched.set(true);
    flush();
    assert_eq!(
        state.first_node(),
        Some(node),
        "the element was rebuilt in place"
    );
    assert_eq!(stage.backend.classes(node), [ClassName::new("title")]);
    assert_eq!(
        stage.backend.attribute(node, test_id),
        None,
        "the attribute only the previous description wrote is still on the element"
    );

    state.unmount(&stage.dom);
    stage.window.unmount();
}

/// A value another writer put on the element is not the description's to remove.
#[test]
fn an_attribute_another_writer_replaced_stays_when_the_description_drops_it() {
    let stage = Stage::new();
    let mode = AttrName::new("data-mode");
    let handle = stage.window.with(NodeRef::new);
    let declared = stage.window.with(|| RwSignal::new(true));

    let mut state = stage.mount(move || {
        if declared.get() {
            view! { box(node_ref = handle, attr:data-mode = "declared") }
        } else {
            view! { box(node_ref = handle) }
        }
    });
    let node = state.first_node().expect("the closure built an element");

    handle.set_attribute(mode, "imperative");
    declared.set(false);
    flush();
    assert_eq!(state.first_node(), Some(node));
    assert_eq!(
        stage.backend.attribute(node, mode).as_deref(),
        Some("imperative")
    );

    state.unmount(&stage.dom);
    stage.window.unmount();
}

/// A computed attribute is removed under the value its binding wrote last.
#[test]
fn a_computed_attribute_is_removed_after_its_value_changed() {
    let stage = Stage::new();
    let mode = AttrName::new("data-mode");
    let value = stage.window.with(|| RwSignal::new("first"));
    let declared = stage.window.with(|| RwSignal::new(true));

    let mut state = stage.mount(move || {
        if declared.get() {
            view! { box(attr:data-mode = move || Some(value.get().to_owned())) }
        } else {
            view! { box() }
        }
    });
    let node = state.first_node().expect("the closure built an element");

    value.set("second");
    flush();
    assert_eq!(
        stage.backend.attribute(node, mode).as_deref(),
        Some("second")
    );

    declared.set(false);
    flush();
    assert_eq!(state.first_node(), Some(node));
    assert_eq!(stage.backend.attribute(node, mode), None);

    state.unmount(&stage.dom);
    stage.window.unmount();
}

#[test]
fn a_description_that_writes_nothing_leaves_nothing_on_the_element() {
    let stage = Stage::new();
    let bare = stage.window.with(|| RwSignal::new(false));

    let mut state = stage.mount(move || {
        if bare.get() {
            view! { box() }
        } else {
            view! {
                box(
                    class = "page",
                    class:open = true,
                    style:padding = "4px",
                    var:--fill = "red",
                    attr:data-part = "root",
                    prop:value = "typed",
                    state:disabled = true,
                    custom_state:peeking = true,
                    a11y:role = Role::Group
                )
            }
        }
    });
    let node = state.first_node().expect("the closure built an element");
    assert!(stage.backend.semantics(node).is_some());
    assert_eq!(
        stage
            .backend
            .attribute(node, AttrName::new("data-part"))
            .as_deref(),
        Some("root")
    );

    bare.set(true);
    flush();
    assert_eq!(state.first_node(), Some(node));
    let backend = &stage.backend;
    assert!(backend.classes(node).is_empty());
    assert_eq!(backend.style_property(node, "padding"), None);
    assert_eq!(
        backend.custom_property(node, CustomPropertyName::new("fill")),
        None
    );
    assert_eq!(backend.attribute(node, AttrName::new("data-part")), None);
    assert!(backend.property(node, PropKey::new("value")).is_none());
    assert!(!backend.ui_state(node).contains(UiState::DISABLED));
    assert!(!backend.has_custom_state(node, Ident::new("peeking")));
    assert!(backend.semantics(node).is_none());

    state.unmount(&stage.dom);
    stage.window.unmount();
}

/// What the previous description wrote is removed before the new one is written, so a class that
/// was toggled on and is now in the class list is not toggled off after the list goes on.
#[test]
fn a_class_the_previous_description_toggled_stays_when_the_new_class_list_names_it() {
    let stage = Stage::new();
    let listed = stage.window.with(|| RwSignal::new(false));

    let mut state = stage.mount(move || {
        if listed.get() {
            view! { box(class = "open") }
        } else {
            view! { box(class:open = true) }
        }
    });
    let node = state.first_node().expect("the closure built an element");

    listed.set(true);
    flush();
    assert_eq!(stage.backend.classes(node), [ClassName::new("open")]);

    state.unmount(&stage.dom);
    stage.window.unmount();
}
