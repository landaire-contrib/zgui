//! A menu item is chosen by the release of the pointer over it, for a finger and for a mouse.

mod desktop;

use std::time::Duration;

use zgui::geom::{Device, DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }
                     .area { width: 300px; height: 120px }";

/// The items that were chosen, in order.
type Chosen = RwSignal<Vec<&'static str>, LocalStorage>;

/// A handler that records `name` in `chosen`.
fn record(chosen: Chosen, name: &'static str) -> UnsyncCallback<()> {
    UnsyncCallback::new(move |()| chosen.update(|all| all.push(name)))
}

/// A dropdown menu and a context menu, each with items that record their choice.
#[component]
fn Menus(chosen: Chosen) -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Open menu"}
                DropdownMenuContent {
                    MenuItem(on_select = record(chosen, "Duplicate")) {"Duplicate"}
                    MenuItem(on_select = record(chosen, "Archive")) {"Archive"}
                    MenuItem(on_select = record(chosen, "Delete")) {"Delete"}
                }
            }
            ContextMenu {
                ContextMenuTrigger(class = "area") {"Hold here"}
                ContextMenuContent {
                    MenuItem(on_select = record(chosen, "Copy")) {"Copy"}
                    MenuItem(on_select = record(chosen, "Paste")) {"Paste"}
                }
            }
        }
    }
}

/// A point on the page away from every control.
fn elsewhere() -> Point<DevicePx, Device> {
    Point::new(DevicePx(1100.0), DevicePx(800.0))
}

/// Opens the stage with the dropdown menu open.
fn opened(chosen: Chosen) -> Stage {
    let mut stage = Stage::open(SHEET, move || view! { Menus(chosen = chosen) });
    stage.click_saying("Open menu");
    stage.settle();
    assert!(stage.shows("Duplicate"), "the menu did not open");
    stage
}

/// Whether the focused item has the `data-highlighted` attribute and says `text`.
fn highlights(stage: &Stage, text: &str) -> bool {
    stage.focused().is_some_and(|node| {
        stage.handles().dom.attribute_is(
            node,
            zgui::view::AttrName::new("data-highlighted"),
            "true",
        )
    }) && stage.focused_text() == text
}

#[test]
fn a_touch_highlights_the_item_and_the_lift_chooses_it() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = opened(chosen);

    stage.touch_saying("Duplicate");
    stage.settle();
    assert!(
        chosen.get_untracked().is_empty(),
        "the touch chose the item"
    );
    assert!(
        highlights(&stage, "Duplicate"),
        "the item under the finger has no highlight; focus is on {:?}",
        stage.focused_text()
    );

    stage.lift();
    stage.settle();
    assert_eq!(chosen.get_untracked(), ["Duplicate"]);
    assert!(!stage.shows("Duplicate"), "the choice left the menu open");
}

#[test]
fn a_finger_that_slides_to_another_item_chooses_that_item() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = opened(chosen);

    stage.touch_saying("Duplicate");
    stage.slide_to_saying("Delete");
    stage.settle();
    assert!(chosen.get_untracked().is_empty(), "the slide chose an item");
    assert!(
        highlights(&stage, "Delete"),
        "the highlight did not follow the finger; focus is on {:?}",
        stage.focused_text()
    );

    stage.lift();
    stage.settle();
    assert_eq!(chosen.get_untracked(), ["Delete"]);
}

#[test]
fn a_finger_that_lifts_outside_the_menu_chooses_nothing() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = opened(chosen);

    stage.touch_saying("Duplicate");
    stage.slide(elsewhere());
    stage.lift();
    stage.settle();
    assert!(
        chosen.get_untracked().is_empty(),
        "the lift outside chose an item"
    );
    assert!(stage.shows("Duplicate"), "the lift outside closed the menu");
}

#[test]
fn a_finger_can_press_the_trigger_and_slide_onto_an_item() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(SHEET, move || view! { Menus(chosen = chosen) });

    stage.touch_saying("Open menu");
    stage.settle();
    assert!(stage.shows("Archive"), "the touch did not open the menu");
    stage.slide_to_saying("Archive");
    stage.lift();
    stage.settle();
    assert_eq!(chosen.get_untracked(), ["Archive"]);
}

#[test]
fn a_long_press_opens_the_context_menu_and_a_slide_and_lift_chooses() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(SHEET, move || view! { Menus(chosen = chosen) });

    stage.touch_saying("Hold here");
    stage.hold(Duration::from_millis(700));
    assert!(
        stage.shows("Paste"),
        "the long press did not open the context menu"
    );

    stage.slide_to_saying("Paste");
    stage.lift();
    stage.settle();
    assert_eq!(chosen.get_untracked(), ["Paste"]);
}

#[test]
fn a_lift_in_place_after_a_long_press_chooses_nothing() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(SHEET, move || view! { Menus(chosen = chosen) });

    stage.touch_saying("Hold here");
    stage.hold(Duration::from_millis(700));
    assert!(
        stage.shows("Copy"),
        "the long press did not open the context menu"
    );

    // A finger never stays still. It drifts onto the first item, by less than the travel that lets
    // a release choose.
    stage.slide_by(6.0, 6.0);
    stage.settle();
    assert_eq!(
        stage.focused_text(),
        "Copy",
        "the drift did not reach the first item, so the lift below tests nothing"
    );

    stage.lift();
    stage.settle();
    assert!(
        chosen.get_untracked().is_empty(),
        "the lift in place chose an item"
    );
    assert!(
        stage.shows("Copy"),
        "the lift in place closed the context menu"
    );
}

#[test]
fn a_mouse_chooses_on_the_release_over_the_item() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = opened(chosen);

    stage.press_saying("Duplicate");
    stage.settle();
    assert!(
        chosen.get_untracked().is_empty(),
        "the press chose the item"
    );

    stage.release();
    stage.settle();
    assert_eq!(
        chosen.get_untracked(),
        ["Duplicate"],
        "the release chose other than once"
    );
}

#[test]
fn a_mouse_that_moves_to_another_item_chooses_it_and_away_chooses_nothing() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = opened(chosen);

    stage.press_saying("Duplicate");
    stage.move_saying("Archive");
    stage.release();
    stage.settle();
    assert_eq!(chosen.get_untracked(), ["Archive"]);

    stage.click_saying("Open menu");
    stage.settle();
    stage.press_saying("Duplicate");
    stage.move_to(elsewhere());
    stage.release();
    stage.settle();
    assert_eq!(
        chosen.get_untracked(),
        ["Archive"],
        "the release away chose an item"
    );
}

#[test]
fn a_mouse_can_press_the_trigger_and_release_on_an_item() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(SHEET, move || view! { Menus(chosen = chosen) });

    stage.press_saying("Open menu");
    stage.settle();
    stage.move_saying("Delete");
    stage.release();
    stage.settle();
    assert_eq!(chosen.get_untracked(), ["Delete"]);
}

/// A menu taller than its panel, which scrolls.
#[component]
fn LongMenu(chosen: Chosen) -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Open menu"}
                DropdownMenuContent {
                    MenuItem(on_select = record(chosen, "One")) {"One"}
                    MenuItem(on_select = record(chosen, "Two")) {"Two"}
                    MenuItem(on_select = record(chosen, "Three")) {"Three"}
                    MenuItem(on_select = record(chosen, "Four")) {"Four"}
                    MenuItem(on_select = record(chosen, "Five")) {"Five"}
                    MenuItem(on_select = record(chosen, "Six")) {"Six"}
                    MenuItem(on_select = record(chosen, "Seven")) {"Seven"}
                    MenuItem(on_select = record(chosen, "Eight")) {"Eight"}
                    MenuItem(on_select = record(chosen, "Nine")) {"Nine"}
                    MenuItem(on_select = record(chosen, "Ten")) {"Ten"}
                }
            }
        }
    }
}

#[test]
fn a_finger_that_scrolls_a_long_menu_chooses_nothing() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(SHEET, move || view! { LongMenu(chosen = chosen) });
    // A short window, so that the menu is taller than the room under its trigger and scrolls.
    stage.deliver_resize(1200.0, 220.0);
    stage.settle();
    stage.click_saying("Open menu");
    stage.settle();
    assert!(stage.shows("Three"), "the menu did not open");

    let top = |stage: &Stage| stage.census().control("One").and_then(|node| node.centre());
    let before = top(&stage);
    stage.touch_saying("Three");
    stage.slide_by(0.0, -40.0);
    stage.settle();
    assert_ne!(
        top(&stage),
        before,
        "the panel did not scroll with the finger, so the lift below tests nothing"
    );
    // The finger comes to rest over an item, which hears it move.
    stage.slide_by(0.0, 2.0);
    stage.slide_by(0.0, 2.0);
    stage.lift();
    stage.settle();
    assert!(
        chosen.get_untracked().is_empty(),
        "the lift after the scroll chose {:?}",
        chosen.get_untracked()
    );
}
