//! A finger on a control beside a focused editor.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use zgui_geom::{CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_reactive::prelude::GetUntracked;
use zgui_view::{BuildCx, IntoView, NodeRef, View, ViewHost};
use zgui_vocab::{
    KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, PointerAction, PointerButton,
    PointerEvent, PointerId, PointerKind, Timestamp,
};

/// An editor at the top, and below it a 44 pixel control with a 20 pixel mark in its middle.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   editor { display: block; width: 200px; height: 40px }
                   control { display: flex; align-items: center; justify-content: center;
                             width: 44px; height: 44px }
                   .mark { display: block; width: 20px; height: 20px }";

/// Where the control starts, below the editor.
const CONTROL_TOP: f32 = 40.0;

struct Script {
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    editor: NodeRef,
    control: NodeRef,
    clicks: Rc<Cell<u32>>,
}

impl Script {
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    fn touch(&mut self, action: PointerAction, x: f32, y: f32) {
        let event = PointerEvent {
            id: PointerId::new(1),
            kind: PointerKind::Touch,
            primary: true,
            position: Point::new(CssPx(x), CssPx(y)),
            button: Some(PointerButton::Primary),
            pressure: None,
        };
        self.deliver(SurfaceEvent::Pointer {
            action,
            event,
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    fn focused(&self) -> Option<zgui_view::NodeId> {
        self.harness.app().windows()[0]
            .host()
            .focused()
            .get_untracked()
    }

    fn text_input_on(&self) -> bool {
        matches!(
            self.harness
                .platform()
                .offscreens()
                .first()
                .expect("a surface was created")
                .last_text_input(),
            Some(Some(_))
        )
    }
}

/// A window with the editor focused, and a control that does or does not take focus when pressed.
///
/// With `focus_on_press` set the control has no `focusonpress` attribute, which is the default.
fn scripted(focus_on_press: bool) -> Script {
    let editor = NodeRef::new();
    let control = NodeRef::new();
    let clicks = Rc::new(Cell::new(0u32));
    let counted = Rc::clone(&clicks);

    let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let counted = Rc::clone(&counted);
        let button = zgui_elements::control()
            .node_ref(control)
            .on(zgui_view::events::CLICK, move |_| {
                counted.set(counted.get() + 1);
            })
            .child(zgui_elements::r#box().class("mark"));
        let button = if focus_on_press {
            button
        } else {
            button.focus_on_press(false)
        };
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::editor().node_ref(editor).child("ab"))
                .child(button)
                .into_view()
                .build(cx),
        )
    });

    let mut script = Script {
        harness,
        editor,
        control,
        clicks,
    };
    script.harness.settle(8);
    script.deliver(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    assert_eq!(script.focused(), script.editor.get_untracked());
    assert!(script.text_input_on());
    script
}

#[test]
fn a_control_that_keeps_focus_leaves_the_editor_typing_and_is_still_clicked() {
    let mut script = scripted(false);
    script.touch(PointerAction::Pressed, 22.0, CONTROL_TOP + 22.0);
    assert_eq!(
        script.focused(),
        script.editor.get_untracked(),
        "the press took focus from the editor"
    );
    assert!(script.text_input_on(), "the press closed the keyboard");

    script.touch(PointerAction::Released, 22.0, CONTROL_TOP + 22.0);
    assert_eq!(script.clicks.get(), 1);
    assert_eq!(script.focused(), script.editor.get_untracked());
    assert!(script.text_input_on());
}

#[test]
fn a_control_takes_focus_when_pressed_by_default() {
    let mut script = scripted(true);
    script.touch(PointerAction::Pressed, 22.0, CONTROL_TOP + 22.0);
    script.touch(PointerAction::Released, 22.0, CONTROL_TOP + 22.0);
    assert_eq!(script.clicks.get(), 1);
    assert_eq!(script.focused(), script.control.get_untracked());
    assert!(!script.text_input_on());
}

#[test]
fn a_press_on_the_mark_lifted_over_the_padding_of_the_control_clicks_it() {
    // The mark spans 12 to 32 across the control. The finger moves 7 pixels, inside the slop.
    let mut script = scripted(false);
    script.touch(PointerAction::Pressed, 13.0, CONTROL_TOP + 22.0);
    script.touch(PointerAction::Moved, 6.0, CONTROL_TOP + 22.0);
    script.touch(PointerAction::Released, 6.0, CONTROL_TOP + 22.0);
    assert_eq!(script.clicks.get(), 1);

    script.touch(PointerAction::Pressed, 13.0, CONTROL_TOP + 22.0);
    script.touch(PointerAction::Released, 13.0, CONTROL_TOP + 50.0);
    assert_eq!(
        script.clicks.get(),
        1,
        "a lift below the control is no click"
    );
}
