//! What the `inert` attribute takes out of reach of input.
//!
//! An inert page lies over a control. A press on the page goes to the control below it, the page's
//! own listeners hear nothing, and the page's controls are not in the focus order and refuse focus.

mod support;

use support::{Element, Fixture, Session};
use zgui_dom::EverythingMatters;
use zgui_input::FocusSource;
use zgui_input::focus::order;
use zgui_input::focus::order::FocusDirection;
use zgui_interned::AttrName;
use zgui_vocab::EventKind;
use zgui_vocab::ListenerOptions;
use zgui_vocab::PointerAction;
use zgui_vocab::SharedString;

/// A control in the flow, and an inert page over it that holds a button and a field.
fn covered() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("under").attr("tabindex", "0"),
            Element::new("page").attr("inert", "").children(vec![
                Element::new("button").attr("tabindex", "0"),
                Element::new("field"),
            ]),
            Element::new("after").attr("tabindex", "0"),
        ]),
        "root { display: block; position: relative; width: 400px; height: 300px }
         under { display: block; width: 200px; height: 100px }
         page { display: block; position: absolute; left: 0; top: 0; width: 400px; height: 300px }
         button { display: block; width: 200px; height: 100px }
         field { display: block; width: 200px; height: 40px }
         after { display: block; width: 100px; height: 20px }",
    )
}

/// Sets or removes `inert` on the page.
fn set_inert(fixture: &Fixture, inert: bool) {
    let page = fixture.find("page");
    fixture
        .document
        .edit(&EverythingMatters, |edit| {
            edit.set_attribute(
                page,
                AttrName::new("inert"),
                inert.then(|| SharedString::from("")),
            );
        })
        .expect("not poisoned");
}

#[test]
fn a_press_on_an_inert_page_reaches_the_element_below_it() {
    let fixture = covered();
    for name in ["page", "button"] {
        let node = fixture.find(name);
        fixture
            .document
            .edit(&EverythingMatters, |edit| {
                edit.add_listener(node, EventKind::PointerDown, ListenerOptions::DEFAULT);
                edit.add_listener(node, EventKind::Click, ListenerOptions::DEFAULT);
            })
            .expect("not poisoned");
    }
    let mut session = Session::new(fixture);
    let point = session.fixture.centre_of("button");
    let under = session.fixture.key("under");
    let button = session.fixture.key("button");

    let (path, steps) = session.route(point, PointerAction::Pressed, |routed| {
        (routed.chain.path().to_vec(), routed.steps.to_vec())
    });
    assert_eq!(path.last(), Some(&under), "the press went through the page");
    assert!(steps.is_empty(), "a listener on the inert page ran");
    let released = session.route(point, PointerAction::Released, |routed| routed.target());
    assert_eq!(released, Some(under));

    set_inert(&session.fixture, false);
    let target = session.route(point, PointerAction::Pressed, |routed| routed.target());
    assert_eq!(target, Some(button), "the page takes the press again");
}

#[test]
fn an_inert_page_is_out_of_the_focus_order_and_refuses_focus() {
    let fixture = covered();
    let store = fixture.document.store();
    let root = store.key_of(fixture.root);
    let under = fixture.key("under");
    let button = fixture.key("button");
    let field = fixture.key("field");
    let after = fixture.key("after");

    let sequence = order::focusables(store, Some(&fixture.layout), root);
    assert_eq!(sequence, vec![under, after]);
    assert_eq!(
        order::step(&sequence, Some(under), FocusDirection::Next, false),
        Some(after),
        "tab skips the inert page"
    );
    assert!(!order::is_focusable(store, Some(&fixture.layout), field));

    let mut session = Session::new(fixture);
    {
        let filter = session.fixture.filter();
        let world = session.fixture.world(&filter);
        session
            .router
            .focus(&world, Some(under), FocusSource::Script);
        let moved = session
            .router
            .focus(&world, Some(field), FocusSource::Script);
        assert_eq!(moved, (None, None));
    }
    assert_eq!(session.router.interaction().focus.focused(), Some(under));

    set_inert(&session.fixture, false);
    let store = session.fixture.document.store();
    assert_eq!(
        order::focusables(store, Some(&session.fixture.layout), root),
        vec![under, button, field, after]
    );
}
