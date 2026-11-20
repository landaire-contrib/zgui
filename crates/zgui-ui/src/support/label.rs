//! Controls that a press on their label operates.

use std::cell::Cell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, RwSignal};
use zgui::view::{AttrName, HostHandle, NodeId, current_host};
use zgui::vocab::{ListenerOptions, PointerButton};
use zgui_ui_primitives::Listening;

/// The attribute a label carries while a press on it operates its control.
///
/// The label's style sheet selects on it to show a pointer cursor.
pub const OPERATES: &str = "data-operates";

/// The attribute a control carries while the pointer is over its label.
///
/// The control's style sheet selects on it together with `:hover`, so the control looks the same
/// when the pointer is over the control and when it is over its label.
pub const LABEL_HOVER: &str = "data-label-hover";

/// The attribute a label carries while its control is disabled.
///
/// The label's style sheet selects on it to fade the label and to stop it taking the pointer.
const DISABLED: &str = "data-disabled";

/// Lets a press on `label` operate `control`, and tells whether the pointer is over `label`.
///
/// A primary press on the label does what a press on the control does: it moves focus to the
/// control and clicks it, so the control's own `click` handler does the work. The release that
/// ends the press does not click the label. A press on the control itself, or on another focusable
/// element inside the label, is left to that element.
///
/// While `disabled` answers `true`, the label carries `data-disabled` in place of [`OPERATES`],
/// a press on it does nothing and the answer is `false`. With no label, nothing is attached and
/// the answer is always `false`.
///
/// A control binds the answer to [`LABEL_HOVER`]:
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::LocalStorage;
/// use zgui::{component, view};
/// use zgui_ui::support::{LABEL_HOVER, activate_on_press, operated_by_label};
///
/// /// A control that its label operates.
/// #[component]
/// fn Flip(
///     #[prop(optional)] labelled_by: Option<NodeRef>,
///     #[prop(into, default = Signal::stored_local(false))] disabled: Signal<bool, LocalStorage>,
/// ) -> impl IntoView {
///     let element = NodeRef::new();
///     let hovered = operated_by_label(element, labelled_by, move || disabled.get());
///     let own = Attrs::new().attribute(zgui::view::AttrName::new(LABEL_HOVER), move || {
///         hovered.get().then(String::new)
///     });
///     view! {
///         control(
///             node_ref = element,
///             tabindex = {Focus::Sequential},
///             on:pointer_down = activate_on_press(),
///             on:click = move |_| { /* flip it */ },
///             {..own}
///         )
///     }
/// }
/// ```
pub fn operated_by_label(
    control: NodeRef,
    label: Option<NodeRef>,
    disabled: impl Fn() -> bool + Copy + 'static,
) -> Signal<bool, LocalStorage> {
    // The engine holds `:hover` for the label, and no selector carries it from the label to the
    // control, which can sit on either side of it. So this follows the label's enter and leave
    // events, and starts again from `false` whenever the label is bound to another element.
    let over_label = RwSignal::new_local(false);
    let answer = Signal::derive_local(move || over_label.get() && !disabled());
    let Some(label) = label else {
        return answer;
    };

    let host = current_host();
    let listening = Listening::new(label, move || {
        over_label.set(false);
        let host = host.clone();
        let mut held = Vec::new();
        held.extend(label.listen(
            events::POINTER_DOWN,
            ListenerOptions::DEFAULT,
            move |ev: &mut EventCx<'_, events::PointerDown>| {
                if ev.button != Some(PointerButton::Primary)
                    || disabled()
                    || takes_its_own_press(control, label, host.as_ref(), ev.target)
                {
                    return;
                }
                let Some(node) = control.get_untracked() else {
                    return;
                };
                // The press default would take focus away, because a label cannot hold it.
                ev.prevent_default();
                ev.request_focus(node);
                ev.activate_now_on(node);
            },
        ));
        held.extend(label.listen(
            events::POINTER_ENTER,
            ListenerOptions::DEFAULT,
            move |_: &mut EventCx<'_, events::PointerEnter>| over_label.set(true),
        ));
        held.extend(label.listen(
            events::POINTER_LEAVE,
            ListenerOptions::DEFAULT,
            move |_: &mut EventCx<'_, events::PointerLeave>| over_label.set(false),
        ));
        held
    });

    let operates = AttrName::new(OPERATES);
    let faded = AttrName::new(DISABLED);
    // Only a `data-disabled` this function wrote is taken away again, so one the caller put on the
    // label stays.
    let wrote_faded = Rc::new(Cell::new(false));
    let marking = {
        let wrote_faded = Rc::clone(&wrote_faded);
        RenderEffect::new(move |_| {
            if label.get().is_none() {
                over_label.set(false);
                return;
            }
            if disabled() {
                label.remove_attribute(operates);
                label.set_attribute(faded, "");
                wrote_faded.set(true);
            } else {
                label.set_attribute(operates, "");
                if wrote_faded.replace(false) {
                    label.remove_attribute(faded);
                }
            }
        })
    };

    on_cleanup_local(move || {
        drop(marking);
        drop(listening);
        label.remove_attribute(operates);
        if wrote_faded.get() {
            label.remove_attribute(faded);
        }
    });
    answer
}

/// Whether a press on `target` belongs to the control, or to another focusable element inside the
/// label, which each answer it themselves.
fn takes_its_own_press(
    control: NodeRef,
    label: NodeRef,
    host: Option<&HostHandle>,
    target: NodeId,
) -> bool {
    control.contains(target)
        || host.is_some_and(|host| {
            label
                .focusables()
                .into_iter()
                .any(|inner| host.contains(inner, target))
        })
}
