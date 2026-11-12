//! A virtualised list whose rows differ in height, in a window with a real layout.
//!
//! The rows are measured from the layout the window gives them, and a row that grows above the port
//! moves the list by the same amount. Both are checked against the boxes the window laid out.

mod desktop;

use std::cell::Cell;
use std::rc::Rc;

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RwSignal};
use zgui::view;
use zgui::view::{ScrollBehavior, ScrollTarget};
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

/// Rows 20px tall, every fourth one 60px, and the open one 200px, in a port 400px tall.
const SHEET: &str = ".list { width: 300px; height: 400px }
                     .short { height: 20px }
                     .tall { height: 60px }
                     .open { height: 200px }";

/// The port and the open row, handed out of the view once it is built.
type Handed = Rc<Cell<Option<(NodeRef, RwSignal<Option<usize>, LocalStorage>)>>>;

/// A list of ten thousand rows, estimated at 20px, opened and settled.
fn open() -> (Stage, NodeRef, RwSignal<Option<usize>, LocalStorage>) {
    let handed: Handed = Rc::default();
    let out = Rc::clone(&handed);
    let mut stage = Stage::open(SHEET, move || {
        let port = NodeRef::new();
        let opened = RwSignal::new_local(None);
        out.set(Some((port, opened)));
        view! {
            VirtualList(
                class = "list",
                count = 10_000_usize,
                measured = MeasuredRows::new(20.0),
                overscan = 2_usize,
                node_ref = port,
                label = "Rows",
                row = move |index: usize| {
                    let size = if index.is_multiple_of(4) { "tall" } else { "short" };
                    view! {
                        box(class = size, class:open = move || opened.get() == Some(index)) {
                            {format!("row {index}")}
                        }
                    }
                }
            )
        }
    });
    stage.settle();
    let (port, opened) = handed.get().expect("the view was built");
    (stage, port, opened)
}

/// The top of the port, in window device pixels.
fn port_top(stage: &Stage, port: NodeRef) -> f32 {
    let node = port.get_untracked().expect("the port is bound");
    stage
        .handles()
        .host
        .border_box(node)
        .expect("the port is laid out")
        .origin
        .y
        .0
}

/// The first row whose box reaches below the top of the port, and where its box starts.
fn first_visible(stage: &Stage, port: NodeRef) -> (usize, f32) {
    let top = port_top(stage, port);
    let census = stage.census();
    (0..10_000)
        .filter_map(|index| {
            let seen = census.control(&format!("row {index}"))?;
            let rect = seen.rect?;
            (rect.origin.y.0 + rect.size.height.0 > top).then_some((index, rect.origin.y.0))
        })
        .next()
        .expect("a row is on screen")
}

#[test]
fn rows_are_built_for_the_heights_they_are_laid_out_at() {
    let (stage, port, _) = open();

    // Rows 0 to 12 start inside the 400px port, and two more are slack.
    let census = stage.census();
    assert!(
        census.control("row 14").is_some(),
        "the last row of slack is built"
    );
    assert!(
        census.control("row 15").is_none(),
        "a row the port cannot reach is not built"
    );

    let node = port.get_untracked().expect("the port is bound");
    let extent = stage
        .handles()
        .host
        .scroll_position(node)
        .content_size
        .height
        .0;
    let grown = extent - 10_000.0 * 20.0;
    assert!(
        grown >= 4.0 * 40.0 && grown % 40.0 == 0.0,
        "every measured 60px row adds 40px to the estimated extent: {extent}"
    );
}

#[test]
fn a_row_that_opens_above_the_port_leaves_the_rows_on_screen_where_they_are() {
    let (mut stage, port, opened) = open();
    port.scroll_to(
        ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(2_010.0))),
        ScrollBehavior::Instant,
    );
    stage.settle();
    let (row, before) = first_visible(&stage, port);
    assert!(row > 2, "the list scrolled");

    opened.set(Some(row - 1));
    stage.settle();

    assert_eq!(
        first_visible(&stage, port),
        (row, before),
        "row {row} moved when the row above it opened"
    );
    let census = stage.census();
    let open = census
        .control(&format!("row {}", row - 1))
        .and_then(|seen| seen.rect)
        .expect("the open row is built");
    assert_eq!(open.size.height.0, 200.0);
}
