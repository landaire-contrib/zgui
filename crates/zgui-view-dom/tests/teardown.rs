//! Releasing a handler or a channel that owns another registration.
//!
//! A listener's handler or an observer's channel can own a view's scope, and the scope's cleanups
//! remove other listeners and observations from the same backend. The backend releases such a
//! closure only after it releases its own tables and the document, or the cleanup finds them
//! borrowed and panics. At window close that panic is the last thing the application does.

mod support;

use std::rc::Rc;

use zgui_dom::side::observed::ObservedMask;
use zgui_elements::{ElementState, r#box};
use zgui_view::{
    Anchor, IntoView, ListenerGuard, ListenerOptions, NodeId, ObservationHandle, Observed, View,
};
use zgui_vocab::EventKind;

use crate::support::Window;

/// Builds an empty box and mounts it under the window's root.
fn mounted_box(window: &Window) -> ElementState {
    let mut built = window
        .window
        .with(|| r#box().into_view().build(&mut window.cx.cx()));
    built.mount(&window.dom, window.root, None);
    built
}

/// The mask the document holds for `node`.
fn mask(window: &Window, node: NodeId) -> ObservedMask {
    let index = window.backend.index_of(node);
    let document = window.document.borrow();
    let key = document.store().key_of(index);
    document
        .store()
        .columns()
        .observed
        .get(key)
        .map(|slots| slots.mask)
        .unwrap_or_default()
}

/// Watches the border box of `node` with a channel that does nothing.
fn watch(window: &Window, node: NodeId) -> ObservationHandle {
    window
        .dom
        .observe(node, Observed::BorderBox, Rc::new(|_| {}))
}

/// The shape of a table header that closed with its window: the header's listener owns the scope
/// that watches another node, and the frame that ends after the header is gone releases it.
#[test]
fn ending_the_frame_releases_a_handler_that_owns_an_observation() {
    let window = Window::open();
    let mut gone = mounted_box(&window);
    let kept = mounted_box(&window);
    let handlers = window.backend.handler_count();

    let handle = watch(&window, kept.node());
    window.dom.add_listener(
        gone.node(),
        EventKind::Click,
        ListenerOptions::DEFAULT,
        Rc::new(move |_| {
            let _ = &handle;
        }),
    );
    assert_eq!(mask(&window, kept.node()), ObservedMask::BORDER_BOX);

    gone.unmount(&window.dom);
    window.backend.end_frame();

    assert_eq!(window.backend.handler_count(), handlers);
    assert_eq!(window.backend.observation_count(), 0);
    assert_eq!(
        mask(&window, kept.node()),
        ObservedMask::empty(),
        "the observation the handler owned was not deregistered"
    );
}

#[test]
fn ending_the_frame_releases_a_channel_that_owns_an_observation() {
    let window = Window::open();
    let mut gone = mounted_box(&window);
    let kept = mounted_box(&window);

    let handle = watch(&window, kept.node());
    let _outer = window.dom.observe(
        gone.node(),
        Observed::ContentSize,
        Rc::new(move |_| {
            let _ = &handle;
        }),
    );
    assert_eq!(window.backend.observation_count(), 2);

    gone.unmount(&window.dom);
    window.backend.end_frame();

    assert_eq!(window.backend.observation_count(), 0);
    assert_eq!(mask(&window, kept.node()), ObservedMask::empty());
}

#[test]
fn dropping_an_observation_releases_a_channel_that_owns_another() {
    let window = Window::open();
    let node = mounted_box(&window).node();

    let inner = watch(&window, node);
    let outer = window.dom.observe(
        node,
        Observed::ContentSize,
        Rc::new(move |_| {
            let _ = &inner;
        }),
    );
    assert_eq!(
        mask(&window, node),
        ObservedMask::BORDER_BOX | ObservedMask::CONTENT_SIZE
    );

    drop(outer);

    assert_eq!(window.backend.observation_count(), 0);
    assert_eq!(mask(&window, node), ObservedMask::empty());
}

#[test]
fn removing_a_listener_releases_a_handler_that_owns_another_listener() {
    let window = Window::open();
    let node = mounted_box(&window).node();
    let handlers = window.backend.handler_count();

    let inner = ListenerGuard::new(
        window.dom.clone(),
        node,
        window.dom.add_listener(
            node,
            EventKind::Click,
            ListenerOptions::DEFAULT,
            Rc::new(|_| {}),
        ),
    );
    let outer = ListenerGuard::new(
        window.dom.clone(),
        node,
        window.dom.add_listener(
            node,
            EventKind::Click,
            ListenerOptions::DEFAULT,
            Rc::new(move |_| {
                let _ = &inner;
            }),
        ),
    );
    assert_eq!(window.backend.handler_count(), handlers + 2);

    drop(outer);

    assert_eq!(window.backend.handler_count(), handlers);
}
