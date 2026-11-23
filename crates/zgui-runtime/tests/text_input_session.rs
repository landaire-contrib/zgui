//! The text input session and the keyboard a field asks for, through the whole loop.

mod support;

use zgui_platform::{SurfaceEvent, TextInput, TextInputPurpose};
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   field { display: block; width: 200px; height: 40px }
                   control { display: block; width: 100px; height: 24px }";

/// A window with two fields and a control after them.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
}

impl Script {
    /// A window holding a numeric field, a secure numeric field and a control.
    fn new() -> Self {
        let first = NodeRef::new();
        let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::field()
                            .node_ref(first)
                            .input_mode("numeric")
                            .child("12"),
                    )
                    .child(
                        zgui_elements::field()
                            .secure(true)
                            .input_mode("numeric")
                            .child("34"),
                    )
                    .child(zgui_elements::control())
                    .into_view()
                    .build(cx),
            )
        });
        harness.settle(8);
        Self { harness }
    }

    /// Presses a named key.
    fn press(&mut self, key: NamedKey, code: KeyCode, modifiers: Modifiers) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(key, PhysicalKey::Code(code)),
            modifiers,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(8);
    }

    /// Moves focus forward.
    fn tab(&mut self) {
        self.press(NamedKey::Tab, KeyCode::Tab, Modifiers::NONE);
    }

    /// Moves focus back.
    fn back_tab(&mut self) {
        self.press(NamedKey::Tab, KeyCode::Tab, Modifiers::SHIFT);
    }

    /// What the surface was last told.
    fn told(&self) -> Option<TextInput> {
        self.harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface was created")
            .last_text_input()
            .flatten()
    }
}

#[test]
fn each_field_focus_is_a_session_and_a_caret_move_stays_in_it() {
    let mut script = Script::new();
    script.tab();
    let first = script.told().expect("text input is on");
    assert_eq!(first.purpose, TextInputPurpose::Digits);
    let session = first.session.expect("the window tells the session");

    script.press(NamedKey::End, KeyCode::End, Modifiers::NONE);
    let moved = script.told().expect("text input is on");
    assert_eq!(
        moved.session,
        Some(session),
        "a caret move started a session"
    );

    // Straight to the next field: a new session, and the keyboard stays.
    script.tab();
    let second = script.told().expect("text input stays on across a switch");
    assert_eq!(second.purpose, TextInputPurpose::Pin);
    assert_ne!(second.session, Some(session));

    // Out to the control and back: text input goes off, and the return is a new session.
    script.tab();
    assert_eq!(
        script.told(),
        None,
        "the keyboard stays with no field focused"
    );
    script.back_tab();
    let back = script.told().expect("text input is on again");
    assert_ne!(back.session, second.session);
}
