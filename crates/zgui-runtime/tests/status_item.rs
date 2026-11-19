//! The status item: built from signals, given to the platform again when they change, picked from
//! like the application menu, and pressed.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::{Css, CssPx, Point, Rect, Size};
use zgui_platform::{
    AppEvent, MenuAction, MenuEntry, MenuId, PlatformCapabilities, PlatformCx, StatusAnchor,
    StatusItem, StatusPress, WakeReason,
};
use zgui_platform_headless::{Harness, Headless};
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::{Get, Set};
use zgui_runtime::Runtime;
use zgui_view::{Anchor, BuildCx, IntoView, View};

/// The stylesheet the window is styled by.
const SHEET: &str = "root { display: block } column { display: block }";

/// A view that builds one empty column, for a window whose contents are not the point.
fn empty(cx: &mut BuildCx<'_>) -> Box<dyn Anchor> {
    Box::new(zgui_elements::column().into_view().build(cx))
}

/// A platform that shows a status item, or one that does not.
fn platform(status_item: bool) -> Headless {
    let mut capabilities = PlatformCapabilities::none();
    capabilities.status_item = status_item;
    Headless::new().with_capabilities(capabilities)
}

/// An application whose status item shows `count`, and is removed while `hidden` is set.
///
/// What the user picks from any menu is written to `picked`.
fn application(
    platform: Headless,
    count: RwSignal<u32>,
    hidden: RwSignal<bool>,
    picked: Rc<RefCell<Vec<MenuId>>>,
) -> Harness<Runtime> {
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .with_context(move || {
            let guard =
                zgui_runtime::desktop::on_menu(move |id| picked.borrow_mut().push(id.clone()));
            core::mem::forget(guard);
        })
        .with_status_item(move || {
            if hidden.get() {
                return None;
            }
            Some(
                StatusItem::new()
                    .with_title(count.get().to_string())
                    .with(MenuAction::new("show", "Show")),
            )
        })
        .into_handler(empty)
        .expect("the reactive runtime installs");
    Harness::over(platform, handler)
}

/// An application whose status item does what `press` says on a primary click.
///
/// Where the item was each time it reported a press is written to `pressed`.
fn pressing_application(
    press: RwSignal<StatusPress>,
    pressed: Rc<RefCell<Vec<StatusAnchor>>>,
) -> Harness<Runtime> {
    let handler = zgui_runtime::App::new()
        .with_title("test")
        .with_size(400.0, 300.0)
        .with_stylesheet(SHEET)
        .with_renderer(Box::new(support::capture))
        .with_context(move || {
            let guard = zgui_runtime::desktop::on_status_item_press(move |anchor| {
                pressed.borrow_mut().push(*anchor);
            });
            core::mem::forget(guard);
        })
        .with_status_item(move || {
            Some(
                StatusItem::new()
                    .with_press(press.get())
                    .with(MenuAction::new("quit", "Quit")),
            )
        })
        .into_handler(empty)
        .expect("the reactive runtime installs");
    Harness::over(platform(true), handler)
}

/// A rectangle `width` by `height` with its top left corner at `x`, `y`.
fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect<CssPx, Css> {
    Rect::new(
        Point::new(CssPx(x), CssPx(y)),
        Size::new(CssPx(width), CssPx(height)),
    )
}

/// The titles of every status item the platform was given, with `None` for each removal.
fn titles(harness: &Harness<Runtime>) -> Vec<Option<String>> {
    harness
        .platform()
        .status_item_log()
        .into_iter()
        .map(|item| {
            item.and_then(|item| item.title)
                .map(|title| title.as_str().to_owned())
        })
        .collect()
}

#[test]
fn the_status_item_is_shown_and_shown_again_when_what_it_read_changes() {
    let count = RwSignal::new(0);
    let hidden = RwSignal::new(false);
    let mut harness = application(platform(true), count, hidden, Rc::default());
    harness.settle(4);
    assert_eq!(titles(&harness), [Some("0".to_owned())]);

    count.set(1);
    harness.settle(8);
    assert_eq!(
        titles(&harness),
        [Some("0".to_owned()), Some("1".to_owned())],
        "a signal the item was built from changed, and the platform was not given the new item"
    );

    hidden.set(true);
    harness.settle(8);
    assert_eq!(
        harness.platform().status_item_log().last(),
        Some(&None),
        "an absent item removes the item shown"
    );
}

/// The desktop gives back the id of the action the user picked from the status item's menu.
#[test]
fn an_action_picked_from_the_status_items_menu_reaches_the_menu_hook() {
    let picked = Rc::new(RefCell::new(Vec::new()));
    let mut harness = application(
        platform(true),
        RwSignal::new(0),
        RwSignal::new(false),
        Rc::clone(&picked),
    );
    harness.settle(4);

    let shown = harness
        .platform()
        .status_item_log()
        .pop()
        .flatten()
        .expect("the platform was given a status item");
    let Some(MenuEntry::Action(action)) = shown.menu.first() else {
        panic!(
            "the status item's menu starts with an action: {:?}",
            shown.menu
        );
    };
    harness
        .platform()
        .waker()
        .wake(WakeReason::App(AppEvent::Menu(action.id.clone())));
    harness.settle(4);
    assert_eq!(*picked.borrow(), [MenuId::new("show")]);
}

#[test]
fn a_desktop_with_no_status_area_is_given_no_status_item() {
    let mut harness = application(
        platform(false),
        RwSignal::new(0),
        RwSignal::new(false),
        Rc::default(),
    );
    harness.settle(4);
    assert!(harness.platform().status_item_log().is_empty());
}

#[test]
fn a_press_of_an_item_that_reports_presses_reaches_the_press_hook() {
    let press = RwSignal::new(StatusPress::Reports);
    let pressed = Rc::new(RefCell::new(Vec::new()));
    let mut harness = pressing_application(press, Rc::clone(&pressed));
    harness.settle(4);

    let anchor = StatusAnchor::new(
        rect(1200.0, 0.0, 24.0, 24.0),
        rect(0.0, 24.0, 1440.0, 876.0),
    );
    harness.platform().press_status_item(anchor);
    harness.settle(4);
    assert_eq!(*pressed.borrow(), [anchor]);

    press.set(StatusPress::OpensMenu);
    harness.settle(8);
    let shown = harness
        .platform()
        .status_item_log()
        .pop()
        .flatten()
        .expect("the platform was given a status item");
    assert_eq!(
        shown.press,
        StatusPress::OpensMenu,
        "the item was rebuilt with what a click does, and the platform was not given it"
    );
    harness.platform().press_status_item(anchor);
    harness.settle(4);
    assert_eq!(
        *pressed.borrow(),
        [anchor],
        "a press of an item that opens its menu reached the press hook"
    );
}
