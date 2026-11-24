//! Touch sequences replayed over a chat: a vertical list of messages, each of which listens for
//! horizontal pans to pull it to the right, as swipe to reply does, in a page that takes the other
//! pans to the right to go back, as a navigation stack does.
//!
//! Each case scripts the contact at the platform seam, stamped as a platform that stamps events on
//! arrival would stamp them, and compares the result with UIScrollView: a pan that no message takes
//! scrolls, a touch on moving content stops it and is not a tap, a flick flings at the speed of the
//! finger, and a new flick during a fling starts from the speed of the new finger alone.

mod support;

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::Runtime;
use zgui_scroll::Behavior;
use zgui_vocab::{GesturePhase, PanEvent, PointerAction, PointerId, Timestamp};

/// The time between two touch samples on a 120 Hz touch screen, rounded to the millisecond.
const SAMPLE: Duration = Duration::from_millis(8);

/// Where the list starts, far enough from both ends that no fling here reaches one.
const MIDDLE: f32 = 3_000.0;

/// When the samples of a flick are stamped.
#[derive(Clone)]
enum Arrival<'a> {
    /// Each when it is taken.
    OnTime,
    /// Each late by the matching entry, in milliseconds, as a platform that stamps an event when the
    /// busy thread gets to it does.
    Late(&'a [u64]),
    /// Each when it is taken, except those of the range, which arrive folded into the next one
    /// while the thread is busy.
    Stalled(core::ops::Range<usize>),
}

/// What a message does with a horizontal pan it is offered.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Messages {
    /// It does not listen for pans.
    Bare,
    /// It takes a pan to the right, to pull itself, and refuses one to the left.
    Swipe,
}

/// A finger over a chat, with the clock its events are stamped by.
struct Chat {
    harness: Harness<Runtime>,
    /// When the finger's next sample is taken.
    stamp: Duration,
    /// How many pans the messages took.
    pulled: Rc<Cell<u32>>,
    /// How many pans the page took.
    paged: Rc<Cell<u32>>,
    /// How many times the list was clicked.
    clicks: Rc<Cell<u32>>,
}

impl Chat {
    fn new(messages: Messages) -> Self {
        const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.list { display: block; width: 400px; height: 300px; overflow-y: auto }
.message { display: block; height: 40px }
";

        let pulled = Rc::new(Cell::new(0));
        let paged = Rc::new(Cell::new(0));
        let clicks = Rc::new(Cell::new(0));
        let (took, went_back, clicked) =
            (Rc::clone(&pulled), Rc::clone(&paged), Rc::clone(&clicks));
        let mut harness = support::app_with_text_on(
            Headless::new(),
            CSS,
            move |cx: &mut zgui_view::BuildCx<'_>| {
                use zgui_view::{IntoView, View};
                let clicked = Rc::clone(&clicked);
                let mut list = zgui_elements::column()
                    .class("list")
                    .on(zgui_view::events::CLICK, move |_| {
                        clicked.set(clicked.get() + 1)
                    });
                for _ in 0..200 {
                    let message = zgui_elements::column().class("message");
                    let message = match messages {
                        Messages::Bare => message,
                        Messages::Swipe => message.on(
                            zgui_view::events::HORIZONTAL_PAN,
                            rightwards(Rc::clone(&took)),
                        ),
                    };
                    list = list.child(message);
                }
                let root = zgui_elements::column()
                    .class("root")
                    .on(
                        zgui_view::events::HORIZONTAL_PAN,
                        rightwards(Rc::clone(&went_back)),
                    )
                    .child(list);
                Box::new(root.into_view().build(cx))
            },
        );
        harness.settle(8);
        let mut chat = Self {
            harness,
            stamp: Duration::from_secs(1),
            pulled,
            paged,
            clicks,
        };
        chat.scroll_to(MIDDLE);
        chat
    }

    /// Delivers one event of the finger at `(x, y)`, sampled `after` the previous sample and
    /// stamped `late` after it was sampled.
    fn touch(
        &mut self,
        action: PointerAction,
        (x, y): (f32, f32),
        after: Duration,
        late: Duration,
    ) {
        self.stamp += after;
        self.harness.touch(
            action,
            PointerId::new(1),
            Point::<CssPx, Css>::new(CssPx(x), CssPx(y)),
            Timestamp::from_origin(self.stamp + late),
        );
        self.harness.settle(4);
    }

    fn press(&mut self, at: (f32, f32)) {
        self.touch(PointerAction::Pressed, at, SAMPLE, Duration::ZERO);
    }

    /// Moves the finger through `to`, one sample apart, each stamped on time.
    fn drag(&mut self, to: &[(f32, f32)]) {
        for at in to {
            self.touch(PointerAction::Moved, *at, SAMPLE, Duration::ZERO);
        }
    }

    fn lift(&mut self, at: (f32, f32)) {
        self.touch(PointerAction::Released, at, SAMPLE, Duration::ZERO);
    }

    /// Flicks vertically from `(x, y)` at `speed` CSS pixels per second, downwards when positive,
    /// for `samples` samples one [`SAMPLE`] apart, and lifts one sample later. `arrival` says when
    /// each sample is stamped.
    fn flick(&mut self, (x, y): (f32, f32), speed: f32, samples: usize, arrival: Arrival<'_>) {
        let step = speed * SAMPLE.as_secs_f32();
        self.press((x, y));
        let mut waited = Duration::ZERO;
        for index in 1..=samples + 1 {
            waited += SAMPLE;
            let late = match &arrival {
                Arrival::OnTime => Duration::ZERO,
                Arrival::Late(lateness) => Duration::from_millis(lateness[index % lateness.len()]),
                Arrival::Stalled(stalled) if stalled.contains(&index) => continue,
                Arrival::Stalled(_) => Duration::ZERO,
            };
            let action = if index > samples {
                PointerAction::Released
            } else {
                PointerAction::Moved
            };
            self.touch(action, (x, y + step * index as f32), waited, late);
            waited = Duration::ZERO;
        }
    }

    fn frame(&mut self) {
        // One frame, rounded up past the deadline the park installs.
        const FRAME: Duration = Duration::from_millis(20);

        self.harness.advance(FRAME);
        self.harness.settle(4);
    }

    fn window(&self) -> &zgui_runtime::Window {
        self.harness.app().windows().first().expect("a window")
    }

    /// The list, which is the only scroll container.
    fn list(&self) -> NodeKey {
        let window = self.window();
        let layout = window.layout().borrow();
        layout
            .keys()
            .into_iter()
            .filter(|key| zgui_layout::scroll_region::region_of(&layout, *key).is_some())
            .find_map(|key| layout.node(key).source)
            .expect("the fixture has a list")
    }

    /// Where the list is scrolled to vertically, in device pixels.
    fn offset(&self) -> f32 {
        self.window().scroll().borrow().offset_of(self.list()).y.0
    }

    fn scroll_to(&mut self, y: f32) {
        let list = self.list();
        let window = self.window();
        let layout = window.layout().borrow();
        window.scroll().borrow_mut().scroll_to(
            &layout,
            list,
            zgui_geom::Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(y)),
            Behavior::Instant,
        );
    }

    fn is_animating(&self) -> bool {
        self.window().scroll().borrow().is_animating()
    }

    /// Runs frames until nothing moves, and returns how far the list travelled meanwhile.
    fn run_out(&mut self) -> f32 {
        let from = self.offset();
        for _ in 0..1_000 {
            if !self.is_animating() {
                break;
            }
            self.frame();
        }
        self.offset() - from
    }

    /// The speed the list was thrown at, in device pixels per second, read from how far its fling
    /// travels: a fling at `v` comes to rest `v / -ln(r)` further on.
    fn thrown(&mut self) -> f32 {
        self.run_out() * decay()
    }
}

/// How fast a fling's speed decays, per second.
fn decay() -> f32 {
    -1000.0 * 0.998_f32.ln()
}

/// A horizontal pan listener that takes each pan to the right, counting them in `took`, and
/// refuses each pan to the left.
fn rightwards(
    took: Rc<Cell<u32>>,
) -> impl Fn(&mut zgui_view::EventCx<'_, zgui_view::events::HorizontalPan>) + 'static {
    move |event| {
        let pan: PanEvent = **event;
        if pan.phase != GesturePhase::Started {
            return;
        }
        if pan.translation().width.0 <= 0.0 {
            event.prevent_default();
            return;
        }
        took.set(took.get() + 1);
    }
}

/// Asserts that a fling was thrown at `expected` device pixels per second, to within `share` of it.
fn assert_thrown(thrown: f32, expected: f32, share: f32) {
    assert!(
        (thrown - expected).abs() <= expected.abs() * share,
        "the list was thrown at {thrown} px/s; the finger moved at {expected} px/s"
    );
}

#[test]
fn a_vertical_drag_from_anywhere_on_a_message_scrolls() {
    for x in [5.0, 120.0, 200.0, 395.0] {
        let mut chat = Chat::new(Messages::Swipe);
        chat.press((x, 250.0));
        let path: Vec<(f32, f32)> = (1..=10)
            .map(|step| (x, 250.0 - 15.0 * step as f32))
            .collect();
        chat.drag(&path);
        let moved = chat.offset() - MIDDLE;
        assert!(
            moved > 120.0,
            "a drag of 150 px from x = {x} scrolled {moved} px"
        );
        assert_eq!(chat.pulled.get(), 0, "a message took a vertical drag");
        chat.lift((x, 100.0));
    }
}

#[test]
fn a_drag_that_leaves_sideways_and_turns_upwards_scrolls() {
    for messages in [Messages::Bare, Messages::Swipe] {
        let mut chat = Chat::new(messages);
        chat.press((200.0, 250.0));
        // A thumb that rolls to the left before it pushes upwards: past the slop, the travel is
        // mostly sideways.
        chat.drag(&[(195.0, 249.0), (189.0, 247.0)]);
        let path: Vec<(f32, f32)> = (1..=10)
            .map(|step| (186.0, 247.0 - 15.0 * step as f32))
            .collect();
        chat.drag(&path);
        let moved = chat.offset() - MIDDLE;
        assert!(
            moved > 120.0,
            "a drag of 150 px upwards scrolled {moved} px: a dead swipe"
        );
        assert_eq!(chat.pulled.get(), 0, "a message took a pan to the left");
        assert_eq!(chat.paged.get(), 0, "the page took a pan to the left");
        chat.lift((186.0, 97.0));
    }
}

#[test]
fn a_drag_that_rolls_right_as_it_pushes_upwards_scrolls() {
    for messages in [Messages::Bare, Messages::Swipe] {
        let mut chat = Chat::new(messages);
        chat.press((200.0, 250.0));
        // Past the slop the travel is 40 degrees from the horizontal.
        chat.drag(&[(205.0, 246.0), (209.0, 242.5)]);
        let path: Vec<(f32, f32)> = (1..=10)
            .map(|step| (212.0, 242.5 - 15.0 * step as f32))
            .collect();
        chat.drag(&path);
        let moved = chat.offset() - MIDDLE;
        assert!(
            moved > 120.0,
            "a drag of 150 px upwards scrolled {moved} px: a dead swipe"
        );
        assert_eq!(
            chat.pulled.get() + chat.paged.get(),
            0,
            "a steep pan was taken as a swipe"
        );
        chat.lift((212.0, 92.5));
    }
}

#[test]
fn a_small_diagonal_drag_scrolls() {
    let mut chat = Chat::new(Messages::Swipe);
    chat.press((200.0, 250.0));
    chat.drag(&[
        (203.0, 247.0),
        (207.0, 243.0),
        (212.0, 237.0),
        (218.0, 229.0),
        (224.0, 220.0),
    ]);
    let moved = chat.offset() - MIDDLE;
    assert!(
        moved > 15.0,
        "a diagonal drag of 30 px upwards scrolled {moved} px"
    );
    assert_eq!(chat.pulled.get(), 0);
    chat.lift((224.0, 220.0));
}

#[test]
fn a_message_takes_a_pan_to_the_right_and_the_list_stays() {
    let mut chat = Chat::new(Messages::Swipe);
    chat.press((100.0, 150.0));
    chat.drag(&[
        (108.0, 151.0),
        (118.0, 152.0),
        (140.0, 140.0),
        (160.0, 100.0),
    ]);
    assert_eq!(chat.pulled.get(), 1, "the message did not take the swipe");
    assert_eq!(chat.paged.get(), 0, "the page took a swipe a message took");
    assert_eq!(
        chat.offset(),
        MIDDLE,
        "the list moved under a swipe a message took"
    );
    chat.lift((160.0, 100.0));
}

#[test]
fn a_flick_flings_at_the_speed_of_the_finger() {
    let mut chat = Chat::new(Messages::Swipe);
    chat.flick((200.0, 280.0), -1_500.0, 12, Arrival::OnTime);
    assert_thrown(chat.thrown(), 1_500.0, 0.05);
}

#[test]
fn a_flick_whose_events_arrive_late_flings_at_the_speed_of_the_finger() {
    // Each sample waits for the frame that is drawing when it arrives: 0 to 7 ms.
    let lateness: [u64; 13] = [0, 6, 1, 7, 0, 5, 7, 2, 0, 6, 3, 7, 1];
    for shift in 0..lateness.len() {
        let mut lateness = lateness;
        lateness.rotate_left(shift);
        let mut chat = Chat::new(Messages::Swipe);
        chat.flick((200.0, 280.0), -1_500.0, 12, Arrival::Late(&lateness));
        let thrown = chat.thrown();
        assert!(
            (thrown - 1_500.0).abs() <= 1_500.0 * 0.2,
            "with lateness {lateness:?} the list was thrown at {thrown} px/s; the finger moved at \
             1500 px/s"
        );
    }
}

#[test]
fn a_flick_through_a_long_frame_still_flings() {
    // The thread is busy for 48 ms in the middle of the flick. The samples taken meanwhile arrive
    // as one, when it is free again.
    let mut chat = Chat::new(Messages::Swipe);
    chat.flick((200.0, 280.0), -1_500.0, 12, Arrival::Stalled(6..11));
    assert_thrown(chat.thrown(), 1_500.0, 0.2);
}

#[test]
fn a_touch_during_a_fling_stops_it_at_once_and_is_not_a_tap() {
    let mut chat = Chat::new(Messages::Swipe);
    chat.flick((200.0, 280.0), -1_500.0, 12, Arrival::OnTime);
    for _ in 0..5 {
        chat.frame();
    }
    assert!(chat.is_animating(), "the flick did not fling");

    chat.press((200.0, 150.0));
    let caught = chat.offset();
    assert!(!chat.is_animating(), "the touch did not stop the fling");
    for _ in 0..3 {
        chat.frame();
    }
    chat.touch(
        PointerAction::Released,
        (200.0, 150.0),
        Duration::from_millis(60),
        Duration::ZERO,
    );
    for _ in 0..5 {
        chat.frame();
    }
    assert_eq!(chat.offset(), caught, "the content moved after the catch");
    assert_eq!(chat.clicks.get(), 0, "the catch was a tap");
}

#[test]
fn a_flick_back_during_a_fling_flings_at_the_speed_of_the_new_finger() {
    let mut chat = Chat::new(Messages::Swipe);
    chat.flick((200.0, 280.0), -2_000.0, 12, Arrival::OnTime);
    for _ in 0..8 {
        chat.frame();
    }
    assert!(chat.is_animating(), "the flick did not fling");

    // The second flick goes the other way, slower than what is left of the first.
    chat.flick((200.0, 60.0), 1_000.0, 12, Arrival::OnTime);
    assert_thrown(chat.thrown(), -1_000.0, 0.05);
}

#[test]
fn a_finger_that_stops_before_it_lifts_throws_nothing() {
    let mut chat = Chat::new(Messages::Swipe);
    chat.press((200.0, 280.0));
    let path: Vec<(f32, f32)> = (1..=10)
        .map(|step| (200.0, 280.0 - 12.0 * step as f32))
        .collect();
    chat.drag(&path);
    chat.touch(
        PointerAction::Released,
        (200.0, 160.0),
        Duration::from_millis(120),
        Duration::ZERO,
    );
    assert_eq!(
        chat.run_out(),
        0.0,
        "a finger that stopped still flung the list"
    );
}
