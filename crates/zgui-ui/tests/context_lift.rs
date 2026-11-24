//! A context menu that a finger opens below a lifted copy of its region.

mod desktop;

use std::time::Duration;

use zgui::geom::{Device, DevicePx, Point, Rect};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, UnsyncCallback};
use zgui::view::AttrName;
use zgui::{component, view};
use zgui_ui::context_menu::is_lifted;
use zgui_ui::prelude::*;

use crate::desktop::census::Seen;
use crate::desktop::stage::{HEIGHT, Stage};

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 200px 24px 24px; gap: 16px; align-items: flex-start; height: 100% }
                     .spacer { flex: 1 }
                     .area { width: 300px; height: 60px }
                     .copy { height: 60px }
                     .bar { width: 240px; height: 40px }";

/// How long a long press and the entrance after it take, with room to spare.
const HELD: Duration = Duration::from_millis(1200);

/// The items that were chosen, in order.
type Chosen = RwSignal<Vec<&'static str>, LocalStorage>;

/// A handler that records `name` in `chosen`.
fn record(chosen: Chosen, name: &'static str) -> UnsyncCallback<()> {
    UnsyncCallback::new(move |()| chosen.update(|all| all.push(name)))
}

/// A region with a lifting context menu, at the top of the page or, when `low`, at the bottom.
#[component]
fn Lifting(chosen: Chosen, low: bool) -> impl IntoView {
    let spacer = low.then(|| view! { box(class = "spacer") });
    view! {
        column(class = "page") {
            {spacer}
            ContextMenu {
                ContextMenuTrigger(class = "area", attr:data-test = "area") {"Hold here"}
                ContextMenuContent {
                    ContextMenuPreview(slot) {
                        box(class = "copy", attr:data-test = "copy") {"The copy"}
                    }
                    ContextMenuAccessory(slot) {
                        box(class = "bar") {"Reactions"}
                    }
                    MenuItem(on_select = record(chosen, "Copy")) {"Copy"}
                    MenuItem(on_select = record(chosen, "Paste")) {"Paste"}
                    MenuItem {{if is_lifted() { "Lifted" } else { "At the point" }}}
                }
            }
        }
    }
}

/// The innermost laid-out node saying `text`.
fn innermost(stage: &Stage, text: &str) -> Seen {
    stage
        .census()
        .nodes
        .into_iter()
        .filter(|seen| seen.text == text && seen.area() > 0.0)
        .max_by_key(|seen| seen.depth)
        .unwrap_or_else(|| panic!("nothing laid out says {text:?}"))
}

/// The box of the innermost laid-out node saying `text`.
fn box_of(stage: &Stage, text: &str) -> Rect<DevicePx, Device> {
    innermost(stage, text)
        .rect
        .expect("a laid-out node has a box")
}

/// The node whose `data-test` attribute is `name`.
fn tagged(stage: &Stage, name: &str) -> Seen {
    stage
        .census()
        .nodes
        .into_iter()
        .find(|seen| {
            stage
                .handles()
                .dom
                .attribute_is(seen.id, AttrName::new("data-test"), name)
        })
        .unwrap_or_else(|| panic!("no node is tagged {name:?}"))
}

/// The centre of `rect`.
fn centre(rect: Rect<DevicePx, Device>) -> (f32, f32) {
    (
        rect.origin.x.0 + rect.size.width.0 / 2.0,
        rect.origin.y.0 + rect.size.height.0 / 2.0,
    )
}

/// The box of the menu panel, which holds both items.
fn menu_box(stage: &Stage) -> Rect<DevicePx, Device> {
    let census = stage.census();
    census
        .nodes
        .iter()
        .filter(|seen| {
            seen.text.contains("Copy") && seen.text.contains("Paste") && seen.area() > 0.0
        })
        .max_by_key(|seen| seen.depth)
        .and_then(|seen| seen.rect)
        .expect("the menu is laid out")
}

/// Opens the stage and holds a finger on the region until the menu is up.
fn held(chosen: Chosen, low: bool) -> Stage {
    let mut stage = Stage::open(SHEET, move || view! { Lifting(chosen = chosen, low = low) });
    stage.settle();
    stage.touch_saying("Hold here");
    stage.hold(HELD);
    stage
}

/// A point on the page away from every part of the menu.
fn elsewhere() -> Point<DevicePx, Device> {
    Point::new(DevicePx(1100.0), DevicePx(450.0))
}

#[test]
fn a_long_press_lifts_a_copy_with_the_accessory_above_and_the_items_below() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(
        SHEET,
        move || view! { Lifting(chosen = chosen, low = false) },
    );
    stage.settle();
    let region = tagged(&stage, "area").rect.expect("the region is laid out");

    stage.touch_saying("Hold here");
    stage.hold(HELD);
    assert!(stage.shows("Copy"), "the long press did not open the menu");
    assert!(
        stage.shows("Lifted"),
        "the items do not know they are lifted"
    );

    let copy = tagged(&stage, "copy").rect.expect("the copy is laid out");
    let bar = box_of(&stage, "Reactions");
    let menu = menu_box(&stage);
    // The copy is drawn a little larger than the region, about its centre.
    let (copy_x, copy_y) = centre(copy);
    let (region_x, region_y) = centre(region);
    assert!(
        (copy_x - region_x).abs() <= 1.0 && (copy_y - region_y).abs() <= 1.0,
        "the copy stands where the region is; region {region:?}, copy {copy:?}"
    );
    assert!(
        bar.origin.y.0 + bar.size.height.0 <= copy.origin.y.0,
        "the accessory is above the copy"
    );
    assert!(
        menu.origin.y.0 >= copy.origin.y.0 + copy.size.height.0,
        "the items are below the copy"
    );

    let area = tagged(&stage, "area").id;
    assert!(
        stage
            .handles()
            .dom
            .attribute_is(area, AttrName::new("data-zui-lifted"), ""),
        "the region is hidden while the copy stands in for it"
    );
}

#[test]
fn a_slide_onto_an_item_and_a_lift_chooses_it() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = held(chosen, false);

    stage.slide_to_saying("Paste");
    stage.lift();
    stage.hold(HELD);
    assert_eq!(chosen.get_untracked(), ["Paste"]);
    assert!(!stage.shows("The copy"), "the choice left the copy up");
}

#[test]
fn a_tap_outside_closes_it_and_shows_the_region_again() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = held(chosen, false);
    stage.lift();
    stage.settle();
    assert!(stage.shows("Copy"), "a lift in place closed the menu");

    stage.touch(elsewhere());
    stage.lift();
    stage.hold(HELD);
    assert!(!stage.shows("Copy"), "the tap outside left the menu open");
    assert!(chosen.get_untracked().is_empty());
    let area = tagged(&stage, "area").id;
    assert!(
        !stage
            .handles()
            .dom
            .attribute_is(area, AttrName::new("data-zui-lifted"), "")
    );
}

#[test]
fn a_tap_on_the_copy_closes_it() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = held(chosen, false);
    stage.lift();
    stage.settle();

    stage.touch_saying("The copy");
    stage.lift();
    stage.hold(HELD);
    assert!(!stage.shows("Copy"));
}

#[test]
fn near_the_bottom_the_items_go_above() {
    let chosen = RwSignal::new_local(Vec::new());
    let stage = held(chosen, true);

    let copy = box_of(&stage, "The copy");
    let bar = box_of(&stage, "Reactions");
    let menu = menu_box(&stage);
    assert!(
        copy.origin.y.0 + copy.size.height.0 <= HEIGHT,
        "the copy is on screen"
    );
    assert!(
        menu.origin.y.0 + menu.size.height.0 <= bar.origin.y.0,
        "the items are above the accessory"
    );
    assert!(
        bar.origin.y.0 + bar.size.height.0 <= copy.origin.y.0,
        "which is above the copy"
    );
}

#[test]
fn a_mouse_opens_the_items_at_the_pointer_without_a_copy() {
    let chosen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(
        SHEET,
        move || view! { Lifting(chosen = chosen, low = false) },
    );
    stage.settle();

    stage.move_saying("Hold here");
    stage.press_secondary();
    stage.hold(HELD);
    assert!(stage.shows("Copy"), "the right click did not open the menu");
    assert!(stage.shows("At the point"));
    assert!(!stage.shows("The copy"), "a mouse lifted the region");
}
