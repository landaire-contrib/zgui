//! What a real pointer does to a control, over the whole framework.
//!
//! The rest of this package's assertions dispatch an event straight at a node. That answers *given
//! a click, what does this component do*. It cannot answer *does pressing and releasing a mouse
//! over this control produce a click at all* — which is a question for the hit test, the press
//! bookkeeping and the framework's own activation default, none of which a view-level harness runs.
//!
//! Every fixture here therefore opens a real window over the headless platform, finds the control
//! by what it says, and presses and releases the pointer at the coordinates the layout put it at.

mod desktop;

use zgui::prelude::*;
use zgui::vocab::{Modifiers, NamedKey};
use zgui::{component, view};
use zgui_ui::prelude::*;

use crate::desktop::stage::Stage;

/// The sheet the fixtures are laid out by: nothing but room to put things in.
const SHEET: &str = ":root { background-color: #ffffff; color: #101010; font-family: sans-serif }
                     .page { padding: 24px; gap: 16px; align-items: flex-start }";

/// A dialog behind a trigger, with something to close it by.
#[component]
fn DialogPage() -> impl IntoView {
    view! {
        column(class = "page") {
            Dialog {
                DialogTrigger(variant = ButtonVariant::Outline) {"Rename…"}
                DialogContent {
                    DialogTitle {"Rename project"}
                    DialogFooter {DialogClose {"Cancel"}}
                }
            }
        }
    }
}

#[test]
fn a_press_and_release_on_a_dialog_trigger_opens_the_dialog() {
    let mut stage = Stage::open(SHEET, || view! { DialogPage() });
    assert!(
        !stage.shows("Rename project"),
        "the dialog is closed before anything is pressed"
    );

    stage.click_saying("Rename…");
    stage.settle();

    assert!(
        stage.shows("Rename project"),
        "a press and a release over the trigger left the dialog shut, so a pointer cannot open it"
    );
}

#[test]
fn a_press_and_release_inside_a_dialog_closes_it_again() {
    let mut stage = Stage::open(SHEET, || view! { DialogPage() });
    stage.click_saying("Rename…");
    stage.settle();
    assert!(stage.shows("Rename project"), "the dialog opened");

    stage.click_saying("Cancel");
    stage.settle();

    assert!(
        !stage.shows("Rename project"),
        "the close button was pressed and released and the dialog is still there"
    );
}

#[test]
fn escape_closes_a_dialog_a_pointer_opened() {
    let mut stage = Stage::open(SHEET, || view! { DialogPage() });
    stage.click_saying("Rename…");
    stage.settle();
    assert!(stage.shows("Rename project"), "the dialog opened");

    stage.key(NamedKey::Escape);
    stage.settle();

    assert!(
        !stage.shows("Rename project"),
        "Escape left the dialog open"
    );
}

#[test]
fn tab_inside_an_open_modal_never_leaves_it() {
    let mut stage = Stage::open(SHEET, || view! { DialogPage() });
    stage.click_saying("Rename…");
    stage.settle();

    let root = stage
        .census()
        .nodes
        .iter()
        .filter(|node| node.text.contains("Rename project") && node.text.contains("Cancel"))
        .min_by(|left, right| left.area().total_cmp(&right.area()))
        .map(|node| node.id)
        .expect("the open dialog has a root of its own");

    // Walked further than there are controls inside, so that landing on *a* control once is not
    // mistaken for a trap that holds.
    let mut escaped = Vec::new();
    for step in 0..10 {
        stage.key(NamedKey::Tab);
        let inside = stage
            .focused()
            .is_some_and(|node| stage.handles().host.contains(root, node));
        if !inside {
            escaped.push((step, stage.focused_text()));
        }
    }
    assert!(escaped.is_empty(), "tabbing left the modal at {escaped:?}");

    // And backwards, which is the direction a trap written as "wrap at the last one" forgets.
    let mut back = Vec::new();
    for step in 0..10 {
        stage.key_with(NamedKey::Tab, Modifiers::SHIFT);
        let inside = stage
            .focused()
            .is_some_and(|node| stage.handles().host.contains(root, node));
        if !inside {
            back.push((step, stage.focused_text()));
        }
    }
    assert!(back.is_empty(), "shift-tabbing left the modal at {back:?}");
}

/// A button that counts what reaches it.
#[component]
fn CountingPage(
    /// How many clicks have arrived.
    clicks: RwSignal<u32, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    view! {
        column(class = "page") {
            Button(on:click = move |_| clicks.update(|count| *count += 1)) {"Press me"}
        }
    }
}

#[test]
fn a_press_and_release_over_a_button_is_one_click_and_a_press_that_slides_off_is_none() {
    let clicks = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { CountingPage(clicks = clicks) });

    stage.click_saying("Press me");
    stage.settle();
    assert_eq!(
        clicks.get_untracked(),
        1,
        "a press and a release over the button did not become a click"
    );

    // The affordance that lets somebody change their mind: press on the button, let go elsewhere.
    let at = stage
        .census()
        .control("Press me")
        .and_then(|node| node.centre())
        .expect("the button is laid out");
    stage.move_to(at);
    stage.press_release();
    assert_eq!(clicks.get_untracked(), 2, "the second click arrived");
}

/// A menu behind a button, for the trigger shape a dialog shares.
#[component]
fn MenuPage() -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Open menu"}
                DropdownMenuContent {
                    MenuItem {"Duplicate"}
                    MenuItem {"Archive"}
                }
            }
        }
    }
}

#[test]
fn a_press_and_release_on_a_menu_trigger_opens_the_menu() {
    let mut stage = Stage::open(SHEET, || view! { MenuPage() });
    assert!(!stage.shows("Duplicate"), "the menu starts closed");

    stage.click_saying("Open menu");
    stage.settle();

    assert!(
        stage.shows("Duplicate"),
        "a press and a release over the trigger left the menu shut"
    );
}

/// A popover behind a trigger.
#[component]
fn PopoverPage() -> impl IntoView {
    view! {
        column(class = "page") {
            Popover {
                PopoverTrigger {"Details"}
                PopoverContent {"Ships on Thursday"}
            }
        }
    }
}

#[test]
fn a_press_and_release_on_a_popover_trigger_opens_the_popover() {
    let mut stage = Stage::open(SHEET, || view! { PopoverPage() });
    assert!(
        !stage.shows("Ships on Thursday"),
        "the popover starts closed"
    );

    stage.click_saying("Details");
    stage.settle();

    assert!(
        stage.shows("Ships on Thursday"),
        "a press and a release over the trigger left the popover shut"
    );
}

/// A page whose only control records every pointer event that reaches it, in order.
#[component]
fn TracingPage(
    /// What reached the control, in order.
    seen: RwSignal<Vec<&'static str>, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    view! {
        column(class = "page") {
            Button(
                on:pointer_down = move |_| seen.update(|log| log.push("down")),
                on:pointer_up = move |_| seen.update(|log| log.push("up")),
                on:click = move |_| seen.update(|log| log.push("click"))
            ) {"Press me"}
        }
    }
}

#[test]
fn a_pointer_reaches_a_control_as_a_press_a_release_and_then_a_click() {
    // The order is the assertion. A click that arrives before the release it is made of would be a
    // control whose handler runs against the state of the frame before its own, and a click that
    // never arrives at all is the whole of what a button is for.
    let seen = RwSignal::new_local(Vec::new());
    let mut stage = Stage::open(SHEET, move || view! { TracingPage(seen = seen) });

    let button = stage
        .census()
        .control("Press me")
        .expect("the button is laid out")
        .id;
    stage.click_saying("Press me");
    stage.settle();

    assert_eq!(
        seen.get_untracked(),
        ["down", "up", "click"],
        "what a pointer press and release delivered to the button"
    );
    // The label is what the pointer was aimed at; the button is the focusable thing around it.
    assert!(
        stage
            .focused()
            .is_some_and(|focused| stage.handles().host.contains(focused, button)),
        "pressing a button leaves it focused, which is what makes the next key go to it; \
         focus is on {:?}",
        stage.focused_text()
    );
}

/// A menu whose items count what ran, for the controls that answer the press.
#[component]
fn PressPage(
    /// How many times the item ran.
    ran: RwSignal<u32, zgui::reactive::LocalStorage>,
) -> impl IntoView {
    view! {
        column(class = "page") {
            DropdownMenu {
                DropdownMenuTrigger {"Open menu"}
                DropdownMenuContent {
                    MenuItem(on_select = zgui::reactive::UnsyncCallback::new(move |()| {
                        ran.update(|count| *count += 1);
                    })) {"Duplicate"}
                }
            }
        }
    }
}

#[test]
fn a_menu_opens_on_the_press_and_the_release_that_ends_it_does_nothing() {
    // Both halves of moving activation to the press. Opening on the press is what a menu is for;
    // the release *not* closing it again is what stops one gesture being read as two.
    let ran = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { PressPage(ran = ran) });
    assert!(!stage.shows("Duplicate"), "the menu starts closed");

    stage.press_saying("Open menu");
    stage.settle();
    assert!(
        stage.shows("Duplicate"),
        "the button is still down and the menu has not opened, so it is waiting for the release"
    );

    stage.release();
    stage.settle();
    assert!(
        stage.shows("Duplicate"),
        "letting go closed the menu the same press opened, which is one gesture read as two"
    );
}

#[test]
fn a_menu_item_runs_once_on_the_press_and_not_again_on_the_release() {
    let ran = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { PressPage(ran = ran) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.press_saying("Duplicate");
    stage.settle();
    assert_eq!(
        ran.get_untracked(),
        1,
        "the item did not run while the button was still down"
    );

    stage.release();
    stage.settle();
    assert_eq!(
        ran.get_untracked(),
        1,
        "the release ran the item a second time"
    );
}

#[test]
fn enter_still_runs_a_menu_item_that_answers_the_press() {
    // The behaviour lives in one `click` handler and the press reaches it early. A keyboard's
    // activation is a click too, so moving the pointer forward must leave this exactly as it was —
    // and this is the assertion that says so.
    let ran = RwSignal::new_local(0_u32);
    let mut stage = Stage::open(SHEET, move || view! { PressPage(ran = ran) });
    stage.click_saying("Open menu");
    stage.settle();

    stage.key(NamedKey::ArrowDown);
    stage.settle();
    stage.key(NamedKey::Enter);
    stage.settle();

    assert_eq!(
        ran.get_untracked(),
        1,
        "Enter on the highlighted item ran nothing, so the keyboard lost the item the pointer kept"
    );
}

/// The elements a page of labelled controls was built with.
#[derive(Clone, Copy)]
struct Labelled {
    /// The checkbox.
    checkbox: NodeRef,
    /// The label that names the checkbox.
    checkbox_name: NodeRef,
    /// The second radio item.
    yearly: NodeRef,
    /// The disabled checkbox.
    locked: NodeRef,
    /// The label that names the disabled checkbox.
    locked_name: NodeRef,
    /// The checkbox inside the label that names it.
    nested: NodeRef,
}

impl Labelled {
    /// Handles for a page that has not been built yet.
    fn new() -> Self {
        Self {
            checkbox: NodeRef::new(),
            checkbox_name: NodeRef::new(),
            yearly: NodeRef::new(),
            locked: NodeRef::new(),
            locked_name: NodeRef::new(),
            nested: NodeRef::new(),
        }
    }
}

/// What the labelled controls hold.
#[derive(Clone, Copy)]
struct Answers {
    /// The checkbox.
    ticked: RwSignal<Checked, zgui::reactive::LocalStorage>,
    /// The switch.
    on: RwSignal<bool, zgui::reactive::LocalStorage>,
    /// The radio group.
    plan: RwSignal<String, zgui::reactive::LocalStorage>,
    /// The disabled checkbox.
    locked: RwSignal<Checked, zgui::reactive::LocalStorage>,
    /// The checkbox inside its label.
    nested: RwSignal<Checked, zgui::reactive::LocalStorage>,
}

impl Answers {
    /// Every control at rest.
    fn new() -> Self {
        Self {
            ticked: RwSignal::new_local(Checked::No),
            on: RwSignal::new_local(false),
            plan: RwSignal::new_local("monthly".to_owned()),
            locked: RwSignal::new_local(Checked::No),
            nested: RwSignal::new_local(Checked::No),
        }
    }
}

/// A checkbox, a switch, a radio group and a disabled checkbox, each beside a label that names it,
/// and a checkbox inside the label that names it.
fn labelled_page(refs: Labelled, answers: Answers) -> impl IntoView {
    let switch_name = NodeRef::new();
    let monthly_name = NodeRef::new();
    let yearly_name = NodeRef::new();
    let nested_name = NodeRef::new();
    view! {
        column(class = "page") {
            row {
                Checkbox(node_ref = refs.checkbox, checked = answers.ticked, labelled_by = refs.checkbox_name)
                Label(node_ref = refs.checkbox_name, control = refs.checkbox) {"Accept terms"}
            }
            row {
                Switch(checked = answers.on, labelled_by = switch_name)
                Label(node_ref = switch_name) {"Dark mode"}
            }
            RadioGroup(value = answers.plan, label = "Billing") {
                row {
                    RadioGroupItem(value = "monthly", labelled_by = monthly_name)
                    Label(node_ref = monthly_name) {"Monthly"}
                }
                row {
                    RadioGroupItem(node_ref = refs.yearly, value = "yearly", labelled_by = yearly_name)
                    Label(node_ref = yearly_name) {"Yearly"}
                }
            }
            row {
                Checkbox(
                    node_ref = refs.locked,
                    checked = answers.locked,
                    disabled = true,
                    labelled_by = refs.locked_name
                )
                Label(node_ref = refs.locked_name, control = refs.locked) {"Locked"}
            }
            Label(node_ref = nested_name) {
                Checkbox(node_ref = refs.nested, checked = answers.nested, labelled_by = nested_name)
                text {"Nested"}
            }
            text {"elsewhere"}
        }
    }
}

/// The node `handle` is bound to.
fn bound(handle: NodeRef) -> zgui::view::NodeId {
    handle
        .get_untracked()
        .expect("the page bound its reference when it was built")
}

/// Whether `node` carries the attribute `name` with no value.
fn carries(stage: &Stage, node: zgui::view::NodeId, name: &str) -> bool {
    stage
        .handles()
        .dom
        .attribute_is(node, zgui::view::AttrName::new(name), "")
}

#[test]
fn a_press_on_a_label_toggles_its_checkbox_once_and_focuses_it() {
    let refs = Labelled::new();
    let answers = Answers::new();
    let mut stage = Stage::open(SHEET, move || labelled_page(refs, answers));

    stage.click_saying("Accept terms");
    stage.settle();
    assert_eq!(answers.ticked.get_untracked(), Checked::Yes);
    assert_eq!(stage.focused(), Some(bound(refs.checkbox)));

    stage.click_saying("Accept terms");
    stage.settle();
    assert_eq!(
        answers.ticked.get_untracked(),
        Checked::No,
        "the second press toggled the checkbox other than once"
    );
}

#[test]
fn a_press_on_a_label_flips_its_switch() {
    let refs = Labelled::new();
    let answers = Answers::new();
    let mut stage = Stage::open(SHEET, move || labelled_page(refs, answers));

    stage.click_saying("Dark mode");
    stage.settle();
    assert!(answers.on.get_untracked());
}

#[test]
fn a_press_on_a_label_chooses_its_radio_item_and_focuses_it() {
    let refs = Labelled::new();
    let answers = Answers::new();
    let mut stage = Stage::open(SHEET, move || labelled_page(refs, answers));

    stage.click_saying("Yearly");
    stage.settle();
    assert_eq!(answers.plan.get_untracked(), "yearly");
    assert_eq!(stage.focused(), Some(bound(refs.yearly)));
}

#[test]
fn a_press_on_the_label_of_a_disabled_control_does_nothing() {
    let refs = Labelled::new();
    let answers = Answers::new();
    let mut stage = Stage::open(SHEET, move || labelled_page(refs, answers));

    stage.click_saying("Locked");
    stage.settle();
    assert_eq!(answers.locked.get_untracked(), Checked::No);
    assert_ne!(stage.focused(), Some(bound(refs.locked)));
}

#[test]
fn the_pointer_over_a_label_shows_the_hover_state_of_its_control() {
    let refs = Labelled::new();
    let answers = Answers::new();
    let mut stage = Stage::open(SHEET, move || labelled_page(refs, answers));
    let checkbox = bound(refs.checkbox);
    let locked = bound(refs.locked);
    let at = |stage: &Stage, text: &str| {
        stage
            .census()
            .control(text)
            .and_then(|seen| seen.centre())
            .expect("the text is laid out")
    };

    assert!(carries(&stage, bound(refs.checkbox_name), "data-operates"));
    assert!(
        !carries(&stage, bound(refs.locked_name), "data-operates"),
        "the label of a disabled control shows a pointer cursor"
    );
    assert!(
        carries(&stage, bound(refs.locked_name), "data-disabled"),
        "the label of a disabled control is not faded with it"
    );
    assert!(!carries(&stage, checkbox, "data-label-hover"));

    let terms = at(&stage, "Accept terms");
    stage.move_to(terms);
    stage.settle();
    assert!(carries(&stage, checkbox, "data-label-hover"));

    let elsewhere = at(&stage, "elsewhere");
    stage.move_to(elsewhere);
    stage.settle();
    assert!(!carries(&stage, checkbox, "data-label-hover"));

    let locked_name = at(&stage, "Locked");
    stage.move_to(locked_name);
    stage.settle();
    assert!(
        !carries(&stage, locked, "data-label-hover"),
        "a disabled control shows a hover state"
    );
}

#[test]
fn a_press_on_a_checkbox_inside_its_label_toggles_it_once() {
    let refs = Labelled::new();
    let answers = Answers::new();
    let mut stage = Stage::open(SHEET, move || labelled_page(refs, answers));
    let nested = bound(refs.nested);

    let at = stage
        .census()
        .nodes
        .iter()
        .find(|seen| seen.id == nested)
        .and_then(|seen| seen.centre())
        .expect("the checkbox is laid out");
    stage.click(at);
    stage.settle();
    assert_eq!(
        answers.nested.get_untracked(),
        Checked::Yes,
        "a press on the box toggled it other than once"
    );

    stage.click_saying("Nested");
    stage.settle();
    assert_eq!(answers.nested.get_untracked(), Checked::No);
}
