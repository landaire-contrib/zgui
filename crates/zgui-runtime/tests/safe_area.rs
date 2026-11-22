//! That the safe area of the surface reaches `env(safe-area-inset-*)`, and follows each change.

mod support;

use zgui_geom::{Device, DevicePx, Edges, Point, Size};
use zgui_platform::{Surface, SurfaceEvent};
use zgui_platform_headless::{Harness, OffscreenSurface};
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, View};

/// A root that fills the surface and keeps its content clear of the system interface.
const CSS: &str = "root { display: block; width: 100%; height: 100%; \
                   padding: env(safe-area-inset-top) env(safe-area-inset-right) \
                   env(safe-area-inset-bottom) env(safe-area-inset-left) } \
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

/// Where the content box starts, in device pixels.
fn content_origin(harness: &Harness<Runtime>) -> Point<DevicePx, Device> {
    harness.app().windows()[0]
        .box_with_class("content")
        .expect("the content box is laid out")
        .origin
}

/// Changes the safe area of the surface and delivers the resize a platform sends with it.
fn rotate(
    harness: &mut Harness<Runtime>,
    size: Size<DevicePx, Device>,
    safe_area: Edges<DevicePx>,
) {
    let surface = surface(harness);
    surface.resize(size);
    surface.set_safe_area(safe_area);
    harness.deliver(surface.id(), SurfaceEvent::Resized(size));
    harness.settle(8);
}

#[test]
fn the_content_moves_clear_of_the_safe_area_and_follows_each_change() {
    let mut harness = app();
    harness.settle(8);
    assert_eq!(
        content_origin(&harness),
        Point::new(DevicePx(0.0), DevicePx(0.0))
    );

    let portrait = surface(&harness).size();
    rotate(
        &mut harness,
        portrait,
        Edges::new(DevicePx(59.0), DevicePx(0.0), DevicePx(34.0), DevicePx(0.0)),
    );
    assert_eq!(
        content_origin(&harness),
        Point::new(DevicePx(0.0), DevicePx(59.0)),
        "a safe area change with the same size reaches the style"
    );

    let landscape = Size::new(portrait.height, portrait.width);
    rotate(
        &mut harness,
        landscape,
        Edges::new(
            DevicePx(0.0),
            DevicePx(59.0),
            DevicePx(21.0),
            DevicePx(59.0),
        ),
    );
    assert_eq!(
        content_origin(&harness),
        Point::new(DevicePx(59.0), DevicePx(0.0))
    );
}

#[test]
fn the_safe_area_is_converted_at_the_scale_of_the_surface() {
    let mut harness = app();
    harness.settle(8);

    let surface = surface(&harness);
    let size = surface.size();
    surface.set_scale_factor(3.0);
    surface.set_safe_area(Edges::new(
        DevicePx(177.0),
        DevicePx(0.0),
        DevicePx(102.0),
        DevicePx(0.0),
    ));
    harness.deliver(
        surface.id(),
        SurfaceEvent::ScaleFactorChanged {
            scale_factor: 3.0,
            size,
        },
    );
    harness.settle(8);

    assert_eq!(content_origin(&harness).y, DevicePx(177.0));
}
