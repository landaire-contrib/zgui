//! Segment breaks laid out by the real shaper.
//!
//! A value of `white-space` that preserves breaks makes each segment break force a line break, in
//! text and in an editor.

use std::sync::Arc;

use zgui::app::Fonts;
use zgui::platform::{Surface, SurfaceEvent};
use zgui::prelude::*;
use zgui::render::{RenderTarget, Renderer};
use zgui::runtime::{AppError, Runtime};
use zgui::view::{Anchor, BuildCx};
use zgui::vocab::{KeyCode, KeyEvent, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp};
use zgui_platform_headless::Harness;

/// The height of one line.
const LINE: f32 = 20.0;

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

/// A window with a text block and an editor that both start with `text`, under `white_space`.
fn page(white_space: &str, text: &'static str) -> Harness<Runtime> {
    let sheet = format!(
        "root {{ display: block; width: 400px; height: 300px }}
         .text, editor {{ display: block; width: 300px; line-height: {LINE}px;
                          white-space: {white_space} }}"
    );
    let fonts = fonts();
    let shaping = fonts.clone();
    let handler = zgui::runtime::App::new()
        .with_title("segment-breaks")
        .with_size(400.0, 300.0)
        .with_stylesheet(&sheet)
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
                zgui::elements::column()
                    .class("root")
                    .child(zgui::elements::column().class("text").child(text))
                    .child(zgui::elements::editor().class("notes").child(text))
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.settle(8);
    harness
}

/// How many lines the element with `class` holds.
fn lines(harness: &Harness<Runtime>, class: &str) -> f32 {
    harness.app().windows()[0]
        .box_with_class(class)
        .expect("the element is laid out")
        .height()
        .0
        / LINE
}

/// Presses a named key.
fn press(harness: &mut Harness<Runtime>, key: NamedKey, code: KeyCode) {
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent::named(key, PhysicalKey::Code(code)),
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
}

#[test]
fn each_spelling_of_a_segment_break_forces_a_line_break() {
    for white_space in ["pre", "pre-wrap", "pre-line", "break-spaces"] {
        for text in ["ab\ncd", "ab\r\ncd", "ab\rcd"] {
            let harness = page(white_space, text);
            assert_eq!(
                lines(&harness, "text"),
                2.0,
                "{text:?} in text, {white_space}"
            );
            assert_eq!(
                lines(&harness, "notes"),
                2.0,
                "{text:?} in an editor, {white_space}"
            );
        }
    }
}

#[test]
fn enter_in_a_pre_wrap_editor_starts_a_line() {
    let mut harness = page("pre-wrap", "abcd");
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    press(&mut harness, NamedKey::ArrowRight, KeyCode::ArrowRight);
    press(&mut harness, NamedKey::Enter, KeyCode::Enter);
    assert_eq!(lines(&harness, "notes"), 2.0);
    press(&mut harness, NamedKey::End, KeyCode::End);
    press(&mut harness, NamedKey::Enter, KeyCode::Enter);
    assert_eq!(
        lines(&harness, "notes"),
        3.0,
        "a break at the end adds an empty line"
    );
}
