//! A navigation stack, driven through real frames.

mod harness;

use core::cell::Cell;
use core::time::Duration;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::view;
use zgui::vocab::AnimationEvent;
use zgui::vocab::AnimationPhase;
use zgui::vocab::EventKind;
use zgui::vocab::Payload;
use zgui::vocab::SemanticFlags;
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
