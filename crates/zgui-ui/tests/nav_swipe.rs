//! A navigation stack's swipe back, driven by touches through the real router.

mod desktop;

use core::cell::Cell;
use std::rc::Rc;

use zgui::geom::Css;
use zgui::geom::CssPx;
use zgui::geom::Point;
use zgui::prelude::*;
use zgui::view;
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

const SHEET: &str = ".frame { width: 400px; height: 800px }
                     .strip { flex-shrink: 0; height: 100px; overflow-x: auto }
                     .tile { flex-shrink: 0; width: 300px; height: 100px }
                     .list { flex: 1; min-height: 0; overflow-y: auto }
                     .row { flex-shrink: 0; height: 60px }";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Root,
    Group,
}

/// Opens a stack with a list page pushed on it, and gives back the stack's path.
fn open() -> (Stage, NavPath<Route>) {
    let held: Rc<Cell<Option<NavPath<Route>>>> = Rc::default();
    let keep = Rc::clone(&held);
    let mut stage = Stage::open(SHEET, move || {
        let path = NavPath::new(Route::Root);
        keep.set(Some(path));
        view! {
            column(class = "frame") {
                NavStack(path = path, page = |route: Route| match route {
                    Route::Root => AnyView::new(view! { NavPage(title = "Passwords") {"Root"} }),
                    Route::Group => AnyView::new(view! {
                        NavPage(title = "Bank") {
                            row(class = "strip") {
                                {(0..5)
                                    .map(|tile| view! { box(class = "tile") {{format!("Tile {tile}")}} })
                                    .collect::<Vec<_>>()}
                            }
                            column(class = "list") {
                                {(0..40)
                                    .map(|row| view! { control(class = "row") {{format!("Row {row}")}} })
                                    .collect::<Vec<_>>()}
                            }
                        }
                    }),
                })
            }
        }
    });
    let path = held.get().expect("the stack was built");
    path.push(Route::Group);
    stage.hold(core::time::Duration::from_secs(1));

    (stage, path)
}

fn css(x: f32, y: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(x), CssPx(y))
}

#[test]
fn a_vertical_drag_on_the_page_scrolls_and_does_not_go_back() {
    let (mut stage, path) = open();
    let before = stage
        .census()
        .control("Row 5")
        .and_then(|row| row.centre())
        .expect("row 5 is laid out");

    stage.pan(css(200.0, 600.0), css(205.0, 300.0), 10.0);
    stage.hold(core::time::Duration::from_millis(100));

    assert_eq!(path.count(), 2, "the drag did not go back");
    let after = stage
        .census()
        .control("Row 5")
        .and_then(|row| row.centre())
        .expect("row 5 is laid out");
    assert!(
        after.y.0 < before.y.0 - 100.0,
        "the list scrolled: {before:?} to {after:?}"
    );
}

#[test]
fn a_drag_from_the_leading_edge_goes_back_before_a_strip_under_it_scrolls() {
    let (mut stage, path) = open();
    // Scrolled away from its start, the strip can scroll toward the trailing edge.
    stage.pan(css(350.0, 90.0), css(100.0, 92.0), 10.0);
    stage.hold(core::time::Duration::from_secs(2));
    assert_eq!(path.count(), 2, "a drag toward the leading edge scrolls");

    stage.pan(css(4.0, 90.0), css(300.0, 95.0), 10.0);
    stage.hold(core::time::Duration::from_secs(1));

    assert_eq!(path.pages(), [Route::Root]);
    assert!(stage.shows("Root"));
}

#[test]
fn a_sideways_drag_on_a_strip_that_can_scroll_scrolls_it() {
    let (mut stage, path) = open();
    stage.pan(css(350.0, 90.0), css(100.0, 92.0), 10.0);
    stage.hold(core::time::Duration::from_secs(2));
    let before = stage
        .census()
        .control("Tile 1")
        .and_then(|tile| tile.centre())
        .expect("tile 1 is laid out");

    stage.pan(css(150.0, 90.0), css(300.0, 92.0), 10.0);
    stage.hold(core::time::Duration::from_millis(100));

    assert_eq!(path.count(), 2, "the drag did not go back");
    let after = stage
        .census()
        .control("Tile 1")
        .and_then(|tile| tile.centre())
        .expect("tile 1 is laid out");
    assert!(
        after.x.0 > before.x.0 + 50.0,
        "the strip scrolled: {before:?} to {after:?}"
    );
}

#[test]
fn a_sideways_drag_on_a_list_that_cannot_scroll_sideways_goes_back() {
    let (mut stage, path) = open();

    stage.pan(css(150.0, 400.0), css(380.0, 402.0), 10.0);
    stage.hold(core::time::Duration::from_secs(1));

    assert_eq!(path.pages(), [Route::Root]);
}
