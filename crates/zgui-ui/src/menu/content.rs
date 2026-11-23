//! The surface a menu's items sit on.

use zgui::geom::{Css, CssPx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, StoredValue};
use zgui::view::ClassName;
use zgui::{component, view};
use zgui_ui_primitives::prelude::*;

use crate::menu::SHEET;
use crate::menu::keys::MenuTypeaheadProps;
use crate::menu::style::MenuStyle;
use crate::overlay::{AnchoredSurfaceProps, OverlayState};

/// The panel the calling item sits on, published to the items by [`MenuContent`].
///
/// An item that hands its focus back when the pointer leaves it needs somewhere to put it, and the
/// somewhere is the panel under it — the *nearest* one, so that an item three submenus deep blurs
/// onto the submenu it actually sits on. [`MenuContext`](crate::menu::MenuContext) cannot answer
/// that: it deliberately names the outermost menu only, because that is the one a chosen item
/// closes.
#[derive(Copy, Clone)]
pub(crate) struct MenuSurface {
    /// The panel's element.
    surface: NodeRef,
    /// How far the pointer has moved over the items since the panel opened.
    travel: StoredValue<Travel, LocalStorage>,
}

/// How far the pointer has moved over a panel's items since the panel opened.
///
/// A panel can open under a pointer that is still down, as after a long press. Lifting it where it
/// was chooses nothing, so a release chooses an item only after a press on the panel or a move
/// past [`TRAVEL`]. A release after the panel scrolled chooses nothing, because the finger that
/// scrolls a long menu lifts over an item that it did not aim at.
#[derive(Copy, Clone, Debug, PartialEq)]
enum Travel {
    /// No pointer event has reached an item yet.
    Unseen,
    /// The pointer was first seen over an item at this point and has stayed near it.
    From(Point<CssPx, Css>),
    /// The pointer was pressed on the panel or moved past [`TRAVEL`].
    Moved,
    /// The panel scrolled since the last press.
    Scrolled,
}

/// How far a pointer must move, in CSS pixels, before its release can choose an item. Equal to the
/// slop of a touch tap.
const TRAVEL: f32 = 10.0;

impl MenuSurface {
    /// The panel the calling scope's items sit on, when it is inside a menu.
    pub(crate) fn current() -> Option<Self> {
        use_local_context::<Self>()
    }

    /// The panel's element.
    pub(crate) fn node(self) -> NodeRef {
        self.surface
    }

    /// Records a press on one of the panel's items.
    pub(crate) fn pressed(self) {
        self.travel.set_value(Travel::Moved);
    }

    /// Records a move over one of the panel's items, and tells whether the pointer has moved far
    /// enough for its release to choose an item.
    pub(crate) fn moved(self, at: Point<CssPx, Css>) -> bool {
        let next = match self.travel.get_value() {
            Travel::Unseen => Travel::From(at),
            Travel::From(from) => {
                let dx = at.x.0 - from.x.0;
                let dy = at.y.0 - from.y.0;
                if dx * dx + dy * dy > TRAVEL * TRAVEL {
                    Travel::Moved
                } else {
                    Travel::From(from)
                }
            }
            settled @ (Travel::Moved | Travel::Scrolled) => settled,
        };
        self.travel.set_value(next);
        next == Travel::Moved
    }

    /// Records that the panel scrolled.
    fn scrolled(self) {
        self.travel.set_value(Travel::Scrolled);
    }

    /// Whether a release can choose an item now.
    pub(crate) fn may_choose(self) -> bool {
        self.travel.get_value() == Travel::Moved
    }
}

/// The panel a menu's items are laid out on, with the whole keyboard model over it.
///
/// Used directly by [`DropdownMenuContent`](crate::DropdownMenuContent),
/// [`ContextMenuContent`](crate::ContextMenuContent) and
/// [`MenuSubContent`](crate::MenuSubContent), which differ only in where they are anchored.
///
/// ```
/// # use zgui::prelude::*;
/// # use zgui::{component, view};
/// # use zgui_ui::prelude::*;
/// # #[component]
/// # fn Example() -> impl IntoView {
/// view! {
///     DropdownMenu {
///         DropdownMenuTrigger {"Actions"}
///         DropdownMenuContent {MenuItem {"Open"}}
///     }
/// }
/// # }
/// ```
///
/// # Keyboard
///
/// One tab stop for the whole menu. <kbd>↑</kbd> and <kbd>↓</kbd> move between items and wrap at
/// the ends; <kbd>Home</kbd> and <kbd>End</kbd> jump; typing a letter moves to the next item that
/// reads as beginning with it; <kbd>Enter</kbd> and <kbd>Space</kbd> choose; <kbd>Escape</kbd>
/// closes. The left and right arrows are deliberately left alone here, because they belong to a
/// submenu or to a menubar around it.
#[component]
pub fn MenuContent(
    /// The surface this is the content of.
    state: OverlayState,
    /// Where it is asked to go, before the window's edges have their say.
    #[prop(into, default = Signal::stored_local(Placement::BOTTOM))]
    placement: Signal<Placement, LocalStorage>,
    /// How far off the trigger it sits, in pixels.
    #[prop(default = 4.0)]
    offset: f32,
    /// Whether a press past it closes it.
    #[prop(into, default = Signal::stored_local(true))]
    dismiss_on_outside_press: Signal<bool, LocalStorage>,
    /// Classes merged after the menu's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the panel.
    #[prop(attrs)]
    attrs: Attrs,
    /// The items.
    children: ChildrenFn,
) -> impl IntoView {
    install_stylesheet(SHEET, MenuStyle::CSS);
    // Items blur themselves onto the panel they sit on when the pointer leaves them, and this is
    // how they find it. Published per surface rather than per menu, so an item inside a submenu
    // reaches the submenu's own panel and not the outermost one.
    let travel = StoredValue::new_local(Travel::Unseen);
    let surface = MenuSurface {
        surface: state.content(),
        travel,
    };
    provide_local_context(surface);
    // Each opening starts again, because the pointer that opened it may still be down.
    let opened = RenderEffect::new(move |_| {
        if state.is_open() {
            travel.set_value(Travel::Unseen);
        }
    });
    on_cleanup_local(move || drop(opened));
    let own = Attrs::new()
        .class_toggle(ClassName::new(MenuStyle::CLASS), true)
        .class_toggle(ClassName::new("zui-menu"), true)
        .listener(
            events::SCROLL,
            zgui::vocab::ListenerOptions::DEFAULT,
            move |_: &mut EventCx<'_, events::Scroll>| surface.scrolled(),
        );
    // Held rather than moved: a menu is rebuilt every time it re-opens, and children a closure
    // moved out of on the first open are children the second open does not have.
    let children = StoredValue::new_local(children);

    view! {
        AnchoredSurface(
            state = state,
            placement = placement,
            offset = offset,
            role = {Role::Menu},
            // Confined, self-focusing and restoring: a menu opened from a button takes the caret
            // and gives it back to that button, which is where a user expects to be afterwards.
            trap = {FocusTrapOptions::MODAL},
            dismiss_on_outside_press = dismiss_on_outside_press,
            {..own},
            {..attrs},
            class = class
        ) {
            // The list is a behaviour rather than a box: it is `display: contents`, so the items
            // are laid out by the panel and a reader meets them as the menu's own children.
            RovingFocus(
                orientation = {Orientation::Vertical},
                class = "zui-menu__list"
            ) {
                MenuTypeahead {{children.get_value().view()}}
            }
        }
    }
}
