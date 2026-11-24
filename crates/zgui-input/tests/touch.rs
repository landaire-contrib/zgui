//! A finger and `:hover`.

mod support;

use support::{Element, Fixture, Session};
use zgui_vocab::{PointerAction, UiState};

/// Two rows, one above the other.
fn rows() -> Fixture {
    Fixture::new(
        Element::new("root").children(vec![
            Element::new("row").class("first"),
            Element::new("cell"),
        ]),
        "root { display: block; width: 300px }
         row, cell { display: block; height: 40px }",
    )
}

/// Whether the element named `name` carries `:hover`.
fn hovered(session: &Session, name: &str) -> bool {
    let index = session.fixture.find(name);
    session
        .fixture
        .document
        .store()
        .core(index)
        .ui_state()
        .contains(UiState::HOVER)
}

#[test]
fn a_finger_arrives_on_and_leaves_elements_without_hovering_them() {
    let mut session = Session::new(rows());
    let row = session.fixture.key("row");
    let cell = session.fixture.key("cell");

    let pressed = session.touch_at(session.fixture.centre_of("row"), PointerAction::Pressed);
    assert!(
        pressed.entered.contains(&row),
        "the row hears the finger arrive"
    );
    assert!(!hovered(&session, "row"), "and a finger does not hover it");

    let moved = session.touch_at(session.fixture.centre_of("cell"), PointerAction::Moved);
    assert!(moved.left.contains(&row));
    assert!(
        moved.entered.contains(&cell),
        "a slide onto the cell reaches the cell"
    );
    assert!(!hovered(&session, "cell"));

    let lifted = session.touch_at(session.fixture.centre_of("cell"), PointerAction::Left);
    assert!(lifted.left.contains(&cell));
}

#[test]
fn a_mouse_after_a_finger_hovers_and_a_finger_after_a_mouse_clears_it() {
    let mut session = Session::new(rows());

    session.hover("row");
    assert!(hovered(&session, "row"), "a mouse hovers");

    session.touch_at(session.fixture.centre_of("cell"), PointerAction::Pressed);
    assert!(
        !hovered(&session, "row"),
        "a finger takes the hover off what the mouse was over"
    );
    assert!(!hovered(&session, "cell"));

    session.hover("cell");
    assert!(
        hovered(&session, "cell"),
        "and the mouse hovers again once it moves"
    );
}

/// Re-tests the hover as a frame that changed geometry does.
fn rehit(session: &mut Session) {
    let filter = session.fixture.filter();
    let world = session.fixture.world(&filter);
    session.router.rehit(&world);
}

#[test]
fn a_frame_that_retests_a_held_finger_does_not_hover() {
    let mut session = Session::new(rows());
    session.touch_at(session.fixture.centre_of("row"), PointerAction::Pressed);
    rehit(&mut session);
    assert!(!hovered(&session, "row"));
}

#[test]
fn a_frame_that_retests_keeps_the_hover_of_a_mouse_beside_a_held_finger() {
    let mut session = Session::new(rows());
    session.touch_at(session.fixture.centre_of("cell"), PointerAction::Pressed);
    session.hover("row");
    rehit(&mut session);
    assert!(hovered(&session, "row"), "the mouse still hovers the row");
    assert!(
        !hovered(&session, "cell"),
        "and the hover is not moved under the finger"
    );
}
