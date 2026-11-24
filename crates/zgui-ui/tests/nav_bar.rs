//! How a navigation stack's bar shares its width between the back button and the title, laid out
//! by the real engine.

mod desktop;

use core::cell::Cell;
use std::rc::Rc;

use zgui::geom::Device;
use zgui::geom::DevicePx;
use zgui::geom::Point;
use zgui::geom::Rect;
use zgui::view;
use zgui::view::AttrName;
use zgui::view::NodeId;
use zgui_ui::prelude::*;

use crate::desktop::grab::Handles;
use crate::desktop::stage::Stage;

const BELOW: &str = "Bitwarden Import";

/// Opens a stack `width` CSS pixels wide with a page titled `title` pushed over [`BELOW`]. The
/// stack's short back label is `back_label`.
fn open(width: f32, title: &'static str, back_label: &'static str) -> Stage {
    let sheet = format!(".frame {{ width: {width}px; height: 600px; --zui-space-sm: 8px }}");
    let held: Rc<Cell<Option<NavPath<u8>>>> = Rc::default();
    let keep = Rc::clone(&held);
    let mut stage = Stage::open(&sheet, move || {
        let path = NavPath::new(0_u8);
        keep.set(Some(path));
        view! {
            column(class = "frame") {
                NavStack(path = path, back_label = back_label, page = move |route: u8| {
                    let title = if route == 0 { BELOW } else { title };
                    view! { NavPage(title = title) {"Content"} }
                })
            }
        }
    });
    held.get().expect("the stack was built").push(1);
    stage.hold(core::time::Duration::from_secs(1));
    stage
}

/// Whether `node` is drawn: no ancestor is inert, as the probe that measures the bar is.
fn drawn(handles: &Handles, node: NodeId) -> bool {
    let mut at = Some(node);
    while let Some(here) = at {
        if handles.dom.attribute_is(here, AttrName::new("inert"), "") {
            return false;
        }
        at = handles.dom.parent(here);
    }
    true
}

/// The drawn box of the smallest drawn node that says exactly `text`.
fn drawn_box(stage: &Stage, text: &str) -> Option<Rect<DevicePx, Device>> {
    stage
        .census()
        .nodes
        .into_iter()
        .filter(|node| node.text == text && node.area() > 0.0)
        .filter(|node| drawn(stage.handles(), node.id))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .and_then(|node| node.rect)
}

/// Whether a reader meets a back button named after the page below.
fn back_is_announced(stage: &Stage) -> bool {
    stage
        .announced()
        .iter()
        .any(|node| node.role == "Button" && node.name == BELOW)
}

fn right(rect: Rect<DevicePx, Device>) -> f32 {
    rect.origin.x.0 + rect.size.width.0
}

fn centre(rect: Rect<DevicePx, Device>) -> f32 {
    rect.origin.x.0 + rect.size.width.0 / 2.0
}

/// The space the sheet gives between neighbors, and at each end of the bar.
const GAP: f32 = 8.0;

#[test]
fn a_wide_bar_shows_the_full_back_title_beside_the_centred_title() {
    let stage = open(600.0, "Personal", "Back");
    let back = drawn_box(&stage, BELOW).expect("back shows the title of the page below");
    let title = drawn_box(&stage, "Personal").expect("the title shows");

    assert!(
        (centre(title) - 300.0).abs() <= 1.0,
        "the title is centred: {title:?}"
    );
    assert!(
        right(back) + GAP <= title.origin.x.0,
        "{back:?} clears {title:?}"
    );
    assert!(back_is_announced(&stage));
}

#[test]
fn a_phone_bar_shortens_the_back_title_and_keeps_the_title_centred() {
    let stage = open(320.0, "Personal", "Back");
    assert!(
        drawn_box(&stage, BELOW).is_none(),
        "the long back title is gone"
    );
    let back = drawn_box(&stage, "Back").expect("back says Back");
    let title = drawn_box(&stage, "Personal").expect("the title shows");

    assert!(
        (centre(title) - 160.0).abs() <= 1.0,
        "the title is centred: {title:?}"
    );
    assert!(
        right(back) + GAP <= title.origin.x.0,
        "{back:?} clears {title:?}"
    );
    assert!(back_is_announced(&stage));
}

#[test]
fn the_short_back_label_is_the_one_the_stack_is_given() {
    let stage = open(320.0, "Personal", "Retour");
    assert!(drawn_box(&stage, "Back").is_none());
    assert!(drawn_box(&stage, "Retour").is_some(), "back says Retour");
}

#[test]
fn a_narrow_bar_shows_only_the_chevron() {
    let title = "Personal Accounts";
    let stage = open(200.0, title, "Back");
    assert!(
        drawn_box(&stage, BELOW).is_none(),
        "the long back title is gone"
    );
    assert!(drawn_box(&stage, "Back").is_none(), "Back is gone");
    let shown = drawn_box(&stage, title).expect("the title shows");

    assert!(
        shown.origin.x.0 >= GAP + 44.0 + GAP,
        "the title clears the chevron: {shown:?}"
    );
    assert!(
        right(shown) <= 200.0 - GAP,
        "the title stays in the bar: {shown:?}"
    );
    assert!(back_is_announced(&stage));
}

#[test]
fn a_title_view_is_centred_and_takes_clicks() {
    let clicks: Rc<Cell<u32>> = Rc::default();
    let count = Rc::clone(&clicks);
    let held: Rc<Cell<Option<NavPath<u8>>>> = Rc::default();
    let keep = Rc::clone(&held);
    let mut stage = Stage::open(
        ".frame { width: 320px; height: 600px; --zui-space-sm: 8px }",
        move || {
            let path = NavPath::new(0_u8);
            keep.set(Some(path));
            let count = Rc::clone(&count);
            view! {
                column(class = "frame") {
                    NavStack(path = path, page = move |route: u8| {
                        let count = Rc::clone(&count);
                        let title = if route == 0 { BELOW } else { "Ada Lovelace" };
                        view! {
                            NavPage(title = title) {
                                NavTitle(slot) {
                                    {(route == 1).then(|| {
                                        let count = Rc::clone(&count);
                                        view! {
                                            control(on:click = move |_| count.set(count.get() + 1)) {
                                                text {"Ada"}
                                            }
                                        }
                                    })}
                                }
                                "Content"
                            }
                        }
                    })
                }
            }
        },
    );
    held.get().expect("the stack was built").push(1);
    stage.hold(core::time::Duration::from_secs(1));

    assert!(
        drawn_box(&stage, "Ada Lovelace").is_none(),
        "the view replaces the title text"
    );
    let shown = drawn_box(&stage, "Ada").expect("the title view shows");
    assert!(
        (centre(shown) - 160.0).abs() <= 1.0,
        "the title view is centred: {shown:?}"
    );
    assert!(
        drawn_box(&stage, "Back").is_some(),
        "back fits beside the title view"
    );

    let middle = Point::new(
        DevicePx(centre(shown)),
        DevicePx(shown.origin.y.0 + shown.size.height.0 / 2.0),
    );
    stage.click(middle);
    assert_eq!(clicks.get(), 1, "the title view takes the click");
}
