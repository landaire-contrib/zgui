//! The panel a context menu's items sit on.

use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue};
use zgui::{component, view};
use zgui_ui_primitives::{Align, Placement, Side};

use crate::context_menu::lift::{
    ContextMenuAccessory, ContextMenuPreview, LiftCx, LiftedMenuProps, Presentation,
};
use crate::menu::MenuContentProps;
use crate::overlay::OverlayState;

/// The items of a [`ContextMenu`](crate::ContextMenu), at the point it was asked for.
///
/// With a [`ContextMenuPreview`], a long press by a finger shows them below a lifted copy of the
/// region instead, with the [`ContextMenuAccessory`] above the copy.
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
///         ContextMenuContent {
///             ContextMenuPreview(slot) {text {"A row"}}
///             MenuItem {"Copy"}
///         }
///     }
/// }
/// # }
/// ```
#[component(slot_aware)]
pub fn ContextMenuContent(
    /// Where it is asked to go, before the window's edges have their say.
    ///
    /// Down and to the right of the point, which is what a context menu does — and the positioner
    /// crosses it to the other side on its own when there is no room there.
    #[prop(into, default = Signal::stored_local(Placement::new(Side::Bottom, Align::Start)))]
    placement: Signal<Placement, LocalStorage>,
    /// What a long press by a finger lifts above the window.
    #[prop(optional)]
    context_menu_preview: Option<ContextMenuPreview>,
    /// What stands above the lifted copy.
    #[prop(optional)]
    context_menu_accessory: Option<ContextMenuAccessory>,
    /// Classes merged after the menu's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: ChildrenFn,
) -> impl IntoView {
    let state = OverlayState::current().unwrap_or_else(|| OverlayState::uncontrolled(false, None));
    let Some(preview) = context_menu_preview else {
        return AnyView::new(view! {
            MenuContent(state = state, placement = placement, offset = {0.0}, {..attrs}, class = class) {
                {children.view()}
            }
        });
    };

    let lift = LiftCx::current();
    let presentation = lift.presentation;
    let area = lift.area;
    // Held rather than moved: the branch below is built again each time the presentation changes.
    let parts = StoredValue::new_local((preview, context_menu_accessory, class, attrs, children));

    AnyView::new(move || {
        let (preview, accessory, class, attrs, children) = parts.get_value();
        match presentation.get() {
            Presentation::AtPoint => AnyView::new(view! {
                MenuContent(state = state, placement = placement, offset = {0.0}, {..attrs}, class = class) {
                    {children.view()}
                }
            }),
            Presentation::Lifted => AnyView::new(view! {
                LiftedMenu(
                    state = state,
                    preview = preview,
                    accessory = accessory,
                    area = area,
                    class = class,
                    attrs = attrs
                ) {
                    {children.view()}
                }
            }),
        }
    })
}
