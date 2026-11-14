//! Content dragged from outside the window tells the elements it is over, and they can show it.
//!
//! A drag entering a zone fires `drag_enter` on it, every move fires `drag_over` on the innermost
//! element and bubbles, and leaving fires `drag_leave`. While the drag is over it, the zone matches
//! `:state(drag-over)`, which here makes it taller. A drag that leaves the window fires
//! `drag_leave` with no position. A drop ends the drag: the state clears with no `drag_leave`,
//! and the drop goes to the element under it, whatever has focus.

mod support;

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{DragEvent, SurfaceEvent};
use zgui_reactive::prelude::GetUntracked;
use zgui_view::event::events;
use zgui_view::{BuildCx, IntoView, View, ViewHost};
use zgui_vocab::{
    DragData, KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp,
};

const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .zone { display: block; height: 40px }
                   .zone:state(drag-over) { height: 80px }
                   .inner { display: block; height: 20px }
                   .target { display: block; height: 40px }";

fn at(x: f32, y: f32) -> Point<CssPx, Css> {
    Point::new(CssPx(x), CssPx(y))
}

#[test]
fn a_drag_is_announced_and_styled_until_it_leaves_or_drops() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = Rc::clone(&log);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let (enter, over, leave, dropped) = (
            Rc::clone(&seen),
            Rc::clone(&seen),
            Rc::clone(&seen),
            Rc::clone(&seen),
        );
        Box::new(
            zgui_elements::column()
                .child(
                    zgui_elements::r#box()
                        .class("zone")
                        .on(events::DRAG_ENTER, move |cx| {
                            enter
                                .borrow_mut()
                                .push(format!("enter {}", cx.data.paths.len()));
                        })
                        .on(events::DRAG_OVER, move |_| {
                            over.borrow_mut().push("over".into())
                        })
                        .on(events::DRAG_LEAVE, move |cx| {
                            let at = cx.position.map(|at| (at.x.0, at.y.0));
                            leave.borrow_mut().push(format!("leave {at:?}"));
                        })
                        .on(events::DROP, move |cx| {
                            dropped
                                .borrow_mut()
                                .push(format!("drop {}", cx.data.paths.len()));
                        })
                        .child(zgui_elements::r#box().class("inner")),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    let height = |app: &zgui_platform_headless::Harness<zgui_runtime::Runtime>| {
        app.app().windows()[0]
            .box_with_class("zone")
            .expect("the zone is laid out")
            .size
            .height
            .0
    };
    assert_eq!(height(&app), 40.0);

    let data = DragData::from_paths(vec![
        PathBuf::from("/tmp/a.wav"),
        PathBuf::from("/tmp/b.wav"),
    ]);
    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Entered {
        data: data.clone(),
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    assert_eq!(
        *log.borrow(),
        ["enter 2", "over"],
        "the inner box's drag_over bubbles"
    );
    assert_eq!(height(&app), 80.0, "the zone matches :state(drag-over)");

    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Moved {
        position: at(10.0, 200.0),
    }));
    app.settle(8);
    assert_eq!(
        log.borrow().last().map(String::as_str),
        Some("leave Some((10.0, 200.0))")
    );
    assert_eq!(height(&app), 40.0, "off the zone, the state clears");

    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Moved {
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Left));
    app.settle(8);
    assert_eq!(
        log.borrow().last().map(String::as_str),
        Some("leave None"),
        "a drag that left the window has no position"
    );
    assert_eq!(
        height(&app),
        40.0,
        "a drag that left the window clears the state"
    );

    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Entered {
        data: data.clone(),
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    log.borrow_mut().clear();
    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Dropped {
        data,
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    assert_eq!(*log.borrow(), ["drop 2"], "a drop is not also a leave");
    assert_eq!(height(&app), 40.0, "a drop ends the drag");
}

/// A box that logs each drag event it gets, under `name`.
fn target(
    name: &'static str,
    log: &Rc<RefCell<Vec<String>>>,
) -> zgui_elements::Element<zgui_elements::Box_> {
    let (enter, over, leave, dropped) = (
        Rc::clone(log),
        Rc::clone(log),
        Rc::clone(log),
        Rc::clone(log),
    );
    zgui_elements::r#box()
        .class("target")
        .on(events::DRAG_ENTER, move |cx| {
            let (paths, at) = (&cx.data.paths, cx.position);
            enter
                .borrow_mut()
                .push(format!("{name} enter {paths:?} {},{}", at.x.0, at.y.0));
        })
        .on(events::DRAG_OVER, move |cx| {
            let (paths, at) = (&cx.data.paths, cx.position);
            over.borrow_mut()
                .push(format!("{name} over {paths:?} {},{}", at.x.0, at.y.0));
        })
        .on(events::DRAG_LEAVE, move |cx| {
            let at = cx.position.map(|at| (at.x.0, at.y.0));
            leave.borrow_mut().push(format!("{name} leave {at:?}"));
        })
        .on(events::DROP, move |cx| {
            let (paths, at) = (&cx.data.paths, cx.position);
            dropped
                .borrow_mut()
                .push(format!("{name} drop {paths:?} {},{}", at.x.0, at.y.0));
        })
}

#[test]
fn each_element_under_the_drag_gets_the_paths_and_a_leave_gets_only_the_position() {
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let seen = Rc::clone(&log);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .child(target("a", &seen))
                .child(target("b", &seen))
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    let take = || core::mem::take(&mut *log.borrow_mut());
    let data = DragData::from_paths(vec![PathBuf::from("/tmp/a.wav")]);

    app.drag_enter(data.clone(), at(10.0, 10.0));
    app.settle(8);
    assert_eq!(
        take(),
        [
            r#"a enter ["/tmp/a.wav"] 10,10"#,
            r#"a over ["/tmp/a.wav"] 10,10"#
        ]
    );

    app.drag_move(at(10.0, 60.0));
    app.settle(8);
    assert_eq!(
        take(),
        [
            "a leave Some((10.0, 60.0))",
            r#"b enter ["/tmp/a.wav"] 10,60"#,
            r#"b over ["/tmp/a.wav"] 10,60"#,
        ],
        "a move carries the paths from the enter, and the leave comes first"
    );

    app.drag_leave();
    app.settle(8);
    assert_eq!(
        take(),
        ["b leave None"],
        "a drag off the window has no position"
    );

    app.drag_enter(data.clone(), at(10.0, 60.0));
    app.drag_drop(data, at(10.0, 60.0));
    app.settle(8);
    assert_eq!(
        take(),
        [
            r#"b enter ["/tmp/a.wav"] 10,60"#,
            r#"b over ["/tmp/a.wav"] 10,60"#,
            r#"b drop ["/tmp/a.wav"] 10,60"#,
        ],
        "the drop goes to the element under it, with no leave"
    );
}

#[test]
fn a_drop_goes_to_the_element_under_it_and_not_to_the_focused_one() {
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let seen = Rc::clone(&log);
    let mut app = support::app(CSS, move |cx: &mut BuildCx<'_>| {
        let (zone, target) = (Rc::clone(&seen), Rc::clone(&seen));
        Box::new(
            zgui_elements::column()
                .child(
                    zgui_elements::r#box()
                        .class("zone")
                        .on(events::DROP, move |_| zone.borrow_mut().push("zone")),
                )
                .child(
                    zgui_elements::control()
                        .class("target")
                        .on(events::DROP, move |_| target.borrow_mut().push("control")),
                )
                .into_view()
                .build(cx),
        )
    });
    app.settle(8);
    // Tab into the window: the control is the first focusable element in it.
    app.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    app.settle(8);
    assert!(
        app.app().windows()[0]
            .host()
            .focused()
            .get_untracked()
            .is_some(),
        "the fixture never focused the control"
    );

    app.deliver_to_first(SurfaceEvent::Drag(DragEvent::Dropped {
        data: DragData::from_paths(vec![PathBuf::from("/tmp/a.wav")]),
        position: at(10.0, 10.0),
    }));
    app.settle(8);
    assert_eq!(*log.borrow(), ["zone"]);
}
