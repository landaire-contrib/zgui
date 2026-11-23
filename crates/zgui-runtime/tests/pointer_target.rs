//! Which element under the pointer a platform pointer takes its shape from.
//!
//! The target is a state the surface is told, so these tests read what the headless surface got.

mod support;

use std::time::Duration;

use zgui_geom::{CssPx, Device, DevicePx, Point, Rect, Size};
use zgui_platform::{PointerShape, PointerTarget, Surface, SurfaceEvent};
use zgui_platform_headless::Harness;
use zgui_runtime::Runtime;
use zgui_view::{A11yBinding, Anchor, BuildCx, IntoView, View};
use zgui_vocab::{Modifiers, PointerAction, PointerEvent, Role, Timestamp};

/// A little more than one frame at the surface's refresh rate.
const FRAME: Duration = Duration::from_millis(17);

/// Two buttons, a box that sets `cursor: pointer`, two fields, a grip and a disabled button, each
/// at a known place.
const CSS: &str = "root { display: block; width: 400px; height: 300px }
                   .first { display: block; position: absolute; left: 0; top: 0;
                            width: 100px; height: 40px; border-radius: 6px }
                   .second { display: block; position: absolute; left: 0; top: 50px;
                             width: 100px; height: 40px; border-radius: 9999px }
                   .label { display: block; width: 40px; height: 20px }
                   .card { display: block; position: absolute; left: 150px; top: 0;
                           width: 120px; height: 80px; border-radius: 12px; cursor: pointer }
                   .field { display: block; position: absolute; left: 0; top: 120px;
                            width: 200px; height: 40px; font-size: 16px; line-height: 20px }
                   .grip { display: block; position: absolute; left: 300px; top: 0;
                           width: 50px; height: 50px; cursor: col-resize }
                   .off { display: block; position: absolute; left: 300px; top: 100px;
                          width: 50px; height: 50px }
                   .combo { display: block; position: absolute; left: 0; top: 200px;
                            width: 200px; height: 40px; font-size: 16px; line-height: 24px }";

/// A window that holds every element of [`CSS`].
fn window() -> Harness<Runtime> {
    let mut harness = support::app_with_text(CSS, |cx: &mut BuildCx<'_>| {
        let view = zgui_elements::r#box()
            .child(
                zgui_elements::r#box()
                    .class("first")
                    .a11y(A11yBinding::new(Role::Button))
                    .child(zgui_elements::r#box().class("label")),
            )
            .child(
                zgui_elements::r#box()
                    .class("second")
                    .a11y(A11yBinding::new(Role::Button)),
            )
            .child(
                zgui_elements::r#box()
                    .class("card")
                    .child(zgui_elements::r#box().class("label")),
            )
            .child(zgui_elements::editor().class("field").child("hello there"))
            .child(
                zgui_elements::editor()
                    .class("combo")
                    .a11y(A11yBinding::new(Role::ComboBox))
                    .child("pick one"),
            )
            // A role that the highlight fits, under a cursor that keeps the pointer.
            .child(
                zgui_elements::r#box()
                    .class("grip")
                    .a11y(A11yBinding::new(Role::Button)),
            )
            .child(
                zgui_elements::r#box()
                    .class("off")
                    .a11y(A11yBinding::new(Role::Button).disabled(true)),
            )
            .into_view();
        Box::new(view.build(cx)) as Box<dyn Anchor>
    });
    harness.settle(8);
    harness
}

/// Moves the pointer to a point in the window and runs a frame.
fn point_at(harness: &mut Harness<Runtime>, x: f32, y: f32) {
    harness.deliver_to_first(SurfaceEvent::Pointer {
        event: PointerEvent::mouse(Point::new(CssPx(x), CssPx(y))),
        action: PointerAction::Moved,
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(4);
    harness.advance(FRAME);
    harness.settle(4);
}

/// The target the surface was last given.
fn target(harness: &Harness<Runtime>) -> Option<PointerTarget> {
    harness
        .platform()
        .offscreens()
        .first()?
        .last_pointer_target()
}

/// How many targets the surface was given.
fn told(harness: &Harness<Runtime>) -> usize {
    harness.platform().offscreens()[0]
        .pointer_target_log()
        .len()
}

/// The scale of the surface, which the target is measured in.
fn scale(harness: &Harness<Runtime>) -> f32 {
    harness.platform().offscreens()[0].scale_factor() as f32
}

/// A rectangle in CSS pixels, in the physical pixels of the surface.
fn on_device(scale: f32, x: f32, y: f32, width: f32, height: f32) -> Rect<DevicePx, Device> {
    Rect::new(
        Point::new(DevicePx(x * scale), DevicePx(y * scale)),
        Size::new(DevicePx(width * scale), DevicePx(height * scale)),
    )
}

#[test]
fn a_button_gives_a_highlight_with_its_corner_radius() {
    let mut harness = window();
    let scale = scale(&harness);

    // The label inside the button computes no cursor of its own, and the button gives the area.
    point_at(&mut harness, 10.0, 10.0);
    let button = target(&harness).expect("the pointer is over a button");
    assert_eq!(button.area, on_device(scale, 0.0, 0.0, 100.0, 40.0));
    assert_eq!(
        button.shape,
        PointerShape::Highlight {
            corner_radius: DevicePx(6.0 * scale),
        },
    );

    // A pill radius is cut to half the height, as it is drawn.
    point_at(&mut harness, 10.0, 60.0);
    assert_eq!(
        target(&harness).map(|target| target.shape),
        Some(PointerShape::Highlight {
            corner_radius: DevicePx(20.0 * scale),
        }),
    );
}

#[test]
fn cursor_pointer_gives_a_highlight_on_the_element_that_sets_it() {
    let mut harness = window();
    let scale = scale(&harness);

    point_at(&mut harness, 155.0, 5.0);
    let card = target(&harness).expect("the pointer is over a box that sets cursor: pointer");
    assert_eq!(card.area, on_device(scale, 150.0, 0.0, 120.0, 80.0));
    assert_eq!(
        card.shape,
        PointerShape::Highlight {
            corner_radius: DevicePx(12.0 * scale),
        },
    );

    // A point on the card outside its label gives the same element.
    point_at(&mut harness, 255.0, 70.0);
    assert_eq!(target(&harness).map(|target| target.id), Some(card.id));
}

#[test]
fn a_text_field_gives_a_beam_with_its_line_height() {
    let mut harness = window();
    let scale = scale(&harness);

    point_at(&mut harness, 20.0, 130.0);
    let field = target(&harness).expect("the pointer is over a field");
    assert_eq!(field.area, on_device(scale, 0.0, 120.0, 200.0, 40.0));
    assert_eq!(
        field.shape,
        PointerShape::Beam {
            line_height: DevicePx(20.0 * scale),
        },
    );

    // A field that is also a control, as the text field of a combobox is, takes a caret.
    point_at(&mut harness, 20.0, 210.0);
    assert_eq!(
        target(&harness).map(|target| target.shape),
        Some(PointerShape::Beam {
            line_height: DevicePx(24.0 * scale),
        }),
    );
}

#[test]
fn empty_space_another_cursor_and_a_disabled_button_give_no_target() {
    let mut harness = window();

    for (x, y, over) in [
        (380.0, 280.0, "empty space"),
        (310.0, 10.0, "a resize grip"),
        (310.0, 110.0, "a disabled button"),
    ] {
        // The control: from a button, the target changes to none.
        point_at(&mut harness, 10.0, 10.0);
        assert!(target(&harness).is_some(), "a button gives a target");

        point_at(&mut harness, x, y);
        assert_eq!(target(&harness), None, "the pointer is over {over}");
    }
}

#[test]
fn the_target_changes_as_the_pointer_moves_between_elements() {
    let mut harness = window();

    point_at(&mut harness, 10.0, 10.0);
    let first = target(&harness).expect("the pointer is over the first button");
    let after_first = told(&harness);

    // A move inside one element tells the surface nothing.
    for step in 0..5 {
        point_at(&mut harness, 12.0 + step as f32, 12.0);
    }
    assert_eq!(told(&harness), after_first);

    point_at(&mut harness, 10.0, 60.0);
    let second = target(&harness).expect("the pointer is over the second button");
    assert_ne!(second.id, first.id);
    assert_ne!(second.area, first.area);

    point_at(&mut harness, 10.0, 10.0);
    assert_eq!(target(&harness), Some(first));
}
