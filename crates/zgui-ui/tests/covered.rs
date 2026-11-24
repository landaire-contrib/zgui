//! An open menu covers the page: the press that closes it reaches nothing under it.

mod desktop;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::{component, view};
use zgui_ui::prelude::*;
use zgui_ui_primitives::dismiss::LayerStack;

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
