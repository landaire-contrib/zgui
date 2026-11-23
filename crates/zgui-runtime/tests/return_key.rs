//! A line break an input method commits, through the whole loop.
//!
//! The on-screen keyboard of iOS commits its return key as a line break. A single-line field takes
//! it as a press of Enter. A multi-line editor takes it as text.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_platform::SurfaceEvent;
use zgui_view::{AnyView, BuildCx, EventCx, IntoView, NodeRef, View, events};
use zgui_vocab::{
    ImeEvent, Key, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp,
    ValueChange,
};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   field { display: block; width: 200px; height: 40px }
                   editor { display: block; width: 200px; height: 40px }";

/// What the element's listeners heard.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Heard {
    /// A key went down.
    KeyDown(Key),
    /// A key came up.
    KeyUp(Key),
    /// The value changed to this text.
    Input(String),
}

/// One window holding a single editable element.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The element.
    element: NodeRef,
    /// What the element's listeners heard, in order.
    heard: Rc<RefCell<Vec<Heard>>>,
}

impl Script {
    /// A window holding a field, or an editor when `multi_line` is set, with `content` in it.
    fn new(multi_line: bool, content: &'static str) -> Self {
        let element = NodeRef::new();
        let heard: Rc<RefCell<Vec<Heard>>> = Rc::new(RefCell::new(Vec::new()));
        let recorded = Rc::clone(&heard);

        let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
            let editable = if multi_line {
                AnyView::new(listened(
                    zgui_elements::editor(),
                    element,
                    &recorded,
                    content,
                ))
            } else {
                AnyView::new(listened(
                    zgui_elements::field(),
                    element,
                    &recorded,
                    content,
                ))
            };
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(editable)
                    .into_view()
                    .build(cx),
            )
        });

        let mut script = Self {
            harness,
            element,
            heard,
        };
        script.harness.settle(8);
        script.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        script.heard.borrow_mut().clear();
        script
    }

    /// Delivers one surface event and lets the frames it produced settle.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    /// Delivers one step of a composition.
    fn ime(&mut self, event: ImeEvent) {
        self.deliver(SurfaceEvent::Ime(event));
    }

    /// Everything the listeners heard since the last call.
    fn heard(&self) -> Vec<Heard> {
        core::mem::take(&mut self.heard.borrow_mut())
    }

    /// The element's value.
    fn value(&self) -> String {
        let window = &self.harness.app().windows()[0];
        let document = window.document().borrow();
        let store = document.store();
        let key = zgui_view_dom::id::to_document(self.element.get_untracked().expect("bound"))
            .expect("the element names a document node");
        let index = store.index_of(key).expect("the element is still live");
        let mut paragraphs = Vec::new();
        let mut child = store.core(index).first_child();
        while let Some(node) = child {
            if let Some(text) = zgui_dom::text::text_of(store, node) {
                paragraphs.push(text.to_owned());
            }
            child = store.core(node).next_sibling();
        }
        paragraphs.concat()
    }
}

/// `view` with `content` in it, bound to `element`, and with listeners that record into `heard`.
fn listened<T: zgui_elements::Tag>(
    view: zgui_elements::Element<T>,
    element: NodeRef,
    heard: &Rc<RefCell<Vec<Heard>>>,
    content: &'static str,
) -> zgui_elements::Element<T> {
    let down = Rc::clone(heard);
    let up = Rc::clone(heard);
    let input = Rc::clone(heard);

    view.node_ref(element)
        .on(
            events::KEY_DOWN,
            move |cx: &mut EventCx<'_, events::KeyDown>| {
                if let Some(key) = cx.payload().as_key() {
                    down.borrow_mut().push(Heard::KeyDown(key.key.clone()));
                }
            },
        )
        .on(
            events::KEY_UP,
            move |cx: &mut EventCx<'_, events::KeyUp>| {
                if let Some(key) = cx.payload().as_key() {
                    up.borrow_mut().push(Heard::KeyUp(key.key.clone()));
                }
            },
        )
        .on(events::INPUT, move |cx: &mut EventCx<'_, events::Input>| {
            if let Some(value) = cx.payload().as_value()
                && value.kind == ValueChange::Input
            {
                input
                    .borrow_mut()
                    .push(Heard::Input(value.value.to_string()));
            }
        })
        .child(content)
}

/// The key a line break stands for.
fn enter() -> Key {
    Key::Named(NamedKey::Enter)
}

#[test]
fn a_line_break_committed_into_a_field_is_a_press_of_enter() {
    let mut script = Script::new(false, "pw");
    script.ime(ImeEvent::Commit("\n".into()));

    assert_eq!(
        script.value(),
        "pw",
        "a line break landed in a single-line field"
    );
    assert_eq!(
        script.heard(),
        vec![Heard::KeyDown(enter()), Heard::KeyUp(enter())]
    );
}

#[test]
fn the_text_around_a_committed_line_break_is_typed_in_order() {
    let mut script = Script::new(false, "");
    script.ime(ImeEvent::Commit("ab\r\ncd".into()));

    assert_eq!(script.value(), "abcd");
    assert_eq!(
        script.heard(),
        vec![
            Heard::Input("ab".to_owned()),
            Heard::KeyDown(enter()),
            Heard::KeyUp(enter()),
            Heard::Input("abcd".to_owned()),
        ]
    );
}

#[test]
fn a_composition_that_commits_a_line_break_ends_before_the_press() {
    let mut script = Script::new(false, "");
    script.ime(ImeEvent::Enabled);
    script.ime(ImeEvent::Preedit {
        text: "x".into(),
        cursor: None,
    });
    script.heard();
    script.ime(ImeEvent::Commit("\n".into()));

    assert_eq!(script.value(), "");
    assert_eq!(
        script.heard(),
        vec![
            Heard::Input(String::new()),
            Heard::KeyDown(enter()),
            Heard::KeyUp(enter()),
        ]
    );
}

#[test]
fn a_line_break_committed_into_an_editor_is_text() {
    let mut script = Script::new(true, "ab");
    script.ime(ImeEvent::Commit("\n".into()));

    assert_eq!(script.value(), "\nab");
    assert_eq!(script.heard(), vec![Heard::Input("\nab".to_owned())]);
}
