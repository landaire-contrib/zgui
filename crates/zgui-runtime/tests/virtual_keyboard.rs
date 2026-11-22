//! That the on-screen keyboard of the surface reaches `env(keyboard-inset-*)`, and follows each
//! change.

mod support;

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_platform_headless::{Harness, OffscreenSurface};
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, View};

/// A root that fills the surface, ends above the keyboard and puts its content at its end.
const CSS: &str = "root { display: flex; flex-direction: column; justify-content: flex-end; \
                   box-sizing: border-box; width: 100%; height: 100%; \
                   padding-bottom: env(keyboard-inset-height) } \
                   .content { height: 10px }";

/// An application with one content box inside the root.
fn app() -> Harness<Runtime> {
    support::app(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .child(zgui_elements::column().class("content"))
                .into_view()
                .build(cx),
        )
    })
}

/// The window's surface.
fn surface(harness: &Harness<Runtime>) -> std::sync::Arc<OffscreenSurface> {
    harness
        .platform()
        .offscreens()
        .first()
        .cloned()
        .expect("the application opened its window")
}

/// Where the content box ends, in device pixels.
fn content_bottom(harness: &Harness<Runtime>) -> DevicePx {
    harness.app().windows()[0]
        .box_with_class("content")
        .expect("the content box is laid out")
        .bottom()
}

/// Shows the keyboard over `area`, or hides it, and delivers the resize a platform sends with it.
fn keyboard(harness: &mut Harness<Runtime>, area: Option<Rect<DevicePx, Device>>) {
    let surface = surface(harness);
    surface.set_virtual_keyboard(area);
    harness.deliver(surface.id(), SurfaceEvent::Resized(surface.size()));
    harness.settle(8);
}

#[test]
fn the_content_ends_above_the_keyboard_and_returns_when_it_hides() {
    let mut harness = app();
    harness.settle(8);
    let size = surface(&harness).size();
    assert_eq!(content_bottom(&harness), size.height);

    let shown = Rect::new(
        Point::new(DevicePx(0.0), DevicePx(size.height.0 - 120.0)),
        Size::new(size.width, DevicePx(120.0)),
    );
    keyboard(&mut harness, Some(shown));
    assert_eq!(
        content_bottom(&harness),
        DevicePx(size.height.0 - 120.0),
        "a keyboard change with the same size reaches the style"
    );

    keyboard(&mut harness, None);
    assert_eq!(content_bottom(&harness), size.height);
}

#[test]
fn the_keyboard_area_is_converted_at_the_scale_of_the_surface() {
    let mut harness = app();
    harness.settle(8);

    let surface = surface(&harness);
    let size = surface.size();
    surface.set_scale_factor(2.0);
    surface.set_virtual_keyboard(Some(Rect::new(
        Point::new(DevicePx(0.0), DevicePx(size.height.0 - 100.0)),
        Size::new(size.width, DevicePx(100.0)),
    )));
    harness.deliver(
        surface.id(),
        SurfaceEvent::ScaleFactorChanged {
            scale_factor: 2.0,
            size,
        },
    );
    harness.settle(8);

    assert_eq!(content_bottom(&harness), DevicePx(size.height.0 - 100.0));
}
