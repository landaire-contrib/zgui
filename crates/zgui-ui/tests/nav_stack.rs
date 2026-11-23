//! A navigation stack, driven through real frames.

mod harness;

use core::cell::Cell;
use core::time::Duration;
use std::rc::Rc;

use zgui::geom::Css;
use zgui::geom::CssPx;
use zgui::geom::Point;
use zgui::geom::Size;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::view::CustomPropertyName;
use zgui::vocab::AnimationEvent;
use zgui::vocab::AnimationPhase;
use zgui::vocab::DefaultAction;
use zgui::vocab::EventKind;
use zgui::vocab::GesturePhase;
use zgui::vocab::PanEvent;
use zgui::vocab::Payload;
use zgui::vocab::PointerKind;
use zgui::vocab::ScrollEvent;
use zgui::vocab::SemanticFlags;
use zgui::vocab::TransitionEvent;
use zgui::vocab::TransitionPhase;
use zgui::vocab::Velocity;
use zgui_ui::prelude::*;

use crate::harness::Harness;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Route {
    Root,
    Group,
    Entry,
}

impl Route {
    fn title(self) -> &'static str {
        match self {
            Self::Root => "Passwords",
            Self::Group => "Bank",
            Self::Entry => "Checking",
        }
    }
}

/// What a test holds of a mounted stack.
struct Stack {
    harness: Harness,
    path: NavPath<Route>,
    /// Whether the entry page holds unsaved edits, which blocks back.
    editing: RwSignal<bool, LocalStorage>,
    /// How many page views the stack built.
    built: Rc<Cell<usize>>,
}

fn mount() -> Stack {
    let harness = Harness::open();
    let path = harness.window.scope.with(|| NavPath::new(Route::Root));
    let editing = harness.window.scope.with(|| RwSignal::new_local(false));
    let built = Rc::new(Cell::new(0));
    let counter = Rc::clone(&built);
    harness.mount(move || {
        view! {
            NavStack(path = path, page = move |route: Route| {
                counter.set(counter.get() + 1);
                let blocked = Signal::derive_local(move || route == Route::Entry && editing.get());
                view! {
                    NavPage(title = route.title(), back_blocked = blocked) {
                        text(class = "page-body") {{route.title()}}
                    }
                }
            })
        }
    });

    Stack {
        harness,
        path,
        editing,
        built,
    }
}

fn places(harness: &Harness, class: &str) -> Vec<String> {
    harness
        .find_all(class)
        .into_iter()
        .map(|node| harness.attribute(node, "data-place").unwrap_or_default())
        .collect()
}

fn back_button(harness: &Harness) -> Option<NodeId> {
    harness.find_all("zui-nav-back").into_iter().find(|node| {
        let row = harness
            .window
            .dom
            .tree()
            .parent(*node)
            .and_then(|leading| harness.window.dom.tree().parent(leading));
        row.is_some_and(|row| harness.attribute(row, "data-place").as_deref() == Some("current"))
    })
}

/// Runs the frames past the deadline of a motion that no animation ends.
fn run_out(harness: &Harness) {
    harness.window.frame();
    harness.window.advance(Duration::from_secs(1));
}

fn end_animation(harness: &Harness, node: NodeId) {
    harness.window.dispatcher().send_to(
        node,
        EventKind::AnimationEnd,
        Payload::Animation(AnimationEvent {
            name: zgui::view::Ident::new("zui-nav-slide-in"),
            elapsed: Duration::from_millis(500),
            phase: AnimationPhase::Ended,
            pseudo: None,
        }),
    );
    harness.window.frame();
}

#[test]
fn a_push_keeps_the_page_below_mounted_and_hides_it_once_the_motion_ends() {
    let Stack {
        harness,
        path,
        built,
        ..
    } = mount();
    let root = harness.find("zui-nav-page");

    harness.window.scope.with(|| path.push(Route::Group));
    harness.window.frame();
    assert_eq!(places(&harness, "zui-nav-page"), ["push-out", "push-in"]);
    assert_eq!(
        places(&harness, "zui-nav-bar__row"),
        ["push-out", "push-in"]
    );

    let pushed = harness.find_all("zui-nav-page")[1];
    end_animation(&harness, pushed);
    assert_eq!(places(&harness, "zui-nav-page"), ["covered", "current"]);
    assert_eq!(
        harness.find_all("zui-nav-page")[0],
        root,
        "the root page was kept"
    );
    assert_eq!(built.get(), 2, "each page was built once");

    let semantics = harness.window.dom.tree().semantics(root);
    assert!(semantics.is_some_and(|semantics| semantics.flags.contains(SemanticFlags::HIDDEN)));
}

#[test]
fn a_popped_page_stays_until_its_animation_ends() {
    let Stack {
        harness,
        path,
        built,
        ..
    } = mount();
    harness.window.scope.with(|| path.push(Route::Group));
    run_out(&harness);
    let root = harness.find_all("zui-nav-page")[0];
    let group = harness.find_all("zui-nav-page")[1];

    harness.click(back_button(&harness).expect("the group page offers back"));
    assert_eq!(places(&harness, "zui-nav-page"), ["pop-in", "pop-out"]);
    assert_eq!(harness.window.scope.with(|| path.pages()), [Route::Root]);

    end_animation(&harness, group);
    assert_eq!(harness.find_all("zui-nav-page"), [root]);
    assert_eq!(places(&harness, "zui-nav-page"), ["current"]);
    assert_eq!(built.get(), 2, "the root page was not built again");
}

#[test]
fn a_motion_with_no_animation_ends_at_the_deadline() {
    let Stack { harness, path, .. } = mount();
    harness.window.scope.with(|| path.push(Route::Group));
    harness.window.frame();
    assert_eq!(places(&harness, "zui-nav-page"), ["push-out", "push-in"]);

    run_out(&harness);
    assert_eq!(places(&harness, "zui-nav-page"), ["covered", "current"]);
}

#[test]
fn the_back_button_names_the_page_below() {
    let Stack { harness, path, .. } = mount();
    assert!(
        back_button(&harness).is_none(),
        "the root page has no back button"
    );

    harness.window.scope.with(|| path.push(Route::Group));
    run_out(&harness);
    let back = back_button(&harness).expect("the group page offers back");
    assert_eq!(harness.window.dom.tree().text_content(back), "Passwords");

    let titles: Vec<String> = harness
        .find_all("zui-nav-bar__title")
        .into_iter()
        .map(|node| harness.window.dom.tree().text_content(node))
        .collect();
    assert_eq!(titles, ["Passwords", "Bank"]);
}

#[test]
fn a_page_with_unsaved_edits_blocks_back() {
    let Stack {
        harness,
        path,
        editing,
        ..
    } = mount();
    harness.window.scope.with(|| {
        path.push(Route::Group);
        path.push(Route::Entry);
    });
    run_out(&harness);
    assert!(back_button(&harness).is_some());

    editing.set(true);
    harness.window.frame();
    assert!(
        back_button(&harness).is_none(),
        "back is gone while editing"
    );
    assert_eq!(
        harness.window.scope.with(|| path.pop()),
        Err(PopError::Blocked)
    );
    assert_eq!(
        harness.window.scope.with(|| path.pop_to_root()),
        Err(PopError::Blocked)
    );
    assert_eq!(harness.find_all("zui-nav-page").len(), 3);

    editing.set(false);
    harness.window.frame();
    harness.click(back_button(&harness).expect("back returns after the edit"));
    assert_eq!(
        harness.window.scope.with(|| path.pages()),
        [Route::Root, Route::Group]
    );
}

#[test]
fn pop_to_root_slides_out_only_the_top_page() {
    let Stack { harness, path, .. } = mount();
    harness.window.scope.with(|| {
        path.push(Route::Group);
        path.push(Route::Entry);
    });
    run_out(&harness);

    assert_eq!(harness.window.scope.with(|| path.pop_to_root()), Ok(()));
    harness.window.frame();
    assert_eq!(places(&harness, "zui-nav-page"), ["pop-in", "pop-out"]);

    run_out(&harness);
    assert_eq!(places(&harness, "zui-nav-page"), ["current"]);
    assert_eq!(
        harness.window.scope.with(|| path.pop()),
        Err(PopError::AtRoot)
    );
}

/// The width of the page area in the swipe tests, in CSS pixels.
const WIDTH: f32 = 400.0;

/// Pushes the group page and gives the page area a phone's width.
fn pushed() -> Stack {
    let stack = mount();
    stack
        .harness
        .window
        .scope
        .with(|| stack.path.push(Route::Group));
    run_out(&stack.harness);
    let pages = stack.harness.find("zui-nav-pages");
    stack.harness.window.place(pages, 0.0, 44.0, WIDTH, 800.0);
    stack.harness.window.frame();

    stack
}

/// Drags from `x` across by `travel` pixels on `node` and lets go at `velocity` pixels per second.
/// Returns whether the stack took the drag.
fn swipe(harness: &Harness, node: NodeId, x: f32, travel: f32, velocity: f32) -> bool {
    let origin = Point::<CssPx, Css>::new(CssPx(x), CssPx(300.0));
    let at = |phase: GesturePhase, travelled: f32, speed: f32| {
        let pan = PanEvent {
            phase,
            origin,
            position: Point::new(CssPx(x + travelled), CssPx(300.0)),
            velocity: Velocity { x: speed, y: 0.0 },
            kind: PointerKind::Touch,
            edge: None,
        };
        let delivered =
            harness
                .window
                .dispatcher()
                .send_to(node, EventKind::HorizontalPan, Payload::Pan(pan));
        harness.window.frame();
        delivered.default
    };
    let started = at(GesturePhase::Started, 10.0, 0.0);
    if started == DefaultAction::Prevented {
        return false;
    }
    at(GesturePhase::Moved, travel / 2.0, velocity);
    assert_eq!(
        places(harness, "zui-nav-page"),
        ["swipe-below", "swipe-top"]
    );
    at(GesturePhase::Moved, travel, velocity);
    at(GesturePhase::Ended, travel, velocity);

    true
}

#[test]
fn an_edge_swipe_past_half_the_width_goes_back() {
    let Stack { harness, path, .. } = pushed();
    let pages = harness.find("zui-nav-pages");

    assert!(swipe(&harness, pages, 4.0, 240.0, 0.0));
    assert_eq!(places(&harness, "zui-nav-page"), ["swipe-in", "swipe-out"]);
    assert_eq!(harness.window.scope.with(|| path.pages()), [Route::Root]);

    run_out(&harness);
    assert_eq!(places(&harness, "zui-nav-page"), ["current"]);
}

#[test]
fn a_fast_flick_short_of_half_the_width_goes_back() {
    let Stack { harness, path, .. } = pushed();
    let pages = harness.find("zui-nav-pages");

    assert!(swipe(&harness, pages, 4.0, 100.0, 600.0));
    assert_eq!(harness.window.scope.with(|| path.pages()), [Route::Root]);
    let stack = harness.only_child();
    let ease = harness
        .window
        .dom
        .tree()
        .custom_property(stack, CustomPropertyName::new("zui-nav-swipe-ease"));
    assert!(
        ease.is_some_and(|ease| ease.starts_with("cubic-bezier(")),
        "the page continues at the speed of the finger"
    );
}

#[test]
fn a_slow_drag_short_of_half_the_width_slides_back() {
    let Stack { harness, path, .. } = pushed();
    let pages = harness.find("zui-nav-pages");

    assert!(swipe(&harness, pages, 4.0, 100.0, 50.0));
    assert_eq!(
        places(&harness, "zui-nav-page"),
        ["swipe-cover", "swipe-back"]
    );
    assert_eq!(
        harness.window.scope.with(|| path.pages()),
        [Route::Root, Route::Group]
    );

    run_out(&harness);
    assert_eq!(places(&harness, "zui-nav-page"), ["covered", "current"]);
}

#[test]
fn the_page_area_takes_a_drag_only_from_its_leading_edge() {
    let Stack { harness, path, .. } = pushed();
    let pages = harness.find("zui-nav-pages");

    assert!(!swipe(&harness, pages, 200.0, 240.0, 0.0));
    assert_eq!(
        harness.window.scope.with(|| path.pages()),
        [Route::Root, Route::Group]
    );
}

#[test]
fn a_page_with_unsaved_edits_takes_no_swipe() {
    let Stack {
        harness,
        path,
        editing,
        ..
    } = pushed();
    harness.window.scope.with(|| path.push(Route::Entry));
    run_out(&harness);
    editing.set(true);
    harness.window.frame();
    let pages = harness.find("zui-nav-pages");

    assert!(!swipe(&harness, pages, 4.0, 240.0, 600.0));
    assert_eq!(
        places(&harness, "zui-nav-page"),
        ["covered", "covered", "current"]
    );
    assert_eq!(harness.window.scope.with(|| path.count()), 3);
}

#[test]
fn a_large_title_moves_into_the_bar_once_it_scrolls_under_it() {
    let harness = Harness::open();
    let path = harness.window.scope.with(|| NavPath::new(Route::Root));
    harness.mount(move || {
        view! {
            NavStack(path = path, page = move |route: Route| view! {
                NavPage(title = route.title(), large_title = true) {
                    text {"Bank"}
                }
            })
        }
    });
    let title = harness.find("zui-nav-bar__title");
    let content = harness.find("zui-nav-content");
    assert_eq!(harness.find_all("zui-nav-large-title").len(), 1);
    assert_eq!(harness.attribute(title, "data-hidden").as_deref(), Some(""));

    let scroll = |y: f32| {
        harness.window.dispatcher().send_to(
            content,
            EventKind::Scroll,
            Payload::Scroll(ScrollEvent {
                offset: Point::new(CssPx(0.0), CssPx(y)),
                content_size: Size::new(CssPx(WIDTH), CssPx(2000.0)),
                scrollport: Size::new(CssPx(WIDTH), CssPx(800.0)),
            }),
        );
        harness.window.frame();
    };
    scroll(60.0);
    assert_eq!(harness.attribute(title, "data-hidden"), None);
    scroll(0.0);
    assert_eq!(harness.attribute(title, "data-hidden").as_deref(), Some(""));
}

/// Sends one pan event to `node` and runs a frame.
fn pan(harness: &Harness, node: NodeId, phase: GesturePhase, travel: f32) {
    let event = PanEvent {
        phase,
        origin: Point::new(CssPx(4.0), CssPx(300.0)),
        position: Point::new(CssPx(4.0 + travel), CssPx(300.0)),
        velocity: Velocity::default(),
        kind: PointerKind::Touch,
        edge: None,
    };
    harness
        .window
        .dispatcher()
        .send_to(node, EventKind::HorizontalPan, Payload::Pan(event));
    harness.window.frame();
}

#[test]
fn the_end_of_the_slide_out_ends_a_completed_swipe() {
    let Stack { harness, .. } = pushed();
    let pages = harness.find("zui-nav-pages");
    assert!(swipe(&harness, pages, 4.0, 240.0, 0.0));
    let out = harness.find_all("zui-nav-page")[1];

    harness.window.dispatcher().send_to(
        out,
        EventKind::TransitionEnd,
        Payload::Transition(TransitionEvent {
            property: zgui::view::Ident::new("transform"),
            elapsed: Duration::from_millis(500),
            phase: TransitionPhase::Ended,
            pseudo: None,
        }),
    );
    harness.window.frame();
    assert_eq!(places(&harness, "zui-nav-page"), ["current"]);
}

#[test]
fn a_cancelled_touch_at_rest_ends_the_swipe_at_once() {
    let Stack { harness, path, .. } = pushed();
    let pages = harness.find("zui-nav-pages");

    pan(&harness, pages, GesturePhase::Started, 10.0);
    assert_eq!(
        places(&harness, "zui-nav-page"),
        ["swipe-below", "swipe-top"]
    );
    pan(&harness, pages, GesturePhase::Cancelled, 10.0);
    assert_eq!(places(&harness, "zui-nav-page"), ["covered", "current"]);
    assert_eq!(harness.window.scope.with(|| path.count()), 2);
}

#[test]
fn a_push_during_a_swipe_keeps_its_motion_when_the_finger_lifts() {
    let Stack { harness, path, .. } = pushed();
    let pages = harness.find("zui-nav-pages");

    pan(&harness, pages, GesturePhase::Started, 10.0);
    harness.window.scope.with(|| path.push(Route::Entry));
    harness.window.frame();
    pan(&harness, pages, GesturePhase::Ended, 0.0);
    assert_eq!(
        places(&harness, "zui-nav-page"),
        ["covered", "push-out", "push-in"]
    );

    // A later swipe starts.
    run_out(&harness);
    pan(&harness, pages, GesturePhase::Started, 10.0);
    assert_eq!(
        places(&harness, "zui-nav-page"),
        ["covered", "swipe-below", "swipe-top"]
    );
}
