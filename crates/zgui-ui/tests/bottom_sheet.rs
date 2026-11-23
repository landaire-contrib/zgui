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
use zgui::view::CustomPropertyName;
use zgui::view::Ident;
use zgui::vocab::DefaultAction;
use zgui::vocab::EventKind;
use zgui::vocab::GesturePhase;
use zgui::vocab::NamedKey;
use zgui::vocab::PanEvent;
use zgui::vocab::Payload;
use zgui::vocab::PointerKind;
use zgui::vocab::TransitionEvent;
use zgui::vocab::TransitionPhase;
use zgui::vocab::ValueChange;
use zgui::vocab::ValueEvent;
use zgui::vocab::Velocity;
use zgui_ui::prelude::*;

use crate::harness::Harness;

/// The height of the sheet at its large detent. The window is 400 by 1000 pixels, so the medium
/// detent is 890 - 500 = 390 pixels below the large one.
const PANEL: f32 = 890.0;

/// How far below its large detent the sheet rests at medium.
const MEDIUM: f32 = 390.0;

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
        self.place(self.rest_offset());

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

    /// One custom property of the sheet.
    fn property(&self, name: &str) -> Option<String> {
        let surface = self.surface()?;
        self.harness
            .window
            .dom
            .tree()
            .custom_property(surface, CustomPropertyName::new(name))
    }

    /// How far below its large detent the sheet rests at its detent.
    fn rest_offset(&self) -> f32 {
        match self.resting().as_deref() {
            Some("medium") => MEDIUM,
            _ => 0.0,
        }
    }

    /// Draws the sheet `offset` pixels below its large detent, as a renderer does.
    fn place(&self, offset: f32) {
        self.place_stretched(offset, 0.0);
    }

    /// Draws the sheet `offset` pixels below its large detent and `stretch` pixels taller.
    fn place_stretched(&self, offset: f32, stretch: f32) {
        let surface = self.surface().expect("the sheet is open");
        self.harness.window.place(
            surface,
            0.0,
            1000.0 - PANEL + offset - stretch,
            400.0,
            PANEL + stretch,
        );
        self.harness.window.frame();
    }

    /// Ends the transition of `property` on the sheet.
    fn end_transition(&self, property: &str) {
        let surface = self.surface().expect("the sheet is open");
        let event = TransitionEvent {
            property: Ident::new(property),
            elapsed: Duration::from_millis(500),
            phase: TransitionPhase::Ended,
            pseudo: None,
        };
        self.harness.window.dispatcher().send_to(
            surface,
            EventKind::TransitionEnd,
            Payload::Transition(event),
        );
        self.harness.window.frame();
    }

    /// Draws an open sheet at its detent, as at the end of its settle.
    fn rest(&self) {
        if !self.open.get_untracked() {
            return;
        }
        self.place(self.rest_offset());
        self.end_transition("translate");
        self.end_transition("top");
    }

    /// Sends one event of a drag that started at 600 and has travelled `travel` pixels down.
    fn pan(&self, phase: GesturePhase, travel: f32, velocity: f32) {
        let body = self.harness.find("zui-bottom-sheet__body");
        let pan = PanEvent {
            phase,
            origin: Point::<CssPx, Css>::new(CssPx(500.0), CssPx(600.0)),
            position: Point::new(CssPx(500.0), CssPx(600.0 + travel)),
            velocity: Velocity { x: 0.0, y: velocity },
            kind: PointerKind::Touch,
            edge: None,
        };
        self.harness.window.dispatcher().send_to(
            body,
            EventKind::VerticalPan,
            Payload::Pan(pan),
        );
        self.harness.window.frame();
    }

    /// Drags the sheet by `travel` pixels down and lets go at `velocity` pixels per second.
    fn release(&self, travel: f32, velocity: f32) {
        self.pan(GesturePhase::Started, travel.signum() * 10.0, 0.0);
        self.pan(GesturePhase::Moved, travel / 2.0, velocity);
        self.pan(GesturePhase::Moved, travel, velocity);
        self.pan(GesturePhase::Ended, travel, velocity);
    }

    /// Drags the sheet, lets go, and lets it settle.
    fn drag(&self, travel: f32, velocity: f32) {
        self.release(travel, velocity);
        self.rest();
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

#[test]
fn a_drag_that_starts_while_the_sheet_settles_starts_where_the_sheet_is() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();

    // Let go of a flick up from medium, and take the sheet again before it reaches large.
    sheet.release(-100.0, -1200.0);
    assert_eq!(sheet.resting().as_deref(), Some("large"));
    sheet.place(200.0);

    sheet.pan(GesturePhase::Started, 10.0, 0.0);
    assert_eq!(
        sheet.property("zui-bottom-sheet-offset").as_deref(),
        Some("200px"),
        "the sheet stays where the settle was"
    );
    sheet.pan(GesturePhase::Moved, 30.0, 0.0);
    assert_eq!(
        sheet.property("zui-bottom-sheet-offset").as_deref(),
        Some("230px"),
        "the drag moves the sheet from there"
    );
}

#[test]
fn a_drag_while_the_keyboard_holds_the_sheet_at_large_starts_from_large() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();
    assert_eq!(sheet.resting().as_deref(), Some("medium"));

    // The keyboard draws a sheet at medium at its large detent.
    sheet.place(0.0);
    sheet.pan(GesturePhase::Started, 10.0, 0.0);
    sheet.pan(GesturePhase::Moved, 40.0, 0.0);
    assert_eq!(
        sheet.property("zui-bottom-sheet-offset").as_deref(),
        Some("40px")
    );
}

#[test]
fn the_release_curve_ends_with_the_settle() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();

    sheet.release(-100.0, -1200.0);
    let curve = sheet.property("zui-bottom-sheet-ease");
    assert!(
        curve.as_deref().is_some_and(|curve| curve.starts_with("cubic-bezier(")),
        "{curve:?}"
    );

    sheet.rest();
    assert_eq!(sheet.property("zui-bottom-sheet-ease"), None);
}

#[test]
fn a_drag_above_the_large_detent_stretches_the_sheet() {
    let sheet = mount(vec![Detent::Large]);
    sheet.present();

    sheet.pan(GesturePhase::Started, -10.0, 0.0);
    sheet.pan(GesturePhase::Moved, -200.0, 0.0);
    let stretch = sheet
        .property("zui-bottom-sheet-stretch")
        .and_then(|stretch| stretch.strip_suffix("px")?.parse::<f32>().ok())
        .expect("the sheet stretches");
    assert!(stretch > 0.0 && stretch < 200.0, "{stretch}");

    // Let go, and take the sheet again while it is still 30 pixels taller.
    sheet.pan(GesturePhase::Ended, -200.0, 0.0);
    assert_eq!(sheet.property("zui-bottom-sheet-stretch"), None);
    sheet.place_stretched(0.0, 30.0);
    sheet.pan(GesturePhase::Started, -10.0, 0.0);
    assert_eq!(
        sheet.property("zui-bottom-sheet-stretch").as_deref(),
        Some("30px"),
        "the drag starts from the stretch on the window"
    );
    sheet.pan(GesturePhase::Ended, -10.0, 0.0);

    sheet.rest();
    sheet.pan(GesturePhase::Started, 10.0, 0.0);
    assert_eq!(
        sheet.property("zui-bottom-sheet-offset").as_deref(),
        Some("0px"),
        "a settled sheet is measured at its own height"
    );
}

#[test]
fn the_grabber_is_a_control_that_moves_the_sheet() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.present();
    let grabber = sheet.harness.find("zui-bottom-sheet__grabber");
    let semantics = sheet.harness.semantics(grabber);
    assert_eq!(semantics.role, Role::Button);
    assert_eq!(semantics.label.as_deref(), Some("Sheet grabber"));
    assert_eq!(semantics.value.as_deref(), Some("Medium"));
    assert_eq!(semantics.numeric.value, Some(0.0));

    sheet.harness.press(grabber, NamedKey::Enter);
    assert_eq!(sheet.resting().as_deref(), Some("large"));
    sheet.harness.press(grabber, NamedKey::Space);
    assert_eq!(sheet.resting().as_deref(), Some("medium"));

    sheet.harness.press(grabber, NamedKey::ArrowUp);
    assert_eq!(sheet.resting().as_deref(), Some("large"));
    sheet.harness.press(grabber, NamedKey::ArrowDown);
    assert_eq!(sheet.resting().as_deref(), Some("medium"));

    sheet.harness.press(grabber, NamedKey::ArrowDown);
    assert!(!sheet.open.get_untracked(), "a collapse at medium closes it");
}

#[test]
fn a_reader_steps_the_grabber_and_a_blocked_sheet_stays_open() {
    let sheet = mount(vec![Detent::Medium, Detent::Large]);
    sheet.blocked.set(true);
    sheet.present();
    let grabber = sheet.harness.find("zui-bottom-sheet__grabber");
    let step = |value: &str| {
        sheet.harness.window.dispatcher().send_to(
            grabber,
            EventKind::Change,
            Payload::Value(ValueEvent::new(value, ValueChange::Committed)),
        );
        sheet.harness.window.frame();
    };

    step("1");
    assert_eq!(sheet.resting().as_deref(), Some("large"));
    step("0");
    assert_eq!(sheet.resting().as_deref(), Some("medium"));

    step("-1");
    assert!(sheet.open.get_untracked(), "the decrement was refused");
    assert_eq!(sheet.attempts.get(), 1);
}
