//! That the focused field scrolls above the on-screen keyboard, through the whole loop.

mod support;

use core::time::Duration;

use zgui_geom::{Device, DevicePx, Point, Rect, Size};
use zgui_platform::{CubicBezier, KeyboardEasing, KeyboardMotion, Surface, SurfaceEvent};
use zgui_platform_headless::{Harness, OffscreenSurface};
use zgui_runtime::Runtime;
use zgui_view::{BuildCx, IntoView, View};
use zgui_vocab::{
    Key, KeyCode, KeyEvent, KeyLocation, KeyState, Modifiers, NamedKey, PhysicalKey, Timestamp,
};

/// A page that scrolls, with a field at its top, a field lower down and an editor after it.
///
/// The window is 300 pixels tall. The lower field is at 224 to 248. A line of the editor holds 25
/// characters.
const CSS: &str = "root { display: block; width: 400px; height: 300px; overflow: auto }
                   field, editor { display: block; width: 200px; line-height: 24px }
                   editor { white-space: pre-wrap }
                   field { height: 24px }
                   .gap { display: block; height: 200px }
                   .tail { display: block; height: 400px }";

/// The height of the keyboard in the tests.
const KEYBOARD: f32 = 120.0;

/// The space the window keeps between the caret and the keyboard.
const MARGIN: f32 = 16.0;

/// The page.
fn app() -> Harness<Runtime> {
    let mut harness = support::app_with_text(CSS, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(zgui_elements::field().class("top").child("top"))
                .child(zgui_elements::column().class("gap"))
                .child(zgui_elements::field().class("low").child("low"))
                .child(zgui_elements::column().class("gap"))
                .child(
                    zgui_elements::editor()
                        .class("notes")
                        .child("alpha beta gamma delta"),
                )
                .child(zgui_elements::column().class("tail"))
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    harness
}

/// How far a transform holds the panel of [`sheet`] below where it stops.
const RISING: f32 = 150.0;

/// A bottom sheet on its way up: a fixed panel, from 20 pixels to the bottom of the window, that a
/// transform still holds lower down. Its content scrolls, with a field at 150 to 174 in it.
fn sheet() -> Harness<Runtime> {
    let css = format!(
        "root {{ display: block; width: 400px; height: 300px }}
         .sheet {{ position: fixed; left: 0; right: 0; top: 20px; bottom: 0;
                   translate: 0px {RISING}px; overflow: hidden }}
         .content {{ display: block; height: 100%; overflow: auto }}
         field {{ display: block; width: 200px; height: 24px; line-height: 24px }}
         .gap {{ display: block; height: 150px }}
         .tail {{ display: block; height: 400px }}"
    );
    let mut harness = support::app_with_text(&css, |cx: &mut BuildCx<'_>| {
        Box::new(
            zgui_elements::column()
                .class("root")
                .child(
                    zgui_elements::column().class("sheet").child(
                        zgui_elements::column()
                            .class("content")
                            .child(zgui_elements::column().class("gap"))
                            .child(zgui_elements::field().class("inner").child("inner"))
                            .child(zgui_elements::column().class("tail")),
                    ),
                )
                .into_view()
                .build(cx),
        )
    });
    harness.settle(8);
    harness
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

/// Shows the keyboard over the bottom of the surface, and delivers the resize a platform sends.
fn show_keyboard(harness: &mut Harness<Runtime>) -> DevicePx {
    let surface = surface(harness);
    let size = surface.size();
    let top = DevicePx(size.height.0 - KEYBOARD);
    surface.set_virtual_keyboard(Some(Rect::new(
        Point::new(DevicePx(0.0), top),
        Size::new(size.width, DevicePx(KEYBOARD)),
    )));
    harness.deliver(surface.id(), SurfaceEvent::Resized(size));
    harness.settle(8);
    top
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

/// Types one character.
fn type_letter(harness: &mut Harness<Runtime>, text: &str) {
    harness.deliver_to_first(SurfaceEvent::Key {
        state: KeyState::Pressed,
        event: KeyEvent {
            key: Key::Character(text.into()),
            key_without_modifiers: Key::Character(text.into()),
            physical: PhysicalKey::Code(KeyCode::KeyA),
            location: KeyLocation::Standard,
            repeat: false,
        },
        modifiers: Modifiers::NONE,
        timestamp: Timestamp::ORIGIN,
    });
    harness.settle(8);
}

/// Lets `total` pass, one frame interval at a time.
fn wait(harness: &mut Harness<Runtime>, total: Duration) {
    harness.run_for(total, Duration::from_millis(16));
}

/// Where the element with `class` is, in device pixels.
fn box_of(harness: &Harness<Runtime>, class: &str) -> Rect<DevicePx, Device> {
    harness.app().windows()[0]
        .box_with_class(class)
        .expect("the element is laid out")
}

#[test]
fn a_field_under_the_keyboard_scrolls_above_it_when_it_gets_focus() {
    let mut harness = app();
    let keyboard_top = show_keyboard(&mut harness);
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    wait(&mut harness, Duration::from_millis(400));

    let low = box_of(&harness, "low");
    assert_eq!(
        low.bottom().0 + MARGIN,
        keyboard_top.0,
        "the field and the margin under it end at the keyboard"
    );
}

#[test]
fn a_field_above_the_keyboard_does_not_scroll() {
    let mut harness = app();
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    let before = box_of(&harness, "top");
    show_keyboard(&mut harness);
    wait(&mut harness, Duration::from_millis(400));

    assert_eq!(box_of(&harness, "top"), before);
}

#[test]
fn the_scroll_moves_with_the_keyboard() {
    let mut harness = app();
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    let before = box_of(&harness, "low").top().0;
    surface(&harness).set_virtual_keyboard_motion(Some(KeyboardMotion {
        duration: Duration::from_millis(500),
        easing: KeyboardEasing::CubicBezier(CubicBezier {
            x1: 0.0,
            y1: 0.0,
            x2: 1.0,
            y2: 1.0,
        }),
    }));
    let keyboard_top = show_keyboard(&mut harness);
    let end = keyboard_top.0 - MARGIN - 24.0;

    wait(&mut harness, Duration::from_millis(240));
    let halfway = box_of(&harness, "low").top().0;
    assert!(
        halfway < before && halfway > end,
        "half of the keyboard's movement moved the field part of the way: {before} {halfway} {end}"
    );

    wait(&mut harness, Duration::from_millis(300));
    assert_eq!(box_of(&harness, "low").top().0, end);
}

#[test]
fn a_caret_that_wraps_to_a_new_line_under_the_keyboard_scrolls_above_it() {
    let mut harness = app();
    let keyboard_top = show_keyboard(&mut harness);
    for _ in 0..3 {
        press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    }
    press(&mut harness, NamedKey::End, KeyCode::End);
    wait(&mut harness, Duration::from_millis(400));
    let focused = box_of(&harness, "notes");
    assert_eq!(focused.bottom().0 + MARGIN, keyboard_top.0);

    // Typing on the same line moves nothing.
    type_letter(&mut harness, " ");
    wait(&mut harness, Duration::from_millis(400));
    assert_eq!(box_of(&harness, "notes"), focused);

    for letter in ["e", "p", "s", "i", "l", "o", "n"] {
        type_letter(&mut harness, letter);
    }
    wait(&mut harness, Duration::from_millis(400));
    let grown = box_of(&harness, "notes");
    assert_eq!(
        grown.height().0,
        focused.height().0 + 24.0,
        "the last word wrapped to a second line"
    );
    assert_eq!(
        grown.bottom().0 + MARGIN,
        keyboard_top.0,
        "the caret on the second line and the margin under it end at the keyboard"
    );
}

#[test]
fn the_content_of_a_rising_sheet_scrolls_by_what_the_keyboard_covers_where_the_sheet_stops() {
    let mut harness = sheet();
    let keyboard_top = show_keyboard(&mut harness);
    press(&mut harness, NamedKey::Tab, KeyCode::Tab);
    wait(&mut harness, Duration::from_millis(400));

    let inner = box_of(&harness, "inner");
    assert_eq!(
        inner.bottom().0 - RISING + MARGIN,
        keyboard_top.0,
        "without the transform, the field and the margin under it end at the keyboard"
    );
}
