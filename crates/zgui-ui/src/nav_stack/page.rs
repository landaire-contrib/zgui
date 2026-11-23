//! What one page of a navigation stack declares about itself.

use zgui::component;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::slot;
use zgui::view;

use crate::nav_stack::NAV_NAME;
use crate::nav_stack::style::NavStackStyle;

/// What the nav bar shows for one page.
#[derive(Copy, Clone)]
pub(crate) struct PageBar {
    /// The title of the page.
    title: RwSignal<Option<Signal<String, LocalStorage>>, LocalStorage>,
    /// The controls at the leading end of the bar, after the back button.
    leading: RwSignal<Option<ChildrenFn>, LocalStorage>,
    /// The controls at the trailing end.
    trailing: RwSignal<Option<ChildrenFn>, LocalStorage>,
    /// Whether the page blocks back.
    back_blocked: RwSignal<Option<Signal<bool, LocalStorage>>, LocalStorage>,
}

impl PageBar {
    pub(crate) fn new() -> Self {
        Self {
            title: RwSignal::new_local(None),
            leading: RwSignal::new_local(None),
            trailing: RwSignal::new_local(None),
            back_blocked: RwSignal::new_local(None),
        }
    }

    /// The title, subscribing to it.
    pub(crate) fn title(self) -> String {
        self.title
            .get()
            .map(|title| title.get())
            .unwrap_or_default()
    }

    /// The leading controls, subscribing to them.
    pub(crate) fn leading(self) -> Option<ChildrenFn> {
        self.leading.get()
    }

    /// The trailing controls, subscribing to them.
    pub(crate) fn trailing(self) -> Option<ChildrenFn> {
        self.trailing.get()
    }

    /// Whether the page blocks back, subscribing to it.
    pub(crate) fn is_back_blocked(self) -> bool {
        self.back_blocked.get().is_some_and(|blocked| blocked.get())
    }

    /// Whether the page blocks back, without subscribing.
    pub(crate) fn is_back_blocked_untracked(self) -> bool {
        self.back_blocked
            .get_untracked()
            .is_some_and(|blocked| blocked.get_untracked())
    }
}

/// The bar of the page that the calling scope is part of.
#[derive(Copy, Clone)]
pub(crate) struct PageCx {
    /// What the bar shows.
    pub(crate) bar: PageBar,
}

impl PageCx {
    pub(crate) fn provide(self) {
        provide_local_context(self);
    }

    pub(crate) fn current() -> Option<Self> {
        use_local_context::<Self>()
    }
}

/// The controls at the leading end of a [`NavPage`]'s bar, after the back button.
///
/// For an edit mode, block back with `back_blocked` and show Cancel here.
#[slot]
pub struct NavLeading {
    /// The controls.
    children: ChildrenFn,
}

/// The controls at the trailing end of a [`NavPage`]'s bar.
#[slot]
pub struct NavTrailing {
    /// The controls.
    children: ChildrenFn,
}

/// One page of a [`NavStack`](crate::nav_stack::NavStack): its title, its bar controls and its
/// content.
///
/// The stack draws the bar. The bar shows a back button labelled with the title of the page below,
/// the title of this page, and the controls of [`NavLeading`] and [`NavTrailing`]. Outside a stack,
/// a page shows only its content.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// An entry that the person can edit in place.
/// #[component]
/// fn EntryPage() -> impl IntoView {
///     let editing = RwSignal::new_local(false);
///     view! {
///         NavPage(title = "Bank", back_blocked = editing) {
///             NavTrailing(slot) {
///                 Button(on:click = move |_| editing.update(|on| *on = !*on)) {"Edit"}
///             }
///             text {"user@example.com"}
///         }
///     }
/// }
/// ```
#[component(slot_aware)]
pub fn NavPage(
    /// The title in the bar, and the label of the back button of the page above.
    #[prop(into)]
    title: Signal<String, LocalStorage>,
    /// Whether back is blocked. While it is true, the back button is gone, and
    /// [`NavPath::pop`](crate::nav_stack::NavPath::pop) refuses.
    #[prop(into, default = Signal::stored_local(false))]
    back_blocked: Signal<bool, LocalStorage>,
    /// The controls at the leading end of the bar, after the back button.
    #[prop(optional)]
    nav_leading: Option<NavLeading>,
    /// The controls at the trailing end of the bar.
    #[prop(optional)]
    nav_trailing: Option<NavTrailing>,
    /// Classes merged after the content's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the content.
    #[prop(attrs)]
    attrs: Attrs,
    /// The content of the page.
    children: Children,
) -> impl IntoView {
    install_stylesheet(NAV_NAME, NavStackStyle::CSS);
    if let Some(page) = PageCx::current() {
        let bar = page.bar;
        bar.title.set(Some(title));
        bar.back_blocked.set(Some(back_blocked));
        bar.leading.set(nav_leading.map(|leading| leading.children));
        bar.trailing
            .set(nav_trailing.map(|trailing| trailing.children));

        // The signals belong to this scope, so the bar forgets them when it ends.
        on_cleanup_local(move || {
            bar.title.try_set(None);
            bar.back_blocked.try_set(None);
            bar.leading.try_set(None);
            bar.trailing.try_set(None);
        });
    }

    view! {
        column(class = "zui-nav-content", {..attrs}, class = class) {
            {children.into_view_once()}
        }
    }
}
