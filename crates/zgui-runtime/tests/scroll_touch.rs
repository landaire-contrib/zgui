//! How a finger on a touch screen moves a document: as UIScrollView does.
//!
//! Each case scripts touch contacts at the platform seam and reads the offsets and displacements
//! the scroller composes against the model: a fling decays at `0.998` per millisecond, a pull past
//! an end follows the rubber band `(1 - 1/(x*c/d + 1)) * d` with `c = 0.55`, and an edge returns on
//! a critically damped spring of stiffness 100.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::Runtime;
use zgui_scroll::Behavior;
use zgui_vocab::{PointerAction, PointerId, Timestamp};

/// One frame, rounded up past the deadline the park installs.
const FRAME: Duration = Duration::from_millis(20);

/// The time between two moves of a finger.
const BETWEEN: Duration = Duration::from_millis(10);

/// How much of a fling's speed survives one frame.
fn frame_decay() -> f32 {
    0.998_f32.powf(FRAME.as_secs_f32() * 1000.0)
}

/// How fast a fling's speed decays, per second.
fn decay() -> f32 {
    -1000.0 * 0.998_f32.ln()
}

/// A list that scrolls down, in a port 200 pixels tall, with rows as wide as the port.
const LIST: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 200px; overflow: auto }
.row { display: block; height: 40px }
";

/// The same list, with rows two pixels wider than the port.
const STRAY: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 200px; overflow: auto }
.row { display: block; height: 40px; margin-right: -2px }
";

/// The same list, with rows twice as wide as the port.
const WIDE: &str = "
root { display: block; width: 400px; height: 300px }
.port { display: block; width: 400px; height: 200px; overflow: auto }
.row { display: block; width: 800px; height: 40px }
";

/// A finger over one document, with the clock its events are stamped by.
struct Screen {
    harness: Harness<Runtime>,
    stamp: Duration,
    clicks: Rc<Cell<u32>>,
}

impl Screen {
    /// A window with 200 rows in one port, styled by `css`.
    fn new(css: &str) -> Self {
        let clicks = Rc::new(Cell::new(0));
        let heard = Rc::clone(&clicks);
        let mut harness = support::app_with_text_on(
            Headless::new(),
            css,
            move |cx: &mut zgui_view::BuildCx<'_>| {
                use zgui_view::{IntoView, View};
                let heard = Rc::clone(&heard);
                let mut port = zgui_elements::column()
                    .class("port")
                    .on(zgui_view::events::CLICK, move |_| {
                        heard.set(heard.get() + 1)
                    });
                for _ in 0..200 {
                    port = port.child(zgui_elements::column().class("row"));
                }
                let root = zgui_elements::column().class("root").child(port);
                Box::new(root.into_view().build(cx))
            },
        );
        harness.settle(8);
        Self {
            harness,
            stamp: Duration::from_secs(1),
            clicks,
        }
    }

    /// Delivers one event of the first finger at `(x, y)`, `after` the last event.
    fn touch(&mut self, action: PointerAction, x: f32, y: f32, after: Duration) {
        self.touch_with(PointerId::new(1), action, x, y, after);
    }

    /// Delivers one event of `finger` at `(x, y)`, `after` the last event.
    fn touch_with(
        &mut self,
        finger: PointerId,
        action: PointerAction,
        x: f32,
        y: f32,
        after: Duration,
    ) {
        self.stamp += after;
        self.harness.touch(
            action,
            finger,
            Point::<CssPx, Css>::new(CssPx(x), CssPx(y)),
            Timestamp::from_origin(self.stamp),
        );
        self.harness.settle(4);
    }

    /// Puts the finger down at `(x, y)`.
    fn press(&mut self, x: f32, y: f32) {
        self.touch(PointerAction::Pressed, x, y, BETWEEN);
    }

    /// Moves the finger to each point of `to` in turn, [`BETWEEN`] apart.
    fn drag(&mut self, to: &[(f32, f32)]) {
        for (x, y) in to {
            self.touch(PointerAction::Moved, *x, *y, BETWEEN);
        }
    }

    /// Lifts the finger at `(x, y)`, `after` its last move.
    fn lift(&mut self, x: f32, y: f32, after: Duration) {
        self.touch(PointerAction::Released, x, y, after);
    }

    /// Runs one frame of the clock with no input.
    fn frame(&mut self) {
        self.harness.advance(FRAME);
        self.harness.settle(4);
    }

    fn window(&self) -> &zgui_runtime::Window {
        self.harness.app().windows().first().expect("a window")
    }

    /// The port, which is the only scroll container.
    fn port(&self) -> NodeKey {
        let window = self.window();
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .filter(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
            .find_map(|key| layout.node(key).source)
            .expect("the fixture has a port")
    }

    /// Where the port is scrolled to, in device pixels.
    fn offset(&self) -> Point<DevicePx, Device> {
        self.window().scroll().borrow().offset_of(self.port())
    }

    /// How far the port is drawn past its end, in device pixels.
    fn stretch(&self) -> Size<DevicePx, Device> {
        self.window().scroll().borrow().elastic_of(self.port())
    }

    /// How far a person may scroll the port, in device pixels.
    fn range(&self) -> Point<DevicePx, Device> {
        let window = self.window();
        let layout = window.layout().borrow();
        zgui_layout::scroll_region::region_of_element(&layout, self.port())
            .expect("the port scrolls")
            .hand_limit()
    }

    /// Puts the port at `y` at once.
    fn scroll_to(&mut self, y: f32) {
        let port = self.port();
        let window = self.window();
        let layout = window.layout().borrow();
        window.scroll().borrow_mut().scroll_to(
            &layout,
            port,
            Point::new(DevicePx(0.0), DevicePx(y)),
            Behavior::Instant,
        );
    }

    fn is_animating(&self) -> bool {
        self.window().scroll().borrow().is_animating()
    }

    /// Runs frames until nothing moves on its own, and returns the vertical offset and stretch at
    /// each frame.
    fn run_out(&mut self) -> Vec<(f32, f32)> {
        let mut seen = Vec::new();
        while self.is_animating() && seen.len() < 1_000 {
            self.frame();
            seen.push((self.offset().y.0, self.stretch().height.0));
        }
        seen
    }
}

/// The displacement the rubber band gives a pull of `pulled` against a port `extent` long.
fn band(pulled: f32, extent: f32) -> f32 {
    (1.0 - 1.0 / (pulled * 0.55 / extent + 1.0)) * extent
}

#[test]
fn a_flick_decelerates_at_the_rate_of_uiscrollview() {
    let mut screen = Screen::new(LIST);
    screen.press(200.0, 190.0);
    // Fifteen pixels each ten milliseconds: 1500 pixels per second, upwards.
    let moves: Vec<(f32, f32)> = (1..=8)
        .map(|step| (200.0, 190.0 - 15.0 * step as f32))
        .collect();
    screen.drag(&moves);
    screen.lift(200.0, 70.0, Duration::ZERO);
    let released = screen.offset().y.0;
    assert!(screen.is_animating(), "the flick did not fling");

    let seen = screen.run_out();
    let steps: Vec<f32> = core::iter::once(released)
        .chain(seen.iter().map(|(offset, _)| *offset))
        .collect::<Vec<_>>()
        .windows(2)
        .map(|pair| pair[1] - pair[0])
        .collect();

    // Each frame of 20 ms travels 0.998^20 of the frame before it.
    for pair in steps.windows(2).take(60) {
        let ratio = pair[1] / pair[0];
        assert!(
            (ratio - frame_decay()).abs() < 1e-3,
            "one frame travelled {} and the next {}: a ratio of {ratio}, the model's {}",
            pair[0],
            pair[1],
            frame_decay()
        );
    }
    // The first frame carries the speed of the lift.
    let first = 1500.0 * (1.0 - frame_decay()) / decay();
    assert!(
        (steps[0] - first).abs() < first * 0.02,
        "the first frame travelled {}, the model {first}",
        steps[0]
    );
    // And the fling comes to rest where `v0 / -ln(r)` puts it.
    let travelled = seen.last().expect("frames").0 - released;
    let expected = 1500.0 / decay();
    assert!(
        (travelled - expected).abs() < expected * 0.01,
        "the fling travelled {travelled}, the model {expected}"
    );
}

#[test]
fn a_finger_pulling_past_the_top_follows_the_rubber_band_and_holds_it() {
    let mut screen = Screen::new(LIST);
    screen.press(200.0, 50.0);
    // Inside the slop, then past it: the pan begins with the step from 55 to 70.
    screen.drag(&[(200.0, 55.0), (200.0, 70.0)]);
    screen.drag(&[
        (200.0, 90.0),
        (200.0, 110.0),
        (200.0, 130.0),
        (200.0, 150.0),
    ]);
    let pulled = 15.0 + 80.0;
    let expected = -band(pulled, 200.0);
    let held = screen.stretch().height.0;
    assert!(
        (held - expected).abs() < 0.01,
        "a pull of {pulled} held the edge at {held}, the band at {expected}"
    );
    assert_eq!(screen.offset().y.0, 0.0);

    for _ in 0..10 {
        screen.frame();
    }
    assert_eq!(
        screen.stretch().height.0,
        held,
        "the edge moved under a finger that held it"
    );
}

#[test]
fn a_released_edge_springs_back_to_the_edge_on_the_spring_of_uiscrollview() {
    let mut screen = Screen::new(LIST);
    screen.press(200.0, 50.0);
    screen.drag(&[(200.0, 55.0), (200.0, 70.0), (200.0, 110.0), (200.0, 150.0)]);
    let from = screen.stretch().height.0;
    assert!(from < -30.0, "the pull held the edge at {from}");
    // Lifted after the finger stood still, so it lets go with no speed.
    screen.lift(200.0, 150.0, Duration::from_millis(100));

    let seen = screen.run_out();
    for (frame, (_, held)) in seen.iter().enumerate().take(20) {
        let t = (frame + 1) as f32 * FRAME.as_secs_f32();
        let spring = (from + 10.0 * from * t) * (-10.0 * t).exp();
        assert!(
            (held - spring).abs() < 0.05,
            "after {t} s the edge was at {held}, the spring at {spring}"
        );
    }
    assert!(
        seen.windows(2)
            .all(|pair| pair[1].1 >= pair[0].1 && pair[1].1 <= 0.0),
        "the edge crossed its end or turned back: {seen:?}"
    );
    assert_eq!(screen.stretch().height.0, 0.0, "the edge never came home");
    assert_eq!(screen.offset().y.0, 0.0, "the return moved the offset");
    assert!(screen.window().scroll().borrow().settled());
    assert!(
        (36..=60).contains(&seen.len()),
        "the return took {} frames",
        seen.len()
    );
}

#[test]
fn a_finger_that_lifts_while_it_pulls_throws_the_edge_on_before_it_returns() {
    let mut screen = Screen::new(LIST);
    screen.press(200.0, 50.0);
    // Twenty pixels each ten milliseconds: the finger lifts at 2000 pixels per second.
    screen.drag(&[
        (200.0, 55.0),
        (200.0, 70.0),
        (200.0, 90.0),
        (200.0, 110.0),
        (200.0, 130.0),
        (200.0, 150.0),
    ]);
    screen.lift(200.0, 150.0, Duration::ZERO);
    let from = screen.stretch().height.0;

    // The edge leaves at the speed the band moves it: the finger's speed times the slope of the
    // band, `c * (1 - h/d)^2`.
    let free = 1.0 - from.abs() / 200.0;
    let thrown = -2000.0 * 0.55 * free * free;
    screen.frame();
    let t = FRAME.as_secs_f32();
    let spring = (from + (thrown + 10.0 * from) * t) * (-10.0 * t).exp();
    let held = screen.stretch().height.0;
    assert!(
        held < from && (held - spring).abs() < 0.05,
        "released at {from}, the edge was at {held} a frame later, the spring at {spring}"
    );

    screen.run_out();
    assert_eq!(screen.stretch().height.0, 0.0, "the edge never came home");
    assert_eq!(screen.offset().y.0, 0.0);
}

#[test]
fn a_fling_that_reaches_the_end_bounces_on_the_spring() {
    let mut screen = Screen::new(LIST);
    screen.scroll_to(600.0);
    screen.press(200.0, 40.0);
    let moves: Vec<(f32, f32)> = (1..=8)
        .map(|step| (200.0, 40.0 + 15.0 * step as f32))
        .collect();
    screen.drag(&moves);
    screen.lift(200.0, 160.0, Duration::ZERO);
    let distance = screen.offset().y.0;
    assert!(distance > 400.0, "the drag left the port at {distance}");

    let seen = screen.run_out();
    let peak = seen.iter().map(|(_, held)| *held).fold(0.0, f32::min);
    // The fling meets the end at `v0 - k * distance`, and a spring thrown at `v` from its rest
    // point peaks at `v / (w * e)`.
    let hit = 1500.0 - decay() * distance;
    let expected = -hit / (10.0 * core::f32::consts::E);
    assert!(
        (peak - expected).abs() < expected.abs() * 0.03,
        "the bounce peaked at {peak}, the spring at {expected}"
    );
    assert_eq!(screen.offset().y.0, 0.0);
    assert_eq!(screen.stretch().height.0, 0.0, "the bounce never came home");
}

#[test]
fn a_finger_catches_a_fling_and_the_touch_is_not_a_tap() {
    let mut screen = Screen::new(LIST);
    screen.press(200.0, 190.0);
    let moves: Vec<(f32, f32)> = (1..=8)
        .map(|step| (200.0, 190.0 - 15.0 * step as f32))
        .collect();
    screen.drag(&moves);
    screen.lift(200.0, 70.0, Duration::ZERO);
    for _ in 0..5 {
        screen.frame();
    }
    assert!(screen.is_animating());

    screen.press(200.0, 100.0);
    let caught = screen.offset();
    assert!(!screen.is_animating(), "the touch did not stop the fling");
    for _ in 0..5 {
        screen.frame();
    }
    screen.lift(200.0, 100.0, Duration::from_millis(80));
    for _ in 0..5 {
        screen.frame();
    }
    assert_eq!(screen.offset(), caught, "the content moved after the catch");
    assert_eq!(screen.clicks.get(), 0, "the catch was a tap");

    // A tap on content at rest is a tap.
    screen.press(200.0, 100.0);
    screen.lift(200.0, 100.0, Duration::from_millis(80));
    assert_eq!(screen.clicks.get(), 1);
}

#[test]
fn a_page_with_a_stray_range_does_not_stretch_sideways() {
    let mut screen = Screen::new(STRAY);
    assert_eq!(screen.range().x.0, 2.0, "the fixture has no stray range");
    // Diagonal enough that the pan moves both axes.
    screen.press(200.0, 150.0);
    screen.drag(&[(210.0, 135.0), (230.0, 110.0), (250.0, 85.0), (270.0, 60.0)]);
    assert_eq!(screen.stretch().width.0, 0.0, "the page swam sideways");
    assert!(screen.offset().x.0 <= 2.0);
    assert!(screen.offset().y.0 > 0.0, "the page did not scroll down");
    screen.lift(270.0, 60.0, Duration::from_millis(100));
}

#[test]
fn a_page_with_no_horizontal_overflow_does_not_move_sideways() {
    let mut screen = Screen::new(LIST);
    assert_eq!(screen.range().x.0, 0.0);
    screen.press(200.0, 150.0);
    screen.drag(&[(210.0, 135.0), (230.0, 110.0), (250.0, 85.0), (270.0, 60.0)]);
    screen.lift(270.0, 60.0, Duration::ZERO);
    for _ in 0..30 {
        screen.frame();
        assert_eq!(screen.offset().x.0, 0.0);
        assert_eq!(screen.stretch().width.0, 0.0);
    }
}

#[test]
fn a_page_that_overflows_sideways_stretches_sideways() {
    let mut screen = Screen::new(WIDE);
    screen.press(100.0, 100.0);
    screen.drag(&[(115.0, 100.0), (135.0, 100.0), (155.0, 100.0)]);
    assert!(
        screen.stretch().width.0 < 0.0,
        "a pull past the left edge of wide content held {:?}",
        screen.stretch()
    );
    screen.lift(155.0, 100.0, Duration::from_millis(100));
}

#[test]
fn a_second_finger_that_takes_the_scroll_over_pans_on_its_own_axis() {
    let mut screen = Screen::new(WIDE);
    screen.press(100.0, 150.0);
    screen.drag(&[(100.0, 130.0), (100.0, 110.0)]);
    let down = screen.offset().y.0;
    assert!(down > 0.0, "the first finger did not scroll down");

    let second = PointerId::new(2);
    screen.touch_with(second, PointerAction::Pressed, 300.0, 150.0, BETWEEN);
    for x in [285.0, 265.0, 245.0] {
        screen.touch_with(second, PointerAction::Moved, x, 150.0, BETWEEN);
    }
    assert!(
        screen.offset().x.0 > 0.0,
        "the second finger's sideways pan kept the first finger's rail"
    );
    assert_eq!(screen.offset().y.0, down);

    screen.touch_with(second, PointerAction::Released, 245.0, 150.0, FRAME);
    screen.lift(100.0, 110.0, FRAME);
}
