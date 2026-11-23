//! What VoiceOver on iOS stops on, for the shapes an application is built from.
//!
//! The tree is applied through `accesskit_consumer`, and the stops are read by the rules of the
//! iOS adapter. A row whose text is in the tree and a row VoiceOver can reach are different
//! claims: the adapter makes a node with no focusable children one element, and a list of rows
//! with nothing focusable in them is then one element with the name of the list.

mod desktop;

use zgui::reactive::Signal;
use zgui::view;
use zgui::vocab::Role;
use zgui_ui::prelude::*;

use crate::desktop::reader;
use crate::desktop::reader::Stop;
use crate::desktop::stage::Stage;

/// A list a few rows tall, inside a page much taller.
const SHEET: &str = ".list { height: 200px }";

/// How many entries every list here holds.
const ENTRIES: usize = 1000;

/// A stop that says `value`, as a static text element says it.
fn static_text(value: &str) -> Stop {
    Stop {
        role: "Label".to_owned(),
        label: String::new(),
        value: value.to_owned(),
    }
}

/// A stop that is a button called `label`.
fn button_named(label: &str) -> Stop {
    Stop {
        role: "Button".to_owned(),
        label: label.to_owned(),
        value: String::new(),
    }
}

#[test]
fn the_rows_a_virtual_list_built_are_stops_and_carry_their_place_in_the_whole() {
    let stage = Stage::open(SHEET, || {
        view! {
            VirtualList(
                class = "list",
                count = Signal::stored_local(ENTRIES),
                row_size = 40.0_f32,
                overscan = 2_usize,
                label = "Entries",
                row = |index: usize| view! {
                    row {
                        text {{format!("Title {index}")}}
                        text {{format!("user{index}@example.com")}}
                    }
                }
            )
        }
    });

    let stops = reader::ios_stops(&stage.surface());
    assert_eq!(
        stops[..4],
        [
            static_text("Title 0"),
            static_text("user0@example.com"),
            static_text("Title 1"),
            static_text("user1@example.com"),
        ],
        "{stops:#?}"
    );
    assert!(
        stops.iter().all(|stop| stop.role != "List"),
        "the list is a container VoiceOver walks into: {stops:#?}"
    );

    let tree = reader::consumed(&stage.surface()).expect("the window published a tree");
    let state = tree.state();
    let mut pending = vec![state.root()];
    let mut items = Vec::new();
    while let Some(node) = pending.pop() {
        if format!("{:?}", node.role()) == "ListItem" {
            items.push((node.position_in_set(), node.size_of_set()));
        }
        pending.extend(node.children());
    }
    assert!(
        items.contains(&(Some(1), Some(ENTRIES))),
        "each row says where it is in the whole list: {items:?}"
    );
}

#[test]
fn a_row_the_application_calls_a_button_is_one_stop_named_by_its_text() {
    let stage = Stage::open(SHEET, || {
        view! {
            VirtualList(
                class = "list",
                count = Signal::stored_local(ENTRIES),
                row_size = 40.0_f32,
                overscan = 2_usize,
                label = "Entries",
                row = |index: usize| view! {
                    row(a11y:role = Role::Button, on:click = |_| {}) {
                        text {{format!("Title {index}")}}
                        text {{format!("user{index}@example.com")}}
                    }
                }
            )
        }
    });

    let stops = reader::ios_stops(&stage.surface());
    assert_eq!(
        stops[..2],
        [
            button_named("Title 0 user0@example.com"),
            button_named("Title 1 user1@example.com"),
        ],
        "{stops:#?}"
    );
}

#[test]
fn a_detail_reads_its_labels_and_values_and_each_button_says_what_it_copies() {
    let stage = Stage::open(SHEET, || {
        view! {
            column {
                text {"User name"}
                text {"lander@example.com"}
                Button(a11y:label = "Copy user name") {"Copy"}
                text {"Password"}
                text {"\u{2022}\u{2022}\u{2022}\u{2022}"}
                Button(a11y:label = "Copy password") {"Copy"}
            }
        }
    });

    assert_eq!(
        reader::ios_stops(&stage.surface()),
        [
            static_text("User name"),
            static_text("lander@example.com"),
            button_named("Copy user name"),
            static_text("Password"),
            static_text("\u{2022}\u{2022}\u{2022}\u{2022}"),
            button_named("Copy password"),
        ]
    );
}

#[test]
fn a_password_field_is_named_by_its_placeholder_and_says_only_bullets() {
    let stage = Stage::open(SHEET, || {
        view! {
            Input(
                default_value = "hunter2",
                placeholder = "Master password",
                secure = Signal::stored_local(true)
            )
        }
    });

    let stops = reader::ios_stops(&stage.surface());
    assert_eq!(
        stops,
        [Stop {
            role: "PasswordInput".to_owned(),
            label: "Master password".to_owned(),
            value: "\u{2022}".repeat(7),
        }]
    );
}
