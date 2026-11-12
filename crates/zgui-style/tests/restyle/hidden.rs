//! An element whose animation asks for a cascade while an ancestor is `display: none`.
//!
//! A panel that is hidden keeps the animations and transitions of its content in the table. The
//! animation-only traversal stops at the hidden ancestor, so a mark below it is never serviced by
//! that traversal. When the panel is shown again, the ordinary traversal must not meet an
//! animation hint.

use zgui_dom::NodeIndex;

use crate::support::{Harness, animation_frame, width};

/// A width transition and an infinite width animation, which are the cascading tier: a box moves.
const SHEET: &str = "@keyframes grow { from { width: 100px } to { width: 300px } }
                     root { display: block }
                     .panel { display: block }
                     .panel.away { display: none }
                     .field { display: block; width: 100px; transition: width 400ms linear }
                     .field.wide { width: 300px }
                     .spin { display: block; width: 100px; animation: grow 900ms linear infinite }";

/// How long one frame is, in seconds.
const FRAME: f64 = 0.05;

/// A document of root > (panel > field, side), with the field's width transition running.
fn document() -> (Harness, NodeIndex, NodeIndex, NodeIndex) {
    let mut harness = Harness::new();
    harness.add_author(SHEET);
    let root = harness.root;
    let panel = harness.append(root, "box");
    harness.set_classes(panel, &["panel"]);
    let side = harness.append(root, "box");
    harness.set_classes(side, &["panel"]);
    let field = harness.append(panel, "box");
    harness.set_classes(field, &["field"]);
    harness.frame();
    harness.retire_all();
    harness.set_classes(field, &["field", "wide"]);
    harness.frame();
    harness.retire_all();
    (harness, panel, side, field)
}

#[test]
fn a_field_ticked_while_its_panel_is_hidden_is_styled_when_the_panel_opens() {
    let (mut harness, panel, _side, field) = document();
    animation_frame(&mut harness, FRAME);
    let before = width(&harness, field);

    harness.set_classes(panel, &["panel", "away"]);
    animation_frame(&mut harness, 2.0 * FRAME);
    animation_frame(&mut harness, 3.0 * FRAME);
    assert!(
        harness.document.node(field).borrow_style_data().is_none(),
        "a mark gives no style data to an element below a hidden ancestor"
    );

    harness.set_classes(panel, &["panel"]);
    animation_frame(&mut harness, 4.0 * FRAME);

    let after = width(&harness, field);
    assert!(
        after > before && after <= 300.0,
        "the field is styled from its transition when the panel opens: {before} then {after}"
    );
}

#[test]
fn an_infinite_animation_below_a_hidden_panel_is_styled_when_the_panel_opens() {
    let mut harness = Harness::new();
    harness.add_author(SHEET);
    let panel = harness.append(harness.root, "box");
    harness.set_classes(panel, &["panel"]);
    let spin = harness.append(panel, "box");
    harness.set_classes(spin, &["spin"]);
    harness.frame();
    harness.retire_all();
    animation_frame(&mut harness, FRAME);

    harness.set_classes(panel, &["panel", "away"]);
    animation_frame(&mut harness, 2.0 * FRAME);
    animation_frame(&mut harness, 3.0 * FRAME);

    harness.set_classes(panel, &["panel"]);
    animation_frame(&mut harness, 4.0 * FRAME);
    animation_frame(&mut harness, 5.0 * FRAME);

    let after = width(&harness, spin);
    assert!(
        after > 100.0 && after < 300.0,
        "the animation runs on when the panel opens: {after}"
    );
}

#[test]
fn a_field_moved_into_a_hidden_panel_after_its_mark_is_styled_when_the_panel_opens() {
    let (mut harness, panel, side, field) = document();
    harness.set_classes(panel, &["panel", "away"]);
    harness.edit(|edit| edit.insert_before(side, field, None));
    animation_frame(&mut harness, FRAME);
    let before = width(&harness, field);

    let report = harness
        .engine
        .animation_tick(&harness.document, zgui_style::AnimationTime(2.0 * FRAME));
    for element in &report.elements {
        harness
            .engine
            .mark_animation_restyle(&harness.document, element.index);
    }
    harness.edit(|edit| edit.insert_before(panel, field, None));
    harness.engine.restyle(&mut harness.document, None);
    harness.retire_all();

    harness.set_classes(panel, &["panel"]);
    animation_frame(&mut harness, 3.0 * FRAME);

    let after = width(&harness, field);
    assert!(
        after > before && after <= 300.0,
        "the field is styled from its transition when the panel opens: {before} then {after}"
    );
}
