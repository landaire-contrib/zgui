//! That the platform's text size setting, motion preference and lifecycle reach an application.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::DevicePx;
use zgui_platform::Lifecycle;
use zgui_platform::MotionPreference;
use zgui_platform::SurfaceEvent;
use zgui_platform::TextScale;
use zgui_platform_headless::Harness;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::Get;
use zgui_reactive::prelude::Set;
use zgui_runtime::App;
use zgui_runtime::Runtime;
use zgui_view::Anchor;
use zgui_view::BuildCx;
use zgui_view::ClassName;
use zgui_view::IntoView;
use zgui_view::View;

/// A root with one content box, whose height the cases style.
fn content(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(
        zgui_elements::column()
            .child(zgui_elements::column().class("content"))
            .into_view()
            .build(cx),
    )
}

/// The height of the content box, in device pixels.
fn content_height(harness: &Harness<Runtime>) -> DevicePx {
    harness.app().windows()[0]
        .box_with_class("content")
        .expect("the content box is laid out")
        .height()
}

#[test]
fn the_text_scale_reaches_env_and_follows_a_change() {
    let mut harness = support::app(
        "root { display: block } .content { display: block; \
         height: calc(10px * env(preferred-text-scale)) }",
        content,
    );
    harness.settle(8);
    assert_eq!(content_height(&harness), DevicePx(10.0));

    let larger = TextScale::new(1.5).expect("a positive scale");
    harness.platform().set_text_scale(larger);
    harness.settle(8);
    assert_eq!(content_height(&harness), DevicePx(15.0));
}

#[test]
fn prefers_reduced_motion_follows_the_platform() {
    let mut harness = support::app(
        "root { display: block } .content { display: block; height: 10px } \
         @media (prefers-reduced-motion: reduce) { .content { height: 20px } }",
        content,
    );
    harness.settle(8);
    assert_eq!(content_height(&harness), DevicePx(10.0));

    harness
        .platform()
        .set_motion_preference(MotionPreference::Reduce);
    harness.settle(8);
    assert_eq!(content_height(&harness), DevicePx(20.0));

    harness
        .platform()
        .set_motion_preference(MotionPreference::NoPreference);
    harness.settle(8);
    assert_eq!(content_height(&harness), DevicePx(10.0));
}

#[test]
fn each_lifecycle_move_reaches_the_lifecycle_hook_in_order() {
    let heard: Rc<RefCell<Vec<Lifecycle>>> = Rc::default();
    let record = Rc::clone(&heard);
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet("root { display: block }")
        .with_renderer(Box::new(support::capture))
        .with_context(move || {
            let guard = zgui_runtime::desktop::on_lifecycle(move |event| {
                record.borrow_mut().push(event);
            });
            core::mem::forget(guard);
        })
        .into_handler(content)
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(4);

    let moves = [
        Lifecycle::WillResignActive,
        Lifecycle::DidEnterBackground,
        Lifecycle::WillEnterForeground,
        Lifecycle::DidBecomeActive,
    ];
    for event in moves {
        harness.platform().send_lifecycle(event);
        harness.settle(4);
    }

    assert_eq!(*heard.borrow(), moves);
    assert_eq!(
        harness.app().windows().len(),
        1,
        "the window stays through every move"
    );
}

#[test]
fn what_the_application_changes_on_will_resign_active_is_drawn_in_the_same_turn() {
    let covered = RwSignal::new_local(false);
    let handler = App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(
            "root { display: block } .content { display: block; height: 10px } \
             .content.covered { height: 20px }",
        )
        .with_renderer(Box::new(support::capture))
        .with_context(move || {
            let guard = zgui_runtime::desktop::on_lifecycle(move |event| {
                if event == Lifecycle::WillResignActive {
                    covered.set(true);
                }
            });
            core::mem::forget(guard);
        })
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                zgui_elements::column()
                    .child(
                        zgui_elements::column()
                            .class("content")
                            .class_toggle(ClassName::new("covered"), move || covered.get()),
                    )
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(4);
    assert_eq!(content_height(&harness), DevicePx(10.0));

    harness
        .platform()
        .send_lifecycle(Lifecycle::WillResignActive);
    // Delivers the queued move and runs no redraw the platform was asked for.
    harness.deliver_to_first(SurfaceEvent::Focused(false));
    assert_eq!(content_height(&harness), DevicePx(20.0));
}
