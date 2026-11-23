//! Who gets a touch pan, what it reports, and when a touch shows as pressed.
//!
//! The fixture is a panel that listens for vertical and horizontal pans, with a vertical list of
//! rows inside it, as a bottom sheet holds one.

mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_platform_headless::{Harness, Headless};
use zgui_runtime::Runtime;
use zgui_vocab::{
    GesturePhase, ListenerOptions, PanEvent, PointerAction, PointerId, ScreenEdge, Timestamp,
    UiState,
};

/// One frame, rounded up past the deadline the park installs.
const FRAME: Duration = Duration::from_millis(20);

/// The time between two moves of a finger.
const BETWEEN: Duration = Duration::from_millis(10);

const CSS: &str = "
root { display: block; width: 400px; height: 300px }
.panel { display: block; width: 400px; height: 300px }
.list { display: block; width: 400px; height: 200px; overflow-y: auto }
.row { display: block; height: 40px }
";

/// How the panel listens.
#[derive(Clone, Copy)]
struct Panel {
    /// Whether it listens in the capture phase.
    capture: bool,
    /// Whether it refuses each pan it is offered.
    refuses: bool,
}

/// A finger over the fixture, with the clock its events are stamped by.
struct Screen {
    harness: Harness<Runtime>,
    stamp: Duration,
    /// Every pan event the panel heard.
    heard: Rc<RefCell<Vec<PanEvent>>>,
    /// How many times the first row was clicked.
    clicks: Rc<Cell<u32>>,
}

impl Screen {
    fn new(panel: Panel) -> Self {
        let heard: Rc<RefCell<Vec<PanEvent>>> = Rc::default();
        let clicks = Rc::new(Cell::new(0));
        let (shared, clicked) = (Rc::clone(&heard), Rc::clone(&clicks));
        let mut harness = support::app_with_text_on(
            Headless::new(),
            CSS,
            move |cx: &mut zgui_view::BuildCx<'_>| {
                use zgui_view::{IntoView, View};
                let options = if panel.capture {
                    ListenerOptions::CAPTURE
                } else {
                    ListenerOptions::DEFAULT
                };
                let clicked = Rc::clone(&clicked);
                let mut list = zgui_elements::column().class("list").child(
                    zgui_elements::control()
                        .class("row")
                        .on(zgui_view::events::CLICK, move |_| {
                            clicked.set(clicked.get() + 1)
                        }),
                );
                for _ in 0..19 {
                    list = list.child(zgui_elements::column().class("row"));
                }
                let panel = zgui_elements::column()
                    .class("panel")
                    .on_with(
                        zgui_view::events::VERTICAL_PAN,
                        options,
                        recorder(Rc::clone(&shared), panel.refuses),
                    )
                    .on_with(
                        zgui_view::events::HORIZONTAL_PAN,
                        options,
                        recorder(Rc::clone(&shared), panel.refuses),
                    )
                    .child(list);
                let root = zgui_elements::column().class("root").child(panel);
                Box::new(root.into_view().build(cx))
            },
        );
        harness.settle(8);
        Self {
            harness,
            stamp: Duration::from_secs(1),
            heard,
            clicks,
        }
    }

    fn touch(&mut self, action: PointerAction, x: f32, y: f32, after: Duration) {
        self.stamp += after;
        self.harness.touch(
            action,
            PointerId::new(1),
            Point::<CssPx, Css>::new(CssPx(x), CssPx(y)),
            Timestamp::from_origin(self.stamp),
        );
        self.harness.settle(4);
    }

    fn press(&mut self, x: f32, y: f32) {
        self.touch(PointerAction::Pressed, x, y, BETWEEN);
    }

    fn drag(&mut self, to: &[(f32, f32)]) {
        for (x, y) in to {
            self.touch(PointerAction::Moved, *x, *y, BETWEEN);
        }
    }

    fn lift(&mut self, x: f32, y: f32) {
        self.touch(PointerAction::Released, x, y, BETWEEN);
    }

    /// Runs frames with no input for at least `long`.
    fn wait(&mut self, long: Duration) {
        let mut waited = Duration::ZERO;
        while waited < long {
            self.harness.advance(FRAME);
            self.harness.settle(4);
            waited += FRAME;
        }
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

    /// Where the list is scrolled to, and how far it is stretched past an end, in device pixels.
    fn list_moved(&self) -> (f32, f32) {
        let scroller = self.window().scroll().borrow();
        let list = self.list();
        (
            scroller.offset_of(list).y.0,
            scroller.elastic_of(list).height.0,
        )
    }

    fn phases(&self) -> Vec<GesturePhase> {
        self.heard.borrow().iter().map(|pan| pan.phase).collect()
    }

    /// Whether anything shows as pressed.
    fn pressed(&self) -> bool {
        let window = self.window();
        let document = window.document().borrow();
        let store = document.store();
        (0..store.slot_count()).any(|index| {
            store
                .try_core(zgui_dom::NodeIndex::new(index as u32))
                .is_some_and(|core| core.ui_state().contains(UiState::ACTIVE))
        })
    }
}

/// A pan listener that records each event in `heard`, and refuses each pan when `refuses`.
fn recorder<E>(
    heard: Rc<RefCell<Vec<PanEvent>>>,
    refuses: bool,
) -> impl Fn(&mut zgui_view::EventCx<'_, E>) + 'static
where
    E: zgui_view::EventView<Payload = PanEvent>,
{
    move |event| {
        let pan: PanEvent = **event;
        if refuses && pan.phase == GesturePhase::Started {
            event.prevent_default();
        }
        heard.borrow_mut().push(pan);
    }
}

/// A listener in the bubble phase, which takes no pan from a list that can scroll.
const BUBBLE: Panel = Panel {
    capture: false,
    refuses: false,
};

/// A listener in the capture phase, which is offered pans before the list.
const CAPTURE: Panel = Panel {
    capture: true,
    refuses: false,
};

/// A listener in the capture phase that refuses each pan.
const REFUSING: Panel = Panel {
    capture: true,
    refuses: true,
};

#[test]
fn a_downward_pan_on_a_list_at_its_top_goes_to_the_panel_and_does_not_stretch_the_list() {
    let mut screen = Screen::new(BUBBLE);
    screen.press(200.0, 50.0);
    screen.drag(&[(200.0, 70.0), (200.0, 100.0), (200.0, 130.0)]);
    screen.lift(200.0, 130.0);

    assert_eq!(
        screen.phases(),
        [
            GesturePhase::Started,
            GesturePhase::Moved,
            GesturePhase::Moved,
            GesturePhase::Moved,
            GesturePhase::Ended
        ]
    );
    assert_eq!(screen.list_moved(), (0.0, 0.0));
}

#[test]
fn an_upward_pan_on_a_list_that_can_scroll_scrolls_it_past_a_bubble_listener() {
    let mut screen = Screen::new(BUBBLE);
    screen.press(200.0, 150.0);
    screen.drag(&[(200.0, 130.0), (200.0, 100.0), (200.0, 70.0)]);
    screen.lift(200.0, 70.0);

    assert!(screen.phases().is_empty(), "{:?}", screen.phases());
    assert!(screen.list_moved().0 > 0.0);
}

#[test]
fn a_capture_listener_claims_a_pan_that_the_list_could_scroll() {
    let mut screen = Screen::new(CAPTURE);
    screen.press(200.0, 150.0);
    screen.drag(&[(200.0, 130.0), (200.0, 100.0), (200.0, 70.0)]);
    screen.lift(200.0, 70.0);

    assert_eq!(screen.phases().first(), Some(&GesturePhase::Started));
    assert_eq!(screen.phases().last(), Some(&GesturePhase::Ended));
    assert_eq!(
        screen.list_moved(),
        (0.0, 0.0),
        "the list scrolled under a claim"
    );
}

#[test]
fn a_listener_that_refuses_the_start_leaves_the_pan_to_the_list() {
    let mut screen = Screen::new(REFUSING);
    screen.press(200.0, 150.0);
    screen.drag(&[(200.0, 130.0), (200.0, 100.0), (200.0, 70.0)]);
    screen.lift(200.0, 70.0);

    assert_eq!(
        screen.phases(),
        [GesturePhase::Started],
        "a refused pan kept sending events"
    );
    assert!(
        screen.list_moved().0 > 0.0,
        "the refused pan did not scroll"
    );
}

#[test]
fn a_pan_from_the_left_edge_says_so() {
    let mut screen = Screen::new(BUBBLE);
    screen.press(6.0, 100.0);
    screen.drag(&[(25.0, 100.0), (60.0, 102.0)]);
    screen.lift(60.0, 102.0);
    assert!(
        screen
            .heard
            .borrow()
            .iter()
            .all(|pan| pan.edge == Some(ScreenEdge::Left)),
        "{:?}",
        screen.heard.borrow()
    );

    screen.heard.borrow_mut().clear();
    screen.press(100.0, 100.0);
    screen.drag(&[(120.0, 100.0), (160.0, 102.0)]);
    screen.lift(160.0, 102.0);
    let heard = screen.heard.borrow();
    assert!(!heard.is_empty());
    assert!(heard.iter().all(|pan| pan.edge.is_none()), "{heard:?}");
}

#[test]
fn a_held_touch_shows_as_pressed_after_a_moment() {
    let mut screen = Screen::new(BUBBLE);
    screen.press(200.0, 20.0);
    assert!(!screen.pressed(), "the press showed at once");

    screen.wait(Duration::from_millis(60));
    assert!(!screen.pressed(), "the press showed before the delay");
    screen.wait(Duration::from_millis(80));
    assert!(screen.pressed(), "a held touch never showed as pressed");

    screen.lift(200.0, 20.0);
    assert!(!screen.pressed());
    assert_eq!(screen.clicks.get(), 1);
}

#[test]
fn a_touch_that_starts_a_scroll_never_shows_as_pressed() {
    let mut screen = Screen::new(BUBBLE);
    screen.scroll_list_a_little();
    screen.press(200.0, 150.0);
    screen.drag(&[(200.0, 135.0), (200.0, 110.0)]);
    for step in 1..=8 {
        screen.drag(&[(200.0, 110.0 - 3.0 * step as f32)]);
        screen.wait(FRAME);
        assert!(!screen.pressed(), "a row lit up under a scroll");
    }
    screen.lift(200.0, 86.0);
    screen.wait(Duration::from_millis(200));
    assert!(!screen.pressed());
    assert_eq!(screen.clicks.get(), 0);
}

#[test]
fn a_pan_takes_away_a_press_that_already_shows() {
    let mut screen = Screen::new(BUBBLE);
    screen.press(200.0, 20.0);
    screen.wait(Duration::from_millis(140));
    assert!(screen.pressed());

    screen.drag(&[(200.0, 40.0), (200.0, 60.0)]);
    assert!(!screen.pressed(), "the press stayed on through a pan");
    screen.lift(200.0, 60.0);
}

#[test]
fn a_quick_tap_shows_as_pressed_briefly() {
    let mut screen = Screen::new(BUBBLE);
    screen.press(200.0, 20.0);
    screen.lift(200.0, 20.0);
    assert!(screen.pressed(), "a quick tap showed nothing");
    assert_eq!(screen.clicks.get(), 1);

    screen.wait(Duration::from_millis(140));
    assert!(!screen.pressed(), "the tap's press stayed on");
}

impl Screen {
    /// Scrolls the list off its top, so the row under the finger starts a scroll in either
    /// direction.
    fn scroll_list_a_little(&mut self) {
        let list = self.list();
        let window = self.window();
        let layout = window.layout().borrow();
        window.scroll().borrow_mut().scroll_to(
            &layout,
            list,
            Point::new(zgui_geom::DevicePx(0.0), zgui_geom::DevicePx(40.0)),
            zgui_scroll::Behavior::Instant,
        );
    }
}
