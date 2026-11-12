//! An adjustment a view asks for in the flush that changes the height of content above the port.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{DevicePx, Point};
use zgui_reactive::prelude::{Get, Update};
use zgui_reactive::{RenderEffect, RwSignal};
use zgui_view::{BuildCx, IntoView, NodeRef, ScrollBehavior, ScrollTarget, View};

/// A list of 20px rows and a 1px end marker in a 200px scrollport.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.list { display: block; width: 400px; height: 200px; overflow-y: auto }
.row { display: block; height: 20px }
.end { display: block; height: 1px }
";

/// The application under test.
type Harness = zgui_platform_headless::Harness<zgui_runtime::Runtime>;

/// The rows signal and the list, handed out of the build closure.
type Handed = Rc<RefCell<Option<(RwSignal<Vec<u32>, zgui_reactive::LocalStorage>, NodeRef)>>>;

/// How far the list is scrolled down.
fn scrolled(harness: &Harness) -> f32 {
    let window = harness.app().windows().first().expect("a window");
    let layout = window.layout().borrow();
    let element = layout
        .keys()
        .into_iter()
        .find(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
        .and_then(|key| layout.node(key).source)
        .expect("the list scrolls");
    window.scroll().borrow().offset_of(element).y.0
}

/// The application, its rows and its list.
struct Listing {
    /// The application.
    harness: Harness,
    /// The rows of the list.
    rows: RwSignal<Vec<u32>, zgui_reactive::LocalStorage>,
    /// The list.
    list: NodeRef,
}

/// Forty rows, with an effect that adjusts the list by the height of the rows removed from it, as
/// a virtualised list does for rows above its port, in the same flush that removes them.
fn listing() -> Listing {
    let slot: Handed = Rc::default();
    let handed = Rc::clone(&slot);
    let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
        let rows = RwSignal::new_local((0..40).collect::<Vec<u32>>());
        let list = NodeRef::new();
        *handed.borrow_mut() = Some((rows, list));

        let before = Rc::new(RefCell::new(None));
        core::mem::forget(RenderEffect::new(move |_| {
            let count = rows.get().len();
            let Some(was) = before.replace(Some(count)) else {
                return;
            };
            let removed = was as f32 - count as f32;
            if removed != 0.0 {
                list.scroll_to(
                    ScrollTarget::Adjust(Point::new(DevicePx(0.0), DevicePx(-20.0 * removed))),
                    ScrollBehavior::Instant,
                );
            }
        }));

        let column = zgui_elements::column()
            .class("list")
            .node_ref(list)
            .child(zgui_view::For::new(
                move || rows.get(),
                |n: &u32| *n,
                |_| zgui_view::AnyView::new(zgui_elements::column().class("row")),
            ))
            .child(zgui_elements::column().class("end"));
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(column)
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    let (rows, list) = slot.borrow().expect("the view was built");
    Listing {
        harness,
        rows,
        list,
    }
}

#[test]
fn an_adjustment_for_rows_removed_above_a_port_at_the_end_moves_it_once() {
    let Listing {
        mut harness,
        rows,
        list,
    } = listing();

    // At the end: forty rows and the marker, less the port.
    list.scroll_to(
        ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(601.0))),
        ScrollBehavior::Instant,
    );
    harness.settle(8);
    assert_eq!(scrolled(&harness), 601.0);

    rows.update(|rows| {
        rows.drain(..2);
    });
    harness.settle(8);
    assert_eq!(
        scrolled(&harness),
        561.0,
        "the port moved up by the two rows, which is also the new end"
    );
}

#[test]
fn an_adjustment_queued_after_a_scroll_of_its_list_applies_after_it() {
    let Listing {
        mut harness, list, ..
    } = listing();

    list.scroll_to(
        ScrollTarget::Offset(Point::new(DevicePx(0.0), DevicePx(300.0))),
        ScrollBehavior::Instant,
    );
    list.scroll_to(
        ScrollTarget::Adjust(Point::new(DevicePx(0.0), DevicePx(20.0))),
        ScrollBehavior::Instant,
    );
    harness.settle(8);
    assert_eq!(scrolled(&harness), 320.0);
}
