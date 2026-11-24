//! Segment breaks in text and in an editor, through the whole loop.
//!
//! A value of `white-space` that preserves breaks makes each segment break force a line break. The
//! fixed face makes a line 20 pixels tall here and a cluster 8 pixels wide.

mod support;

use zgui_geom::{Device, DevicePx, Rect};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};

/// The height of one line.
const LINE: f32 = 20.0;

/// One cluster's advance.
const ADVANCE: f32 = 8.0;

/// The values of `white-space` that preserve segment breaks.
const PRESERVING: [&str; 4] = ["pre", "pre-wrap", "pre-line", "break-spaces"];

/// The values of `white-space` that collapse segment breaks into spaces.
const COLLAPSING: [&str; 2] = ["normal", "nowrap"];

/// Each spelling of a segment break, between two words.
const SPELLINGS: [&str; 3] = ["ab\ncd", "ab\r\ncd", "ab\rcd"];

/// A window with a text block and an editor that both start with `text`, under `white_space`.
struct Script {
    /// The window being driven.
    harness: Harness<Runtime>,
    /// The editor.
    editor: NodeRef,
}

impl Script {
    fn new(white_space: &str, text: &'static str) -> Self {
        let css = format!(
            "root {{ display: block; width: 400px; height: 300px }}
             .text, editor {{ display: block; width: 300px; line-height: {LINE}px;
                              white-space: {white_space} }}"
        );
        let editor = NodeRef::new();
        let mut harness = support::app_with_text(&css, move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(zgui_elements::column().class("text").child(text))
                    .child(
                        zgui_elements::editor()
                            .class("notes")
                            .node_ref(editor)
                            .child(text),
                    )
                    .into_view()
                    .build(cx),
            )
        });
        harness.settle(8);
        Self { harness, editor }
    }

    /// Where the element with `class` is.
    fn box_of(&self, class: &str) -> Rect<DevicePx, Device> {
        self.harness.app().windows()[0]
            .box_with_class(class)
            .expect("the element is laid out")
    }

    /// How many lines the element with `class` holds.
    fn lines(&self, class: &str) -> f32 {
        self.box_of(class).height().0 / LINE
    }

    /// Presses a named key.
    fn press(&mut self, key: NamedKey, code: KeyCode) {
        self.harness.deliver_to_first(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(key, PhysicalKey::Code(code)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
        self.harness.settle(8);
    }

    /// The caret's offset in the editor, and its column and line on the screen.
    fn caret(&self) -> (usize, f32, f32) {
        let selection = self.editor.selection().expect("the editor has a selection");
        assert!(selection.is_empty(), "the selection is a caret");
        let caret = self.harness.app().windows()[0]
            .caret_rect()
            .expect("the caret is drawn");
        let editor = self.box_of("notes");
        (
            selection.start,
            (caret.origin.x.0 - editor.origin.x.0) / ADVANCE,
            (caret.origin.y.0 - editor.origin.y.0) / LINE,
        )
    }
}

#[test]
fn a_segment_break_forces_a_line_break_where_white_space_preserves_it() {
    for white_space in PRESERVING {
        for text in SPELLINGS {
            let script = Script::new(white_space, text);
            assert_eq!(script.lines("text"), 2.0, "{text:?} in text, {white_space}");
            assert_eq!(
                script.lines("notes"),
                2.0,
                "{text:?} in an editor, {white_space}"
            );
        }
    }
}

#[test]
fn a_segment_break_collapses_to_a_space_where_white_space_collapses_it() {
    for white_space in COLLAPSING {
        for text in SPELLINGS {
            let script = Script::new(white_space, text);
            assert_eq!(script.lines("text"), 1.0, "{text:?} in text, {white_space}");
            assert_eq!(
                script.lines("notes"),
                1.0,
                "{text:?} in an editor, {white_space}"
            );
        }
    }
}

#[test]
fn enter_in_a_pre_wrap_editor_starts_a_line_and_the_caret_crosses_it() {
    let mut script = Script::new("pre-wrap", "abcd");
    script.press(NamedKey::Tab, KeyCode::Tab);
    script.press(NamedKey::ArrowRight, KeyCode::ArrowRight);
    script.press(NamedKey::ArrowRight, KeyCode::ArrowRight);
    assert_eq!(script.caret(), (2, 2.0, 0.0));

    script.press(NamedKey::Enter, KeyCode::Enter);
    assert_eq!(script.lines("notes"), 2.0, "ab, then cd");
    assert_eq!(
        script.caret(),
        (3, 0.0, 1.0),
        "the caret starts the second line"
    );

    script.press(NamedKey::ArrowLeft, KeyCode::ArrowLeft);
    assert_eq!(
        script.caret(),
        (2, 2.0, 0.0),
        "one step back ends the first line"
    );
    script.press(NamedKey::ArrowRight, KeyCode::ArrowRight);
    assert_eq!(
        script.caret(),
        (3, 0.0, 1.0),
        "one step on starts the second"
    );
    script.press(NamedKey::ArrowRight, KeyCode::ArrowRight);
    assert_eq!(script.caret(), (4, 1.0, 1.0));

    script.press(NamedKey::ArrowUp, KeyCode::ArrowUp);
    assert_eq!(script.caret(), (1, 1.0, 0.0), "up keeps the column");
    script.press(NamedKey::ArrowDown, KeyCode::ArrowDown);
    assert_eq!(script.caret(), (4, 1.0, 1.0), "down keeps the column");

    script.press(NamedKey::End, KeyCode::End);
    script.press(NamedKey::Enter, KeyCode::Enter);
    assert_eq!(
        script.lines("notes"),
        3.0,
        "a break at the end adds an empty line"
    );
    assert_eq!(script.caret(), (6, 0.0, 2.0), "and the caret is on it");
}
