//! What a focused field tells the platform about its keyboard, through the whole loop.

mod support;

use zgui_platform::{
    Autocapitalize, EnterKeyHint, SurfaceEvent, TextContentType, TextInput, TextInputHints,
    TextInputPurpose,
};
use zgui_view::{AnyView, BuildCx, IntoView, View};
use zgui_vocab::{KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   field { display: block; width: 200px; height: 40px }
                   editor { display: block; width: 200px; height: 40px }";

/// What the surface was told once `editable` took focus.
fn told(editable: fn() -> AnyView) -> TextInput {
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(editable())
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);

    harness
        .platform()
        .offscreens()
        .first()
        .expect("a surface was created")
        .last_text_input()
        .flatten()
        .expect("text input is on")
}

#[test]
fn a_password_field_declares_a_secret_and_its_content() {
    let told = told(|| {
        AnyView::new(
            zgui_elements::field()
                .secure(true)
                .autocomplete("current-password")
                .enter_key_hint("go"),
        )
    });
    assert_eq!(told.purpose, TextInputPurpose::Password);
    assert_eq!(
        told.hints,
        TextInputHints {
            multi_line: false,
            enter_key: Some(EnterKeyHint::Go),
            autocorrect: None,
            autocapitalize: None,
            content_type: Some(TextContentType::Password),
        }
    );
}

#[test]
fn a_field_turns_correction_and_capitals_off() {
    let told = told(|| {
        AnyView::new(
            zgui_elements::field()
                .autocorrect(false)
                .autocapitalize("none")
                .autocomplete("username"),
        )
    });
    assert_eq!(told.purpose, TextInputPurpose::Normal);
    assert_eq!(told.hints.autocorrect, Some(false));
    assert_eq!(told.hints.autocapitalize, Some(Autocapitalize::None));
    assert_eq!(told.hints.content_type, Some(TextContentType::Username));
}

#[test]
fn an_editor_takes_line_breaks_and_a_plain_field_leaves_the_rest_to_the_platform() {
    let editor = told(|| AnyView::new(zgui_elements::editor()));
    assert!(editor.hints.multi_line);

    let field = told(|| AnyView::new(zgui_elements::field()));
    assert_eq!(field.hints, TextInputHints::default());
}
