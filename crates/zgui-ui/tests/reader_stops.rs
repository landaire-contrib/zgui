//! What VoiceOver on iOS stops on, for the shapes an application is built from.
//!
//! The tree is applied through `accesskit_consumer`, and the stops are read by the rules of the
//! iOS adapter. A row whose text is in the tree and a row VoiceOver can reach are different
//! claims: the adapter makes a node with no focusable children one element, and a list of rows
//! with nothing focusable in them is then one element with the name of the list.

mod desktop;

use std::time::Duration;

use zgui::component;
use zgui::prelude::*;
use zgui::reactive::Signal;
use zgui::view;
use zgui::vocab::NamedKey;
use zgui::vocab::Role;
use zgui_ui::prelude::*;
use zgui_ui::toast::{Toast, use_toaster};
use zgui_ui_primitives::dismiss::LayerStack;

use crate::desktop::reader;
use crate::desktop::reader::Announced;
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
        disabled: false,
    }
}

/// A stop that is a button called `label`.
fn button_named(label: &str) -> Stop {
    Stop {
        role: "Button".to_owned(),
        label: label.to_owned(),
        value: String::new(),
        disabled: false,
    }
}

/// A stop that is a menu item called `label`, which VoiceOver on iOS reads as a button.
fn menu_item(label: &str) -> Stop {
    Stop {
        role: "MenuItem".to_owned(),
        label: label.to_owned(),
        value: String::new(),
        disabled: false,
    }
}

/// A stop that is a menu item called `label` with a tick, which VoiceOver reads as on.
fn ticked_menu_item(label: &str) -> Stop {
    Stop {
        role: "MenuItemCheckBox".to_owned(),
        label: label.to_owned(),
        value: "1".to_owned(),
        disabled: false,
    }
}

/// The items every menu here holds: one plain, one dimmed, and one ticked.
#[component]
fn Items() -> impl IntoView {
    view! {
        MenuItem {"Archive"}
        MenuItem(disabled = true) {"Restore"}
        MenuCheckboxItem(checked = RwSignal::new_local(true)) {"Pinned"}
    }
}

/// What VoiceOver stops on in an open menu of [`Items`].
fn open_items() -> [Stop; 3] {
    [
        menu_item("Archive"),
        Stop {
            disabled: true,
            ..menu_item("Restore")
        },
        ticked_menu_item("Pinned"),
    ]
}

/// A page that is inert while an open layer covers it, as an application writes one.
#[component]
fn Covered(children: Children) -> impl IntoView {
    let stack = LayerStack::current();
    let covered = move || stack.covering().then(String::new);
    view! {
        column(attr:inert = covered) {
            {children.into_view_once()}
        }
    }
}

/// What a reader says of a menu item called `name`.
fn announced_item(name: &str) -> Option<Announced> {
    Some(Announced {
        role: "MenuItem".to_owned(),
        name: name.to_owned(),
    })
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
            disabled: false,
        }]
    );
}

#[test]
fn an_open_dropdown_menu_is_its_items_with_their_states_and_holds_the_focus() {
    let mut stage = Stage::open(SHEET, || {
        view! {
            Covered {
                DropdownMenu {
                    DropdownMenuTrigger {"Actions"}
                    DropdownMenuContent {Items()}
                }
            }
        }
    });
    assert!(!stage.surface().escape_dismisses());

    stage.activate("Actions");

    assert_eq!(reader::ios_stops(&stage.surface()), open_items());
    assert_eq!(stage.announced_focus(), announced_item("Archive"));
    assert!(
        stage.surface().escape_dismisses(),
        "the escape gesture has a menu to close"
    );

    // What the escape gesture delivers.
    stage.key(NamedKey::Escape);
    stage.hold(Duration::from_millis(600));
    assert!(!stage.shows("Archive"), "the menu stayed open");
    assert!(!stage.surface().escape_dismisses());
    assert_eq!(
        reader::ios_stops(&stage.surface()),
        [button_named("Actions")]
    );
}

#[test]
fn a_reader_reaches_nothing_outside_an_open_menu_over_a_live_page() {
    let mut stage = Stage::open(SHEET, || {
        view! {
            column {
                text {"Passwords"}
                DropdownMenu {
                    DropdownMenuTrigger {"Actions"}
                    DropdownMenuContent {Items()}
                }
                Button {"Play"}
            }
        }
    });
    let page = [
        static_text("Passwords"),
        button_named("Actions"),
        button_named("Play"),
    ];
    assert_eq!(reader::ios_stops(&stage.surface()), page);

    stage.activate("Actions");
    assert_eq!(reader::ios_stops(&stage.surface()), open_items());

    stage.key(NamedKey::Escape);
    stage.hold(Duration::from_millis(600));
    assert_eq!(
        reader::ios_stops(&stage.surface()),
        page,
        "the page comes back whole once the menu has closed"
    );
}

/// A button that announces "Copied".
#[component]
fn Notify() -> impl IntoView {
    let toasts = use_toaster();
    view! {
        Button(on:click = move |_| {
            if let Some(toasts) = toasts {
                toasts.push(Toast::new("Copied"));
            }
        }) {"Notify"}
    }
}

#[test]
fn a_notice_stays_readable_over_an_open_menu() {
    let mut stage = Stage::open(SHEET, || {
        view! {
            Toaster {
                column {
                    Notify()
                    DropdownMenu {
                        DropdownMenuTrigger {"Actions"}
                        DropdownMenuContent {Items()}
                    }
                }
            }
        }
    });
    stage.activate("Notify");
    stage.activate("Actions");

    let stops = reader::ios_stops(&stage.surface());
    assert_eq!(stops[..3], open_items(), "{stops:#?}");
    assert!(
        stops
            .iter()
            .any(|stop| stop.label.contains("Copied") || stop.value.contains("Copied")),
        "the notice is out of the reader's reach: {stops:#?}"
    );
    assert!(
        !stops.contains(&button_named("Notify")),
        "the page is in the reader's reach: {stops:#?}"
    );
}

#[test]
fn an_open_context_menu_is_its_items_with_their_states_and_holds_the_focus() {
    let mut stage = Stage::open(SHEET, || {
        view! {
            Covered {
                ContextMenu {
                    ContextMenuTrigger {"Gmail"}
                    ContextMenuContent {Items()}
                }
            }
        }
    });

    // A long press, as VoiceOver passes through a double tap held down.
    stage.touch_saying("Gmail");
    stage.hold(Duration::from_millis(700));
    stage.lift();
    stage.settle();

    assert_eq!(reader::ios_stops(&stage.surface()), open_items());
    assert_eq!(stage.announced_focus(), announced_item("Archive"));
    assert!(stage.surface().escape_dismisses());

    stage.key(NamedKey::Escape);
    stage.hold(Duration::from_millis(600));
    assert!(!stage.shows("Archive"), "the menu stayed open");
    assert!(!stage.surface().escape_dismisses());
}
