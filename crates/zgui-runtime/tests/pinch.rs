//! A pinch reaches the element under the pointer, in the document's CSS pixels.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform_headless::Harness;
use zgui_runtime::{App, Runtime, Zoom};
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{GesturePhase, PinchEvent};

/// A document whose one control is 40 by 20 CSS pixels at 50, 30.
const CSS: &str = "root { display: block; width: 100%; height: 100% }
                   control { display: block; width: 40px; height: 20px;
                             margin: 30px 0 0 50px }";

/// A window at zoom 2 whose control records the pinches it hears.
fn window() -> (Harness<Runtime>, Rc<RefCell<Vec<PinchEvent>>>) {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let recorded = Rc::clone(&heard);
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(CSS)
        .with_zoom(Zoom::new(2.0).expect("the factor is in range"))
        .with_renderer(Box::new(support::capture))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let recorded = Rc::clone(&recorded);
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(
                        zgui_elements::control().on(zgui_view::events::PINCH, move |ev| {
                            recorded.borrow_mut().push(**ev);
                        }),
                    )
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(16);
    (harness, heard)
}

/// One stage of a pinch at `at`, in the platform's CSS pixels.
fn pinch(scale_delta: f32, phase: GesturePhase, at: Point<CssPx, Css>) -> PinchEvent {
    PinchEvent {
        scale_delta,
        phase,
        position: at,
    }
}

#[test]
fn a_pinch_reaches_the_element_under_the_pointer_at_a_zoom() {
    let (mut harness, heard) = window();

    // At zoom 2 the control covers 100 to 180 by 60 to 100 device pixels, and the platform scale
    // is 1.
    let over = Point::new(CssPx(140.0), CssPx(80.0));
    for stage in [
        pinch(0.0, GesturePhase::Started, over),
        pinch(0.25, GesturePhase::Moved, over),
        pinch(0.0, GesturePhase::Ended, over),
    ] {
        harness.pinch(stage);
    }
    harness.settle(16);

    let at = Point::new(CssPx(70.0), CssPx(40.0));
    assert_eq!(
        *heard.borrow(),
        [
            pinch(0.0, GesturePhase::Started, at),
            pinch(0.25, GesturePhase::Moved, at),
            pinch(0.0, GesturePhase::Ended, at),
        ],
        "every stage arrived in the document's CSS pixels with its scale delta unchanged"
    );

    // The control's own CSS coordinates, which are outside it on the device at zoom 2.
    harness.pinch(pinch(0.25, GesturePhase::Moved, at));
    harness.settle(16);
    assert_eq!(
        heard.borrow().len(),
        3,
        "a pinch beside the control missed it"
    );

    harness.shut_down();
}
