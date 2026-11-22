//! A touch pan goes to an element that listens for pans along its axis, or scrolls.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point, Size};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_view::{IntoView, View};
use zgui_vocab::{
    GesturePhase, Modifiers, PanEvent, PointerAction, PointerButton, PointerEvent, PointerId,
    PointerKind, Timestamp,
};

/// A vertical list of rows, and a horizontal strip of cells inside a card below it.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 120px; overflow-y: scroll }
.row { display: block; width: 400px; height: 40px }
.card { display: block; width: 400px; height: 100px }
.strip { display: flex; flex-direction: row; width: 400px; height: 100px; overflow-x: scroll }
.cell { display: block; flex: none; width: 300px; height: 100px }
";

/// What the fixture's listeners heard.
#[derive(Default)]
struct Heard {
    /// The horizontal pans the first row heard.
    row: RefCell<Vec<PanEvent>>,
    /// How many times the first row was clicked.
    clicks: Cell<u32>,
    /// How many scroll events the list sent.
    list_scrolls: Cell<u32>,
    /// The horizontal pans the card heard.
    card: RefCell<Vec<PanEvent>>,
    /// How many scroll events the strip sent.
    strip_scrolls: Cell<u32>,
}

/// The fixture. The list covers 0 to 120 and its first row 0 to 40; the card covers 120 to 220.
fn window() -> (Harness<Runtime>, Rc<Heard>) {
    let heard = Rc::new(Heard::default());
    let shared = Rc::clone(&heard);
    let harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        let row_heard = Rc::clone(&shared);
        let clicked = Rc::clone(&shared);
        let list_heard = Rc::clone(&shared);
        let card_heard = Rc::clone(&shared);
        let strip_heard = Rc::clone(&shared);
        let mut port = zgui_elements::column()
            .class("port")
            .on(zgui_view::events::SCROLL, move |_| {
                list_heard
                    .list_scrolls
                    .set(list_heard.list_scrolls.get() + 1);
            })
            .child(
                zgui_elements::control()
                    .class("row")
                    .on(zgui_view::events::HORIZONTAL_PAN, move |ev| {
                        row_heard.row.borrow_mut().push(**ev);
                    })
                    .on(zgui_view::events::CLICK, move |_| {
                        clicked.clicks.set(clicked.clicks.get() + 1);
                    }),
            );
        for _ in 0..20 {
            port = port.child(zgui_elements::column().class("row"));
        }
        let mut strip =
            zgui_elements::row()
                .class("strip")
                .on(zgui_view::events::SCROLL, move |_| {
                    strip_heard
                        .strip_scrolls
                        .set(strip_heard.strip_scrolls.get() + 1);
                });
        for _ in 0..3 {
            strip = strip.child(zgui_elements::column().class("cell"));
        }
        let card = zgui_elements::column()
            .class("card")
            .on(zgui_view::events::HORIZONTAL_PAN, move |ev| {
                card_heard.card.borrow_mut().push(**ev);
            })
            .child(strip);
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
                .child(card)
                .into_view()
                .build(cx),
        )
    });
    let mut harness = harness;
    harness.settle(8);
    (harness, heard)
}

fn at(x: f32, y: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(x), CssPx(y))
}

/// Delivers one touch event of the first finger.
fn touch(
    harness: &mut Harness<Runtime>,
    action: PointerAction,
    to: Point<CssPx, Css>,
    millis: u64,
) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action,
        event: PointerEvent {
            id: PointerId::new(1),
            kind: PointerKind::Touch,
            primary: true,
            position: to,
            button: Some(PointerButton::Primary),
            pressure: None,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::from_origin(std::time::Duration::from_millis(millis)),
    });
    // One frame per event, so that the moves are not merged into one.
    harness.pump();
}

/// Puts the first finger down at `from`, moves it through `steps` 16 ms apart, and lifts it.
fn drag(harness: &mut Harness<Runtime>, from: Point<CssPx, Css>, steps: &[Point<CssPx, Css>]) {
    touch(harness, PointerAction::Pressed, from, 0);
    let mut last = (from, 0);
    for (index, step) in steps.iter().enumerate() {
        let millis = 16 * (index as u64 + 1);
        touch(harness, PointerAction::Moved, *step, millis);
        last = (*step, millis);
    }
    touch(harness, PointerAction::Released, last.0, last.1);
    harness.pump();
}

fn phases(events: &[PanEvent]) -> Vec<GesturePhase> {
    events.iter().map(|event| event.phase).collect()
}

#[test]
fn a_sideways_pan_on_a_row_goes_to_the_row_and_leaves_the_list_still() {
    let (mut harness, heard) = window();

    drag(
        &mut harness,
        at(100.0, 20.0),
        &[at(130.0, 22.0), at(160.0, 24.0), at(190.0, 24.0)],
    );

    let row = heard.row.borrow();
    assert_eq!(
        phases(&row),
        [
            GesturePhase::Started,
            GesturePhase::Moved,
            GesturePhase::Moved,
            GesturePhase::Moved,
            GesturePhase::Ended,
        ],
        "the row heard the whole pan from the move that made it a pan"
    );
    assert!(row.iter().all(|event| event.origin == at(100.0, 20.0)));
    assert_eq!(
        row.last().map(PanEvent::translation),
        Some(Size::new(CssPx(90.0), CssPx(4.0)))
    );
    assert!(
        row.last().is_some_and(|event| event.velocity.x > 0.0),
        "the lift carried no speed"
    );
    assert_eq!(
        heard.list_scrolls.get(),
        0,
        "the list scrolled with the pan"
    );
    assert_eq!(heard.clicks.get(), 0, "the pan also clicked the row");
}

#[test]
fn a_vertical_pan_on_the_same_row_scrolls_the_list() {
    let (mut harness, heard) = window();

    drag(
        &mut harness,
        at(100.0, 100.0),
        &[at(102.0, 80.0), at(103.0, 60.0), at(103.0, 40.0)],
    );

    assert!(heard.list_scrolls.get() > 0, "the list did not scroll");
    assert!(
        heard.row.borrow().is_empty(),
        "a row that listens for horizontal pans heard a vertical one"
    );
    assert_eq!(heard.clicks.get(), 0, "the scroll also clicked a row");
}

#[test]
fn a_tap_on_the_row_still_clicks_it() {
    let (mut harness, heard) = window();

    drag(&mut harness, at(100.0, 20.0), &[at(102.0, 21.0)]);

    assert_eq!(heard.clicks.get(), 1);
    assert!(heard.row.borrow().is_empty(), "a tap is not a pan");
}

#[test]
fn a_strip_that_can_scroll_keeps_the_pan_and_one_at_its_end_gives_it_up() {
    let (mut harness, heard) = window();

    // Towards the end of the strip, which has room to scroll that way.
    drag(
        &mut harness,
        at(300.0, 170.0),
        &[at(270.0, 170.0), at(240.0, 170.0), at(210.0, 170.0)],
    );
    assert!(heard.strip_scrolls.get() > 0, "the strip did not scroll");
    assert!(
        heard.card.borrow().is_empty(),
        "the card took a pan the strip under it could scroll"
    );

    // Let the fling and any stretch come to rest, then go back to the start and pan past it.
    let frame = std::time::Duration::from_millis(20);
    harness.run_for(std::time::Duration::from_secs(4), frame);
    drag(
        &mut harness,
        at(100.0, 170.0),
        &[at(250.0, 170.0), at(400.0, 170.0)],
    );
    harness.run_for(std::time::Duration::from_secs(4), frame);
    heard.card.borrow_mut().clear();
    let scrolls = heard.strip_scrolls.get();

    drag(
        &mut harness,
        at(100.0, 170.0),
        &[at(130.0, 170.0), at(160.0, 170.0)],
    );
    assert_eq!(
        phases(&heard.card.borrow()),
        [
            GesturePhase::Started,
            GesturePhase::Moved,
            GesturePhase::Moved,
            GesturePhase::Ended,
        ],
        "at its start the strip cannot follow the finger, so the card gets the pan"
    );
    assert_eq!(
        heard.strip_scrolls.get(),
        scrolls,
        "the strip moved as well"
    );
}

#[test]
fn a_cancelled_contact_cancels_the_pan() {
    let (mut harness, heard) = window();

    touch(&mut harness, PointerAction::Pressed, at(100.0, 20.0), 0);
    touch(&mut harness, PointerAction::Moved, at(140.0, 20.0), 16);
    touch(&mut harness, PointerAction::Cancelled, at(140.0, 20.0), 32);
    harness.settle(8);

    assert_eq!(
        phases(&heard.row.borrow()),
        [
            GesturePhase::Started,
            GesturePhase::Moved,
            GesturePhase::Cancelled
        ]
    );
}
