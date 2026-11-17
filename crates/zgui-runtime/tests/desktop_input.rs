//! A handler reads the desktop's double-click time and distance, and the line height a wheel moves
//! its scroll container by, from the event it is handed.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{ClickSettings, SurfaceEvent};
use zgui_platform_headless::Headless;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{
    Modifiers, PointerAction, PointerEvent, ScrollDelta, ScrollPhase, Timestamp, WheelEvent,
};

/// One element covering the whole window, with a line height of 24 pixels.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   column { display: block; width: 400px; height: 300px; line-height: 24px }";

/// What the one element's handlers read.
#[derive(Default)]
struct Read {
    doubles: Cell<u32>,
    click: Cell<Option<ClickSettings>>,
    line: Cell<Option<CssPx>>,
}

/// A window over `platform` whose one element records what its handlers read.
fn window(
    platform: Headless,
) -> (
    zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    Rc<Read>,
) {
    let read = Rc::new(Read::default());
    let seen = Rc::clone(&read);
    let mut app = support::app_with_text_on(platform, CSS, move |cx: &mut BuildCx<'_>| {
        let (doubled, pressed, wheeled) = (Rc::clone(&seen), Rc::clone(&seen), Rc::clone(&seen));
        Box::new(
            zgui_elements::column()
                .class("root")
                .on(zgui_view::events::DOUBLE_CLICK, move |_| {
                    doubled.doubles.set(doubled.doubles.get() + 1);
                })
                .on(zgui_view::events::POINTER_DOWN, move |ev| {
                    pressed.click.set(Some(ev.click_settings()));
                })
                .on(zgui_view::events::WHEEL, move |ev| {
                    wheeled.line.set(ev.line_height());
                })
                .into_view()
                .build(cx),
        )
    });
    app.settle(4);
    (app, read)
}

/// A press and a release at 200, 150, `at` milliseconds after the start.
fn click(app: &mut zgui_platform_headless::Harness<zgui_runtime::Runtime>, at: u64) {
    let pointer = |action| SurfaceEvent::Pointer {
        action,
        event: PointerEvent::mouse(Point::new(CssPx(200.0), CssPx(150.0))),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN + Duration::from_millis(at),
    };
    app.deliver_to_first(pointer(PointerAction::Moved));
    app.deliver_to_first(pointer(PointerAction::Pressed));
    app.deliver_to_first(pointer(PointerAction::Released));
    app.settle(2);
}

#[test]
fn a_desktop_with_a_slower_double_click_counts_a_slower_second_click() {
    let slow = ClickSettings::desktop().with_interval(Duration::from_millis(900));
    let (mut app, read) = window(Headless::new().with_click_settings(slow));
    click(&mut app, 0);
    click(&mut app, 800);
    assert_eq!(read.doubles.get(), 1);
    assert_eq!(
        read.click.get(),
        Some(slow),
        "a handler reads the desktop's settings"
    );

    let (mut app, read) = window(Headless::new());
    click(&mut app, 0);
    click(&mut app, 800);
    assert_eq!(read.doubles.get(), 0, "the fallback interval is shorter");
}

#[test]
fn a_wheel_handler_reads_the_line_height_of_what_it_is_over() {
    let (mut app, read) = window(Headless::new());
    app.deliver_to_first(SurfaceEvent::Wheel {
        event: WheelEvent {
            id: zgui_vocab::PointerId::MOUSE,
            kind: zgui_vocab::PointerKind::Mouse,
            position: Point::<CssPx, Css>::new(CssPx(200.0), CssPx(150.0)),
            delta: ScrollDelta::Lines { x: 0.0, y: 1.0 },
            phase: ScrollPhase::Discrete,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    app.settle(4);
    assert_eq!(read.line.get(), Some(CssPx(24.0)));
}
