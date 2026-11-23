//! The white space at the end of a wrapped line, laid out by the real shaper.
//!
//! A space that ends a wrapped line hangs past the line's end edge. It takes no room, so a page
//! whose text wraps inside its scrollport has nothing to scroll sideways.

use std::sync::Arc;

use zgui::app::Fonts;
use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui_platform_headless::Harness;

/// The paragraph. Its lines wrap at spaces.
const TEXT: &str = "Paired devices send each other database changes whenever both are online, over \
                    an encrypted connection. A device still needs the database's master key to read \
                    them. When no direct connection is possible, the connection goes through the \
                    public relay servers.";

/// The shipped Latin face, which every generic family resolves to.
fn fonts() -> Fonts {
    let fonts = Fonts::shipped_only();
    let path = format!(
        "{}/../zgui-text-parley/tests/fonts/NotoSans-Regular.ttf",
        env!("CARGO_MANIFEST_DIR")
    );
    let data = std::fs::read(&path).unwrap_or_else(|error| panic!("reading {path}: {error}"));
    fonts
        .register(Arc::new(data), Some("Test Face"))
        .expect("the shipped face registers");
    fonts
}

/// A window holding a vertical scroll container 402 px wide, with 16 px of padding, a 15 px gutter,
/// taller content and the paragraph at 12 px.
fn page() -> Harness<Runtime> {
    const SHEET: &str = zgui::css!(
        ".page { display: block; width: 402px; height: 300px; padding: 16px; overflow-y: auto }
         .tall { display: block; height: 600px }
         .para { display: block; font-size: 12px }"
    );

    let fonts = fonts();
    let shaping = fonts.clone();
    let handler = zgui::runtime::App::new()
        .with_title("hanging-space")
        .with_size(600.0, 400.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(
            |_surface: &Arc<dyn Surface>, target: RenderTarget| {
                let mut renderer = zgui_testkit_scene::CaptureRenderer::new();
                Renderer::configure(&mut renderer, target);
                Ok::<_, AppError>(Box::new(renderer) as Box<dyn Renderer>)
            },
        ))
        .with_metrics(Box::new(move || fonts.metrics()))
        .with_text_engine(Box::new(move || {
            Box::new(zgui_layout::Paragraphs::new(shaping.shaper()))
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| -> Box<dyn Anchor> {
            Box::new(
                view! {
                    box(class = "page") {
                        box(class = "tall") {}
                        text(class = "para") { {TEXT} }
                    }
                }
                .into_view()
                .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.deliver_to_first(SurfaceEvent::Resized(zgui::geom::Size::new(
        zgui::geom::DevicePx(600.0),
        zgui::geom::DevicePx(400.0),
    )));
    harness.settle(32);
    harness
}

/// What the page scrolls, read off its scroll container and its paragraph.
struct Reading {
    /// The width of the scrollport.
    port: f32,
    /// How far the content scrolls sideways.
    limit: f32,
    /// How far the widest line reaches, with the white space at its end.
    reach: f32,
}

/// Reads the page's one scroll container and one paragraph.
fn read(harness: &Harness<Runtime>) -> Reading {
    let window = &harness.app().windows()[0];
    let layout = window.layout().borrow();
    let mut region = None;
    let mut reach = 0.0_f32;
    for key in layout.keys() {
        if let Some(found) = zgui_layout::scroll_region::region_of(&layout, key) {
            region = Some(found);
        }
        if let Some(resolution) = layout.inline_resolution(key) {
            for line in &resolution.lines {
                reach = reach.max(line.offset + line.width);
            }
        }
    }
    let region = region.expect("the page scrolls");

    Reading {
        port: region.scrollport.size.width.0,
        limit: region.limit().x.0,
        reach,
    }
}

#[test]
fn the_space_that_hangs_at_the_end_of_a_wrapped_line_does_not_scroll_its_container() {
    let reading = read(&page());

    assert_eq!(reading.port, 355.0, "the page reserves its gutter");
    assert!(
        reading.reach > reading.port,
        "a line's last space reaches past the scrollport: {} of {}",
        reading.reach,
        reading.port
    );
    assert_eq!(
        reading.limit, 0.0,
        "the space hangs, and nothing else reaches past the scrollport"
    );
}
