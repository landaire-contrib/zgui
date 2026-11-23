//! A page with the `inert` attribute, through the whole loop.
//!
//! A navigation stack sets `inert` on each page below the top page, and a page can become inert
//! while a field in it has focus. A click or a tap on an inert page goes to what is below it, tab
//! does not stop in it, and a focused field in it loses focus and the keys that follow.

mod support;

use std::cell::RefCell;
use std::rc::Rc;

use zgui_geom::CssPx;
use zgui_geom::Point;
use zgui_platform::SurfaceEvent;
use zgui_reactive::RwSignal;
use zgui_reactive::prelude::Get;
use zgui_reactive::prelude::GetUntracked;
use zgui_reactive::prelude::Set;
use zgui_view::AttrName;
use zgui_view::BuildCx;
use zgui_view::IntoView;
use zgui_view::NodeId;
use zgui_view::NodeRef;
use zgui_view::View;
use zgui_view::ViewHost;
use zgui_view::events;
use zgui_vocab::KeyCode;
use zgui_vocab::KeyEvent;
use zgui_vocab::KeyState;
use zgui_vocab::Modifiers;
use zgui_vocab::NamedKey;
use zgui_vocab::PhysicalKey;
use zgui_vocab::PointerAction;
use zgui_vocab::PointerButton;
use zgui_vocab::PointerEvent;
use zgui_vocab::PointerId;
use zgui_vocab::PointerKind;
use zgui_vocab::Timestamp;

/// A control in the flow, a page over it, and a control after the page.
const CSS: &str = ".root { display: block; position: relative; width: 400px; height: 300px }
                   .under { display: block; width: 200px; height: 100px }
                   .page { display: block; position: absolute; left: 0; top: 0;
                           width: 400px; height: 300px }
                   .button { display: block; width: 200px; height: 100px }
                   .entry { display: block; width: 200px; height: 40px }
                   .after { display: block; width: 100px; height: 24px }";

/// The middle of the page's button, which is also over the control below the page.
const OVER_BUTTON: Point<CssPx, zgui_geom::Css> = Point::new(CssPx(100.0), CssPx(50.0));

/// What the listeners heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Heard {
    /// The control below the page was clicked.
    UnderClicked,
    /// The page's button was clicked.
    ButtonClicked,
    /// The page's field received a key.
    EntryKey,
}

/// One window holding the page.
struct Script {
    /// The window being driven.
    harness: zgui_platform_headless::Harness<zgui_runtime::Runtime>,
    /// Whether the page is inert.
    inert: RwSignal<bool, zgui_reactive::LocalStorage>,
    /// The control below the page.
    under: NodeRef,
    /// The page's button.
    button: NodeRef,
    /// The page's field.
    entry: NodeRef,
    /// The control after the page.
    after: NodeRef,
    /// What the listeners heard, in order.
    heard: Rc<RefCell<Vec<Heard>>>,
}

impl Script {
    /// A window whose page starts inert when `inert` is set.
    fn new(inert: bool) -> Self {
        let inert = RwSignal::new_local(inert);
        let (under, button, entry, after) = (
            NodeRef::new(),
            NodeRef::new(),
            NodeRef::new(),
            NodeRef::new(),
        );
        let heard: Rc<RefCell<Vec<Heard>>> = Rc::new(RefCell::new(Vec::new()));

        let recorded = Rc::clone(&heard);
        let mut harness = support::app_with_text(CSS, move |cx: &mut BuildCx<'_>| {
            let clicked_under = Rc::clone(&recorded);
            let clicked_button = Rc::clone(&recorded);
            let keyed = Rc::clone(&recorded);
            Box::new(
                zgui_elements::column()
                    .class("root")
                    .child(zgui_elements::control().class("under").node_ref(under).on(
                        events::CLICK,
                        move |_| {
                            clicked_under.borrow_mut().push(Heard::UnderClicked);
                        },
                    ))
                    .child(
                        zgui_elements::column()
                            .class("page")
                            .attribute(AttrName::new("inert"), move || {
                                inert.get().then(String::new)
                            })
                            .child(
                                zgui_elements::control()
                                    .class("button")
                                    .node_ref(button)
                                    .on(events::CLICK, move |_| {
                                        clicked_button.borrow_mut().push(Heard::ButtonClicked);
                                    }),
                            )
                            .child(
                                zgui_elements::field()
                                    .class("entry")
                                    .node_ref(entry)
                                    .on(events::KEY_DOWN, move |_| {
                                        keyed.borrow_mut().push(Heard::EntryKey);
                                    })
                                    .child("ab"),
                            ),
                    )
                    .child(zgui_elements::control().class("after").node_ref(after))
                    .into_view()
                    .build(cx),
            )
        });
        harness.settle(8);

        Self {
            harness,
            inert,
            under,
            button,
            entry,
            after,
            heard,
        }
    }

    /// Delivers one surface event and lets the frames it produced settle.
    fn deliver(&mut self, event: SurfaceEvent) {
        self.harness.deliver_to_first(event);
        self.harness.settle(8);
    }

    /// Presses and releases the mouse button at `at`.
    fn click(&mut self, at: Point<CssPx, zgui_geom::Css>) {
        for action in [
            PointerAction::Moved,
            PointerAction::Pressed,
            PointerAction::Released,
        ] {
            self.deliver(SurfaceEvent::Pointer {
                action,
                event: PointerEvent::mouse(at),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
        }
    }

    /// Puts a finger down at `at` and lifts it.
    fn tap(&mut self, at: Point<CssPx, zgui_geom::Css>) {
        for action in [PointerAction::Pressed, PointerAction::Released] {
            self.deliver(SurfaceEvent::Pointer {
                action,
                event: PointerEvent {
                    id: PointerId::new(1),
                    kind: PointerKind::Touch,
                    primary: true,
                    position: at,
                    button: Some(PointerButton::Primary),
                    pressure: None,
                },
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
        }
    }

    /// Presses and releases one key.
    fn key(&mut self, event: KeyEvent) {
        for state in [KeyState::Pressed, KeyState::Released] {
            self.deliver(SurfaceEvent::Key {
                state,
                event: event.clone(),
                modifiers: Modifiers::NONE,
                timestamp: Timestamp::ORIGIN,
            });
        }
    }

    /// Presses tab.
    fn tab(&mut self) {
        self.deliver(SurfaceEvent::Key {
            state: KeyState::Pressed,
            event: KeyEvent::named(NamedKey::Tab, PhysicalKey::Code(KeyCode::Tab)),
            modifiers: Modifiers::NONE,
            timestamp: Timestamp::ORIGIN,
        });
    }

    /// Sets or removes `inert` on the page.
    fn set_inert(&mut self, inert: bool) {
        self.inert.set(inert);
        self.harness.settle(8);
    }

    /// Which node holds focus, as the framework reports it to a view.
    fn focused(&self) -> Option<NodeId> {
        self.harness
            .app()
            .windows()
            .first()
            .expect("a window")
            .host()
            .focused()
            .get_untracked()
    }

    /// Whether the surface was last told that text input is on.
    fn text_input_on(&self) -> bool {
        self.harness
            .platform()
            .offscreens()
            .first()
            .expect("a surface was created")
            .last_text_input()
            .flatten()
            .is_some()
    }

    /// Everything the listeners heard since the last call.
    fn heard(&self) -> Vec<Heard> {
        core::mem::take(&mut self.heard.borrow_mut())
    }
}

#[test]
fn a_click_or_a_tap_on_an_inert_page_reaches_the_control_below_it() {
    let mut script = Script::new(true);

    script.click(OVER_BUTTON);
    assert_eq!(script.heard(), vec![Heard::UnderClicked]);
    script.tap(OVER_BUTTON);
    assert_eq!(script.heard(), vec![Heard::UnderClicked]);

    script.set_inert(false);
    script.click(OVER_BUTTON);
    assert_eq!(script.heard(), vec![Heard::ButtonClicked]);
    script.tap(OVER_BUTTON);
    assert_eq!(script.heard(), vec![Heard::ButtonClicked]);
}

#[test]
fn tab_skips_the_controls_of_an_inert_page() {
    let mut script = Script::new(true);
    script.tab();
    assert_eq!(script.focused(), script.under.get_untracked());
    script.tab();
    assert_eq!(script.focused(), script.after.get_untracked());

    let mut script = Script::new(false);
    script.tab();
    assert_eq!(script.focused(), script.under.get_untracked());
    script.tab();
    assert_eq!(script.focused(), script.button.get_untracked());
}

#[test]
fn a_focused_field_that_becomes_inert_loses_focus_and_its_keys() {
    let mut script = Script::new(false);
    script.entry.focus();
    script.harness.settle(8);
    assert_eq!(script.focused(), script.entry.get_untracked());
    assert!(script.text_input_on(), "the field asked for the keyboard");
    script.key(KeyEvent::character("x"));
    assert_eq!(script.heard(), vec![Heard::EntryKey]);

    script.set_inert(true);
    assert_eq!(script.focused(), None);
    assert!(!script.text_input_on(), "the keyboard stayed up");
    script.key(KeyEvent::character("x"));
    assert_eq!(script.heard(), Vec::<Heard>::new());

    script.entry.focus();
    script.harness.settle(8);
    assert_eq!(script.focused(), None, "an inert field took focus");
}
