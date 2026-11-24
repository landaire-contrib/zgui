//! Each spelling of a line break enters the buffer as one line feed.
//!
//! A carriage return left in a paragraph lays out as a line break the buffer does not hold, and
//! the caret stops on each side of it.

use zgui_edit::Editor;
use zgui_edit::editor::Command;
use zgui_edit::select::Selection;
use zgui_vocab::ImeEvent;

/// The text every case enters, and the text the buffer holds after it.
const ENTERED: &str = "a\r\nb\rc\nd";
const HELD: &str = "a\nb\nc\nd";

#[test]
fn initial_and_loaded_text_hold_line_feeds() {
    let mut editor = Editor::new(ENTERED);
    assert_eq!(editor.text(), HELD);
    assert_eq!(editor.paragraphs(), ["a", "b", "c", "d"]);

    editor.load(ENTERED);
    assert_eq!(editor.text(), HELD);
}

#[test]
fn inserted_text_holds_line_feeds_and_the_caret_lands_after_it() {
    let mut editor = Editor::new("xy");
    editor.set_selection(Selection::caret(1));
    editor.apply(Command::Insert(ENTERED.to_owned()));
    assert_eq!(editor.text(), format!("x{HELD}y"));
    assert_eq!(editor.selection(), Selection::caret(1 + HELD.len()));

    editor.apply(Command::Undo);
    assert_eq!(editor.text(), "xy");
    editor.apply(Command::Redo);
    assert_eq!(editor.text(), format!("x{HELD}y"));
}

#[test]
fn committed_text_holds_line_feeds_with_and_without_a_composition() {
    let mut editor = Editor::new("");
    editor.ime(&ImeEvent::Commit(ENTERED.into()));
    assert_eq!(editor.text(), HELD);
    assert_eq!(editor.selection(), Selection::caret(HELD.len()));

    let mut editor = Editor::new("");
    editor.ime(&ImeEvent::Enabled);
    editor.ime(&ImeEvent::Preedit {
        text: "a".into(),
        cursor: Some(1..1),
    });
    editor.ime(&ImeEvent::Commit(ENTERED.into()));
    assert_eq!(editor.text(), HELD);
    assert_eq!(editor.selection(), Selection::caret(HELD.len()));
}
