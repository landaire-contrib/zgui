//! An open menu covers the page: the press that closes it reaches nothing under it, whether or not
//! the page is inert.

mod desktop;

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::UnsyncCallback;
use zgui::vocab::NamedKey;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_primitives::dismiss::{DismissReason, DismissableLayerProps, LayerStack};

use crate::desktop::stage::Stage;

const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// A page that is inert while a layer covers it, with a menu and a button that counts its presses.
#[component]
fn Page(pressed: RwSignal<u32, LocalStorage>) -> impl IntoView {
    let stack = LayerStack::current();
    let covered = move || stack.covering().then(String::new);

    view! {
        column(class = "page", attr:inert = covered) {
            DropdownMenu {
                DropdownMenuTrigger {"Open menu"}
                DropdownMenuContent {
                    MenuItem {"Archive"}
                }
            }
            Button(on:click = move |_| pressed.update(|count| *count += 1)) {"Play"}
        }
    }
}

/// A page with a menu and a button that counts its presses, and nothing inert.
#[component]
fn Plain(pressed: RwSignal<u32, LocalStorage>) -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Open menu"}
                DropdownMenuContent {
                    MenuItem {"Archive"}
                }
            }
            Button(on:click = move |_| pressed.update(|count| *count += 1)) {"Play"}
        }
    }
}

/// A page with a layer that leaves the document as soon as it is dismissed, and a button that
/// counts its presses. The button comes first, so it stays where it is when the layer leaves.
#[component]
fn Abrupt(pressed: RwSignal<u32, LocalStorage>) -> impl IntoView {
    let open = RwSignal::new_local(true);
    let dismiss = UnsyncCallback::new(move |_reason: DismissReason| open.set(false));

    view! {
        column(class = "page") {
            Button(on:click = move |_| pressed.update(|count| *count += 1)) {"Play"}
            Show(when = move || open.get(), fallback = || view! { box() }) {
                DismissableLayer(on_dismiss = dismiss, covers = true) {
                    box {"Archive"}
                }
            }
        }
    }
}

#[test]
fn a_tap_past_an_open_menu_closes_it_and_presses_nothing_under_it() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Page(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();
    assert!(stage.shows("Archive"), "the menu did not open");

    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert!(
        !stage.shows("Archive"),
        "the tap past the menu left it open"
    );
    assert_eq!(
        pressed.get_untracked(),
        0,
        "the tap that closed the menu pressed the button"
    );

    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        1,
        "the next tap does not press the button"
    );
}

#[test]
fn a_click_past_an_open_menu_closes_it_and_presses_nothing_under_it() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Page(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.click_saying("Play");
    stage.settle();
    assert!(!stage.shows("Archive"));
    assert_eq!(pressed.get_untracked(), 0);
}

#[test]
fn a_tap_past_an_open_menu_over_a_live_page_closes_it_and_presses_nothing_under_it() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Plain(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();
    assert!(stage.shows("Archive"), "the menu did not open");

    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert!(
        !stage.shows("Archive"),
        "the tap past the menu left it open"
    );
    assert_eq!(
        pressed.get_untracked(),
        0,
        "the tap that closed the menu pressed the button"
    );

    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        1,
        "the next tap does not press the button"
    );
}

#[test]
fn a_click_past_an_open_menu_over_a_live_page_closes_it_and_presses_nothing_under_it() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Plain(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.click_saying("Play");
    stage.settle();
    assert!(!stage.shows("Archive"));
    assert_eq!(pressed.get_untracked(), 0);

    stage.click_saying("Play");
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        1,
        "the next click does not press the button"
    );
}

#[test]
fn the_release_of_a_tap_that_closed_a_layer_is_consumed_after_the_layer_is_gone() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Abrupt(pressed = pressed) });
    assert!(stage.shows("Archive"));

    stage.touch_saying("Play");
    assert!(
        !stage.shows("Archive"),
        "the layer is still in the document when the finger lifts"
    );
    stage.lift();
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        0,
        "the release after the layer left pressed the button"
    );

    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert_eq!(pressed.get_untracked(), 1);
}

#[test]
fn a_cancelled_tap_that_closed_a_menu_leaves_the_next_tap_alone() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Plain(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.touch_saying("Play");
    stage.cancel_touch();
    stage.settle();
    assert!(!stage.shows("Archive"));
    assert_eq!(pressed.get_untracked(), 0);

    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        1,
        "the cancelled press still holds back the next release"
    );
}

#[test]
fn a_click_that_closed_a_menu_and_let_go_outside_the_window_leaves_the_next_click_alone() {
    let pressed = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { Plain(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.move_saying("Play");
    stage.press();
    stage.move_to(Point::new(DevicePx(-40.0), DevicePx(-40.0)));
    stage.release();
    stage.settle();
    assert!(!stage.shows("Archive"));

    stage.click_saying("Play");
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        1,
        "the release that never reached the window holds back the next one"
    );
}

#[test]
fn a_tap_past_a_menu_playing_its_exit_presses_what_it_lands_on() {
    const SLOW_EXIT: &str = ":root { --zui-surface-exit-duration: 2s }";

    let pressed = RwSignal::new_local(0_u32);
    let sheet = format!("{SHEET}\n{SLOW_EXIT}");
    let mut stage = Stage::open(&sheet, move || view! { Plain(pressed = pressed) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.key(NamedKey::Escape);
    assert!(stage.shows("Archive"), "the menu has no exit to play");
    stage.touch_saying("Play");
    stage.lift();
    stage.settle();
    assert_eq!(
        pressed.get_untracked(),
        1,
        "the menu that was already closing took the tap"
    );
}
