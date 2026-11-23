//! The path the engine descends to reach an element that an ancestor's change invalidated, and the
//! marks taken on that path in later frames.

use crate::support::{Harness, radius};

#[test]
fn a_change_below_a_descendant_invalidation_path_restyles_in_a_later_frame() {
    let mut harness = Harness::new();
    let main = harness.append(harness.root, "column");
    let pane = harness.append(main, "column");
    let row = harness.append(pane, "column");
    let message = harness.append(row, "box");
    let actions = harness.append(message, "box");
    harness.set_classes(actions, &["actions"]);
    // The second rule matches `actions` once `main` has the attribute, and gives it the value it
    // already has. The restyle of `actions` then changes nothing and marks nothing.
    harness.add_author(
        "box { color: rgb(1, 1, 1) }
         column[data-showing] .actions { color: rgb(1, 1, 1) }
         .pulled { border-top-left-radius: 7px }",
    );
    harness.frame();
    harness.retire_all();

    // Only `main` is marked. The engine finds `actions` through the rule and descends to it
    // through `pane`, `row` and `message`.
    harness.edit(|edit| {
        edit.set_attribute(
            main,
            zgui_interned::AttrName::new("data-showing"),
            Some(zgui_vocab::SharedString::from("pane")),
        );
    });
    harness.frame();
    harness.retire_all();

    harness.set_classes(message, &["pulled"]);
    harness.frame();
    assert_eq!(
        radius(&harness, message),
        7.0,
        "an element on the path the engine descended restyles when it is marked"
    );
}
