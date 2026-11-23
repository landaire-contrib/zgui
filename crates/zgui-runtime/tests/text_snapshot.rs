//! The snapshot of the focused field a platform's input method edits through, and the selections
//! it sends back, through the whole loop.

mod support;

use zgui_platform::{SurfaceEvent, TextSelection, TextSnapshot};
use zgui_view::{AnyView, BuildCx, IntoView, NodeRef, View};
use zgui_vocab::{
    ImeEvent, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp,
};

/// The sheet the fixture is styled by.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   field { display: block; width: 200px; height: 40px }
                   editor { display: block; width: 40px; height: 120px }";

/// One window holding one editable element, on a surface that asks for snapshots.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// The element.
    element: NodeRef,
}

impl Script {
    /// A window holding what `editable` builds, bound to a node, with focus in it.
    fn new(editable: fn(NodeRef) -> AnyView) -> Self {
        let element = NodeRef::new();
        let harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(editable(element))
                    .into_view()
                    .build(cx),
            )
        });
        let mut script = Self { harness, element };
        script.harness.settle(8);
        script
            .harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface was created")
            .set_text_snapshot_listener(true);
        script.press(NamedKey::Tab, KeyCode::Tab);
        script
    }

    /// Delivers one surface event and lets the frames it produced settle.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    /// Presses a named key.
    fn press(&mut self, key: NamedKey, code: KeyCode) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(key, PhysicalKey::Code(code)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// The last snapshot the surface was given.
    fn snapshot(&self) -> TextSnapshot {
        self.harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface was created")
            .last_text_snapshot()
            .expect("a snapshot was built")
    }

    /// What the framework reports as selected in the element.
    fn selection(&self) -> Option<core::ops::Range<usize>> {
        self.element.selection()
    }
}

#[test]
fn a_focused_field_gives_its_text_selection_and_a_caret_at_each_boundary() {
    let script =
        Script::new(|element| AnyView::new(zgui_elements::field().node_ref(element).child("abc")));
    let snapshot = script.snapshot();

    assert_eq!(snapshot.text, "abc");
    assert_eq!(snapshot.selection, TextSelection::caret(0));
    assert_eq!(snapshot.composition, None);
    assert!(snapshot.bounds.size.width.0 > 0.0);
    let offsets: Vec<_> = snapshot.carets.iter().map(|stop| stop.offset).collect();
    assert_eq!(offsets, vec![0, 1, 2, 3]);
    assert!(
        snapshot
            .carets
            .windows(2)
            .all(|pair| pair[0].rect.origin.x.0 < pair[1].rect.origin.x.0),
        "the carets do not go left to right: {:?}",
        snapshot.carets
    );
    assert!(
        snapshot
            .carets
            .iter()
            .all(|stop| snapshot.bounds.contains(stop.rect.origin)),
        "a caret is outside the field"
    );
}

#[test]
fn a_selection_from_the_input_method_moves_the_caret_and_the_next_snapshot() {
    let mut script =
        Script::new(|element| AnyView::new(zgui_elements::field().node_ref(element).child("abcd")));
    script.deliver(SurfaceEvent::SelectText(TextSelection::new(1, 3)));

    assert_eq!(script.selection(), Some(1..3));
    assert_eq!(script.snapshot().selection, TextSelection::new(1, 3));

    // The commit that follows replaces what was selected, as an autocorrection does.
    script.deliver(SurfaceEvent::Ime(ImeEvent::Commit("X".into())));
    assert_eq!(script.snapshot().text, "aXd");
    assert_eq!(script.snapshot().selection, TextSelection::caret(2));
}

#[test]
fn a_composition_is_in_the_snapshot() {
    let mut script =
        Script::new(|element| AnyView::new(zgui_elements::field().node_ref(element).child("ab")));
    script.deliver(SurfaceEvent::SelectText(TextSelection::caret(2)));
    script.deliver(SurfaceEvent::Ime(ImeEvent::Enabled));
    script.deliver(SurfaceEvent::Ime(ImeEvent::Preedit {
        text: "\u{306b}".into(),
        cursor: Some(3..3),
    }));

    let snapshot = script.snapshot();
    assert_eq!(snapshot.text, "ab\u{306b}");
    assert_eq!(snapshot.composition, Some(2..5));
    assert_eq!(snapshot.selection, TextSelection::caret(5));
}

#[test]
fn a_secure_field_gives_its_mask_and_maps_a_selection_back_to_the_secret() {
    let mut script = Script::new(|element| {
        AnyView::new(
            zgui_elements::field()
                .node_ref(element)
                .secure(true)
                .child("p\u{e9}"),
        )
    });
    let snapshot = script.snapshot();
    assert_eq!(snapshot.text, "\u{2022}\u{2022}");

    // The end of the mask is the end of the secret, which is three bytes long.
    script.deliver(SurfaceEvent::SelectText(TextSelection::caret(6)));
    assert_eq!(script.selection(), Some(3..3));
    assert_eq!(script.snapshot().selection, TextSelection::caret(6));
}

#[test]
fn a_wrapped_line_has_a_caret_at_each_end_of_the_wrap() {
    let script = Script::new(|element| {
        AnyView::new(
            zgui_elements::editor()
                .node_ref(element)
                .child("aaaa bbbb cccc dddd"),
        )
    });
    let snapshot = script.snapshot();
    let lines = snapshot.carets.iter().map(|stop| stop.line).max();
    assert!(
        lines > Some(0),
        "the text did not wrap: {:?}",
        snapshot.carets
    );

    let at_wrap: Vec<_> = snapshot
        .carets
        .windows(2)
        .filter(|pair| pair[0].offset == pair[1].offset)
        .collect();
    assert!(!at_wrap.is_empty(), "no boundary has two carets");
    for pair in at_wrap {
        assert_eq!(pair[0].line + 1, pair[1].line);
    }
}
