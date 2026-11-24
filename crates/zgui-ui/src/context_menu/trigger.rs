//! The region a context menu is asked for over.

use zgui::prelude::*;
use zgui::reactive::{RwSignal, StoredValue};
use zgui::vocab::{HasPopup, PointerButton, PointerKind};
use zgui::{component, view};

use crate::context_menu::SHEET;
use crate::context_menu::lift::{LiftCx, Presentation};
use crate::context_menu::place::ContextMenuPlace;
use crate::context_menu::style::ContextMenuStyle;
use crate::overlay::OverlayState;

/// The region a [`ContextMenu`](crate::ContextMenu) is opened over.
///
/// It renders a wrapper and, inside it, a point of zero size that the menu is anchored to. The
/// point is what makes the menu appear *where the pointer asked*: anchoring to the region itself
/// would put a menu asked for in the middle of a wide table against that table's edge.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     ContextMenu {
///         ContextMenuTrigger {text {"A row"}}
///         ContextMenuContent {MenuItem {"Copy"}}
///     }
/// }
/// # }
/// ```
#[component]
pub fn ContextMenuTrigger(
    /// Classes merged after its own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
    /// What the menu is about.
    children: Children,
) -> impl IntoView {
    install_stylesheet(SHEET, ContextMenuStyle::CSS);
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let lift = LiftCx::current();
    let area = lift.area;
    // Where in the region the pointer asked, in CSS pixels from its top-left corner. Two numbers
    // rather than a point, because they are bound to two style properties.
    let left = RwSignal::new_local(0.0f32);
    let top = RwSignal::new_local(0.0f32);

    let place = ContextMenuPlace {
        area,
        left,
        top,
        state,
        presentation: lift.presentation,
    };
    provide_local_context(place);
    // Whether a finger is down on the region. A request while one is down is its long press,
    // which lifts the region. The request event carries no pointer kind, so the press says it.
    // The request clears it, because the release of a press that opened the menu goes to the
    // menu's layer and never reaches the region.
    let finger = StoredValue::new_local(false);

    let on_context_menu = handler(
        events::CONTEXT_MENU,
        move |ev: &mut EventCx<'_, events::ContextMenu>| {
            ev.prevent_default();
            let by_finger = finger.get_value();
            finger.set_value(false);
            let presentation = if by_finger {
                Presentation::Lifted
            } else {
                Presentation::AtPoint
            };
            place.open(ev.position, presentation);
        },
    );
    // The secondary button as well as the request event, because a request is what a long press
    // produces and a right-click on this platform does not: a region that only answered the
    // request would have no context menu on a mouse at all.
    let on_pointer_down = handler(
        events::POINTER_DOWN,
        move |ev: &mut EventCx<'_, events::PointerDown>| {
            finger.set_value((**ev).kind == PointerKind::Touch);
            if ev.button == Some(PointerButton::Secondary) {
                ev.prevent_default();
                place.open_at(ev.position);
            }
        },
    );
    let on_pointer_up = handler(
        events::POINTER_UP,
        move |_: &mut EventCx<'_, events::PointerUp>| finger.set_value(false),
    );
    let on_pointer_cancel = handler(
        events::POINTER_CANCEL,
        move |_: &mut EventCx<'_, events::PointerCancel>| finger.set_value(false),
    );

    let own = state.trigger_attrs(HasPopup::Menu);

    view! {
        box(
            node_ref = area,
            class = {ContextMenuStyle::CLASS},
            class = "zui-context-menu__area",
            on:context_menu = on_context_menu,
            on:pointer_down = on_pointer_down,
            on:pointer_up = on_pointer_up,
            on:pointer_cancel = on_pointer_cancel,
            {..own},
            {..attrs},
            class = class
        ) {
            {children.into_view_once()}
            box(
                node_ref = {state.trigger()},
                class = "zui-context-menu__anchor",
                style:left = move || Some(format!("{}px", left.get())),
                style:top = move || Some(format!("{}px", top.get())),
                a11y:hidden = true
            )
        }
    }
}
