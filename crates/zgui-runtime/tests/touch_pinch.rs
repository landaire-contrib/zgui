//! Two fingers on a touch screen pinch an element that listens for pinches, or pan as before.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::SurfaceEvent;
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_view::{IntoView, View};
use zgui_vocab::{
    GesturePhase, Modifiers, PinchEvent, PointerAction, PointerButton, PointerEvent, PointerId,
    PointerKind, Timestamp,
};

/// A list that scrolls, holding a picture that listens for pinches above plain rows.
const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 300px; overflow-y: scroll }
.picture { display: block; width: 400px; height: 200px }
.row { display: block; width: 400px; height: 100px }
";

/// What the fixture's listeners heard.
#[derive(Default)]
struct Heard {
    /// The pinches the picture heard.
    pinches: RefCell<Vec<PinchEvent>>,
    /// How many scroll events the list sent.
    scrolls: Cell<u32>,
}

/// The fixture. The picture covers 0 to 200, and the rows below it run past the port.
fn window() -> (Harness<Runtime>, Rc<Heard>) {
    let heard = Rc::new(Heard::default());
    let shared = Rc::clone(&heard);
    let harness = support::app_with_text(CSS, move |cx: &mut zgui_view::BuildCx<'_>| {
        let pinched = Rc::clone(&shared);
        let scrolled = Rc::clone(&shared);
        let mut port = zgui_elements::column()
            .class("port")
            .on(zgui_view::events::SCROLL, move |_| {
                scrolled.scrolls.set(scrolled.scrolls.get() + 1);
            })
            .child(zgui_elements::column().class("picture").on(
                zgui_view::events::PINCH,
                move |ev| {
                    pinched.pinches.borrow_mut().push(**ev);
                },
            ));
        for _ in 0..10 {
            port = port.child(zgui_elements::column().class("row"));
        }
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(port)
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

/// Delivers one touch event of `finger`, in a frame of its own.
fn touch(
    harness: &mut Harness<Runtime>,
    finger: u64,
    action: PointerAction,
    to: Point<CssPx, Css>,
    millis: u64,
) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        action,
        event: PointerEvent {
            id: PointerId::new(finger),
            kind: PointerKind::Touch,
            primary: finger == 1,
            position: to,
            button: Some(PointerButton::Primary),
            pressure: None,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::from_origin(std::time::Duration::from_millis(millis)),
    });
    harness.pump();
}

/// Two fingers go down `spread` apart around `centre`, move upwards while they spread to twice
/// that, and lift.
fn pinch_out(harness: &mut Harness<Runtime>, centre: Point<CssPx, Css>, spread: f32) {
    let half = spread / 2.0;
    touch(
        harness,
        1,
        PointerAction::Pressed,
        at(centre.x.0 - half, centre.y.0),
        0,
    );
    touch(
        harness,
        2,
        PointerAction::Pressed,
        at(centre.x.0 + half, centre.y.0),
        8,
    );
    for step in 1..=4_u8 {
        let grown = half * (1.0 + f32::from(step) / 4.0);
        let up = centre.y.0 - f32::from(step) * 10.0;
        let millis = 8 + 16 * u64::from(step);
        touch(
            harness,
            1,
            PointerAction::Moved,
            at(centre.x.0 - grown, up),
            millis,
        );
        touch(
            harness,
            2,
            PointerAction::Moved,
            at(centre.x.0 + grown, up),
            millis + 1,
        );
    }
    touch(
        harness,
        1,
        PointerAction::Released,
        at(centre.x.0 - spread, centre.y.0 - 40.0),
        90,
    );
    touch(
        harness,
        2,
        PointerAction::Released,
        at(centre.x.0 + spread, centre.y.0 - 40.0),
        91,
    );
    harness.pump();
}

#[test]
fn a_pinch_over_a_listener_reaches_it_and_leaves_the_list_still() {
    let (mut harness, heard) = window();

    pinch_out(&mut harness, at(200.0, 100.0), 100.0);

    let pinches = heard.pinches.borrow();
    let phases: Vec<GesturePhase> = pinches.iter().map(|event| event.phase).collect();
    assert_eq!(phases.first(), Some(&GesturePhase::Started));
    assert_eq!(phases.last(), Some(&GesturePhase::Ended));
    assert!(
        phases[1..phases.len() - 1]
            .iter()
            .all(|phase| *phase == GesturePhase::Moved)
    );
    assert_eq!(
        pinches[0].position,
        at(200.0, 100.0),
        "it starts between the fingers"
    );
    let scale: f32 = pinches.iter().map(PinchEvent::factor).product();
    assert!(
        (scale - 2.0).abs() < 0.001,
        "the fingers spread to twice as far apart: {scale}"
    );
    assert_eq!(heard.scrolls.get(), 0, "the fingers also scrolled the list");

    harness.shut_down();
}

#[test]
fn a_pinch_that_no_element_hears_leaves_the_fingers_scrolling() {
    let (mut harness, heard) = window();

    pinch_out(&mut harness, at(200.0, 250.0), 100.0);

    assert!(heard.pinches.borrow().is_empty());
    assert!(
        heard.scrolls.get() > 0,
        "the fingers did not scroll the list"
    );

    harness.shut_down();
}

#[test]
fn a_pinch_that_takes_a_scrolling_finger_stops_the_scroll() {
    let (mut harness, heard) = window();
    touch(&mut harness, 1, PointerAction::Pressed, at(150.0, 150.0), 0);
    for step in 1..=3_u8 {
        let up = 150.0 - 15.0 * f32::from(step);
        touch(
            &mut harness,
            1,
            PointerAction::Moved,
            at(150.0, up),
            16 * u64::from(step),
        );
    }
    assert!(heard.scrolls.get() > 0, "one finger scrolls the list");

    // The point between the fingers is over the picture.
    touch(
        &mut harness,
        2,
        PointerAction::Pressed,
        at(250.0, 105.0),
        64,
    );
    harness.settle(8);
    let scrolled = heard.scrolls.get();
    for step in 1..=3_u8 {
        let by = 10.0 * f32::from(step);
        let millis = 64 + 16 * u64::from(step);
        touch(
            &mut harness,
            1,
            PointerAction::Moved,
            at(150.0 - by, 105.0 - by),
            millis,
        );
        touch(
            &mut harness,
            2,
            PointerAction::Moved,
            at(250.0 + by, 105.0 - by),
            millis + 1,
        );
    }
    touch(
        &mut harness,
        1,
        PointerAction::Released,
        at(120.0, 75.0),
        120,
    );
    touch(
        &mut harness,
        2,
        PointerAction::Released,
        at(280.0, 75.0),
        121,
    );
    harness.settle(8);

    assert_eq!(
        heard.pinches.borrow().first().map(|event| event.phase),
        Some(GesturePhase::Started)
    );
    assert_eq!(
        heard.scrolls.get(),
        scrolled,
        "the list scrolled while the picture had the pinch"
    );

    harness.shut_down();
}
