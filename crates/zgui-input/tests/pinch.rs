//! Where a pinch is aimed.

mod support;

use support::{Element, Fixture, Session};
use zgui_geom::CssPx;
use zgui_vocab::{EventKind, GesturePhase, Payload, PinchEvent};

/// An image above a caption, each 100 pixels high.
fn captioned() -> Session {
    Session::new(Fixture::new(
        Element::new("root").children(vec![Element::new("image"), Element::new("caption")]),
        "root { display: block; width: 300px }
         image, caption { display: block; height: 100px }",
    ))
}

/// One stage of a pinch over the centre of `name`.
fn over(session: &Session, name: &str, phase: GesturePhase) -> PinchEvent {
    let point = session.fixture.centre_of(name);
    PinchEvent {
        scale_delta: 0.2,
        phase,
        position: zgui_geom::Point::new(CssPx(point.x.0), CssPx(point.y.0)),
    }
}

/// Where `event` is aimed.
fn target(session: &mut Session, event: &PinchEvent) -> Option<zgui_dom::NodeKey> {
    let filter = session.fixture.filter();
    let world = session.fixture.world(&filter);
    session.router.pinch(&world, event).target()
}

#[test]
fn a_pinch_is_aimed_at_what_is_under_the_pointer_and_does_nothing_on_its_own() {
    let mut session = captioned();
    let image = session.fixture.key("image");
    let event = over(&session, "image", GesturePhase::Moved);

    let filter = session.fixture.filter();
    let world = session.fixture.world(&filter);
    let routed = session.router.pinch(&world, &event);
    assert_eq!(routed.target(), Some(image));
    assert_eq!(routed.kind, EventKind::Pinch);
    assert_eq!(routed.payload, Payload::Pinch(event));
    assert!(routed.default.is_none());
}

#[test]
fn every_stage_of_a_pinch_goes_where_it_started() {
    let mut session = captioned();
    let image = session.fixture.key("image");
    let caption = session.fixture.key("caption");

    let started = over(&session, "image", GesturePhase::Started);
    assert_eq!(target(&mut session, &started), Some(image));
    let moved = over(&session, "caption", GesturePhase::Moved);
    assert_eq!(
        target(&mut session, &moved),
        Some(image),
        "the pointer left the image while the fingers were down"
    );
    let ended = over(&session, "caption", GesturePhase::Ended);
    assert_eq!(target(&mut session, &ended), Some(image));

    let next = over(&session, "caption", GesturePhase::Started);
    assert_eq!(
        target(&mut session, &next),
        Some(caption),
        "the next pinch starts where the pointer is"
    );
}
