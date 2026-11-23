//! A bottom sheet, driven through real frames.

mod harness;

use core::cell::Cell;
use core::time::Duration;
use std::rc::Rc;

use zgui::geom::Css;
use zgui::geom::CssPx;
use zgui::geom::DevicePx;
use zgui::geom::Point;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::reactive::UnsyncCallback;
use zgui::view;
use zgui::vocab::DefaultAction;
use zgui::vocab::EventKind;
use zgui::vocab::GesturePhase;
use zgui::vocab::NamedKey;
use zgui::vocab::PanEvent;
use zgui::vocab::Payload;
use zgui::vocab::PointerKind;
use zgui::vocab::Velocity;
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// The height of the sheet at its large detent. The window is 400 by 1000 pixels, so the medium
/// detent is 890 - 500 = 390 pixels below the large one.
const PANEL: f32 = 890.0;

/// What a test holds of a mounted sheet.
struct Sheet {
    harness: Harness,
    open: RwSignal<bool, LocalStorage>,
    blocked: RwSignal<bool, LocalStorage>,
    /// How many times the person tried to close the sheet while it was blocked.
    attempts: Rc<Cell<usize>>,
}

fn mount(detents: Vec<Detent>) -> Sheet {
    let harness = Harness::open();
    let (open, blocked) = harness
        .window
        .scope
        .with(|| (RwSignal::new_local(false), RwSignal::new_local(false)));
    let attempts = Rc::new(Cell::new(0));
    let counter = Rc::clone(&attempts);
    harness.mount(move || {
        let attempt = UnsyncCallback::new(move |()| counter.set(counter.get() + 1));
        view! {
            column {
                control(class = "outside", tabindex = {Focus::Sequential}) {"Behind"}
                BottomSheet(
                    open = open,
                    detents = detents.clone(),
                    dismiss_blocked = blocked,
                    on_dismiss_attempt = attempt,
                    label = "Generator"
                ) {
                    control(class = "first", tabindex = {Focus::Sequential}) {"Copy"}
                    control(class = "last", tabindex = {Focus::Sequential}) {"Done"}
                }
            }
        }
    });

    Sheet {
        harness,
        open,
        blocked,
        attempts,
    }
}

impl Sheet {
    /// Opens the sheet and gives it the size it has on a phone.
    fn present(&self) -> NodeId {
        // A phone-sized window, narrower than a form sheet needs.
        let root = self.harness.window.root;
        self.harness.window.place(root, 0.0, 0.0, 400.0, 1000.0);
        self.open.set(true);
        self.harness.window.frame();
        let surface = self.surface().expect("the sheet opened");
        self.harness
            .window
            .place(surface, 0.0, 1000.0 - PANEL, 400.0, PANEL);
        self.harness.window.frame();

        surface
    }

    fn surface(&self) -> Option<NodeId> {
        self.harness.all().into_iter().find(|node| {
            self.harness
                .window
                .dom
                .tree()
                .classes(*node)
                .contains(&zgui::view::ClassName::new("zui-bottom-sheet"))
        })
    }

    fn resting(&self) -> Option<String> {
        self.surface()
            .and_then(|surface| self.harness.attribute(surface, "data-detent"))
    }

    /// Drags the sheet by `travel` pixels down and lets go at `velocity` pixels per second.
    fn drag(&self, travel: f32, velocity: f32) {
        let body = self.harness.find("zui-bottom-sheet__body");
        let origin = Point::<CssPx, Css>::new(CssPx(500.0), CssPx(600.0));
        let at = |phase: GesturePhase, travelled: f32, speed: f32| {
            let pan = PanEvent {
                phase,
                origin,
                position: Point::new(CssPx(500.0), CssPx(600.0 + travelled)),
                velocity: Velocity { x: 0.0, y: speed },
                kind: PointerKind::Touch,
                edge: None,
            };
            self.harness.window.dispatcher().send_to(
                body,
                EventKind::VerticalPan,
                Payload::Pan(pan),
            );
            self.harness.window.frame();
        };
        at(GesturePhase::Started, travel.signum() * 10.0, 0.0);
        at(GesturePhase::Moved, travel / 2.0, velocity);
        at(GesturePhase::Moved, travel, velocity);
        at(GesturePhase::Ended, travel, velocity);
    }

    /// Runs the frames that the exit of a closed sheet takes.
    fn settle(&self) {
        self.harness.window.advance(Duration::from_millis(1));
    }
}

#[test]
fn a_sheet_opens_at_its_smallest_detent() {
    let sheet = mount(vec![Detent::Large, Detent::Medium]);
    assert!(sheet.surface().is_none(), "absent while closed");

    sheet.present();
    assert_eq!(sheet.resting().as_deref(), Some("medium"));
    assert_eq!(sheet.harness.find_all("zui-bottom-sheet__grabber").len(), 1);

    sheet.drag(-300.0, 0.0);
    assert_eq!(sheet.resting().as_deref(), Some("large"));
    sheet.open.set(false);
    sheet.harness.window.frame();
    sheet.settle();
    sheet.present();
    assert_eq!(
        sheet.resting().as_deref(),
        Some("medium"),
        "it opens at medium again"
    );
}

#[test]
fn a_sheet_with_one_detent_draws_no_grabber() {
    let sheet = mount(vec![Detent::Large]);
    sheet.present();
    assert!(
        sheet
            .harness
            .find_all("zui-bottom-sheet__grabber")
            .is_empty()
    );
}

#[test]
fn a_released_drag_snaps_by_where_it_ends() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();

    // From medium (390) up 300 pixels ends at 90, nearest the large detent.
    sheet.drag(-300.0, 0.0);
    assert_eq!(sheet.resting().as_deref(), Some("large"));

    // From large down 150 pixels ends nearer large than medium.
    sheet.drag(150.0, 0.0);
    assert_eq!(sheet.resting().as_deref(), Some("large"));

    // Down 250 pixels ends nearer medium.
    sheet.drag(250.0, 0.0);
    assert_eq!(sheet.resting().as_deref(), Some("medium"));
    assert!(sheet.open.get_untracked());
}

#[test]
fn a_released_drag_snaps_by_how_fast_it_moves() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();

    // A short flick up from medium carries the sheet to large.
    sheet.drag(-60.0, -900.0);
    assert_eq!(sheet.resting().as_deref(), Some("large"));

    // A short flick down from large carries it past medium and closes it.
    sheet.drag(60.0, 2500.0);
    assert!(!sheet.open.get_untracked(), "the flick closed the sheet");
}

#[test]
fn a_drag_while_the_finger_holds_the_sheet_turns_the_transition_off() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    let surface = sheet.present();
    let body = sheet.harness.find("zui-bottom-sheet__body");
    let pan = |phase: GesturePhase, y: f32| PanEvent {
        phase,
        origin: Point::new(CssPx(500.0), CssPx(600.0)),
        position: Point::new(CssPx(500.0), CssPx(y)),
        velocity: Velocity { x: 0.0, y: 0.0 },
        kind: PointerKind::Touch,
        edge: None,
    };
    for (phase, y) in [(GesturePhase::Started, 610.0), (GesturePhase::Moved, 640.0)] {
        sheet.harness.window.dispatcher().send_to(
            body,
            EventKind::VerticalPan,
            Payload::Pan(pan(phase, y)),
        );
        sheet.harness.window.frame();
    }

    assert!(sheet.harness.attribute(surface, "data-dragging").is_some());
    let offset = sheet.harness.window.dom.tree().custom_property(
        surface,
        zgui::view::CustomPropertyName::new("zui-bottom-sheet-offset"),
    );
    assert_eq!(
        offset.as_deref(),
        Some("430px"),
        "medium (390) plus 40 pixels"
    );
}

#[test]
fn a_press_on_the_dimmed_window_closes_the_sheet() {
    let sheet = mount(vec![Detent::Large]);
    sheet.present();

    sheet.harness.window.dispatcher().pointer_at(
        Point::new(DevicePx(200.0), DevicePx(40.0)),
        EventKind::PointerDown,
    );
    sheet.harness.window.frame();
    sheet.settle();
    assert!(!sheet.open.get_untracked());
    assert!(sheet.surface().is_none());
}

#[test]
fn a_blocked_sheet_stays_open_and_says_that_it_was_asked_to_close() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.blocked.set(true);
    let surface = sheet.present();

    sheet.harness.window.dispatcher().pointer_at(
        Point::new(DevicePx(200.0), DevicePx(40.0)),
        EventKind::PointerDown,
    );
    sheet.harness.window.frame();
    assert!(
        sheet.open.get_untracked(),
        "the press on the dimmed window was refused"
    );
    assert_eq!(sheet.attempts.get(), 1);

    sheet.harness.press(surface, NamedKey::Escape);
    assert!(sheet.open.get_untracked(), "escape was refused");
    assert_eq!(sheet.attempts.get(), 2);

    sheet.drag(400.0, 2500.0);
    assert!(sheet.open.get_untracked(), "the flick was refused");
    assert_eq!(sheet.attempts.get(), 3);
    assert_eq!(sheet.resting().as_deref(), Some("medium"));

    // The caller can still close it.
    sheet.open.set(false);
    sheet.harness.window.frame();
    sheet.settle();
    assert!(sheet.surface().is_none());
}

#[test]
fn an_open_sheet_traps_focus_and_escape_closes_it() {
    let sheet = mount(vec![Detent::Large]);
    assert_eq!(sheet.harness.window.host.live_focus_traps(), 0);

    let surface = sheet.present();
    assert_eq!(sheet.harness.window.host.live_focus_traps(), 1);
    let semantics = sheet.harness.semantics(surface);
    assert_eq!(semantics.role, Role::Dialog);
    assert!(semantics.flags.contains(zgui::vocab::SemanticFlags::MODAL));
    assert_eq!(semantics.label.as_deref(), Some("Generator"));

    sheet.harness.press(surface, NamedKey::Escape);
    sheet.settle();
    assert!(!sheet.open.get_untracked());
    assert_eq!(sheet.harness.window.host.live_focus_traps(), 0);
}

#[test]
fn an_upward_drag_on_a_list_grows_the_sheet_until_it_is_at_its_largest() {
    // The frame is offered every pan before a list inside the sheet. It takes an upward pan while
    // the sheet can grow, and refuses the rest so that the list scrolls.
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();
    let frame = sheet.harness.find("zui-bottom-sheet__frame");
    let offer = |travel: f32| {
        let pan = PanEvent {
            phase: GesturePhase::Started,
            origin: Point::new(CssPx(500.0), CssPx(600.0)),
            position: Point::new(CssPx(500.0), CssPx(600.0 + travel)),
            velocity: Velocity { x: 0.0, y: 0.0 },
            kind: PointerKind::Touch,
            edge: None,
        };
        let delivered = sheet.harness.window.dispatcher().send_to(
            frame,
            EventKind::VerticalPan,
            Payload::Pan(pan),
        );
        sheet.harness.window.frame();
        delivered.default
    };
    let end = || {
        for phase in [GesturePhase::Moved, GesturePhase::Ended] {
            let pan = PanEvent {
                phase,
                origin: Point::new(CssPx(500.0), CssPx(600.0)),
                position: Point::new(CssPx(500.0), CssPx(200.0)),
                velocity: Velocity { x: 0.0, y: 0.0 },
                kind: PointerKind::Touch,
                edge: None,
            };
            sheet.harness.window.dispatcher().send_to(
                frame,
                EventKind::VerticalPan,
                Payload::Pan(pan),
            );
            sheet.harness.window.frame();
        }
    };

    assert_eq!(
        offer(10.0),
        DefaultAction::Prevented,
        "a downward pan is left to the list"
    );
    assert_eq!(
        offer(-10.0),
        DefaultAction::Allowed,
        "an upward pan at medium is taken"
    );
    end();
    assert_eq!(sheet.resting().as_deref(), Some("large"));
    assert_eq!(
        offer(-10.0),
        DefaultAction::Prevented,
        "at large the list scrolls"
    );
}
