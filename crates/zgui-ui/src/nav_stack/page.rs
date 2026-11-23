//! What one page of a navigation stack declares about itself.

use zgui::component;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::slot;
use zgui::view;
use zgui::view::AttrName;

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
    /// Whether the page shows its title large at the top of its content.
    large: RwSignal<bool, LocalStorage>,
    /// Whether the content has scrolled the large title under the bar.
    collapsed: RwSignal<bool, LocalStorage>,
}

impl PageBar {
    pub(crate) fn new() -> Self {
        Self {
            title: RwSignal::new_local(None),
            leading: RwSignal::new_local(None),
            trailing: RwSignal::new_local(None),
            back_blocked: RwSignal::new_local(None),
            large: RwSignal::new_local(false),
            collapsed: RwSignal::new_local(false),
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

    /// Whether the bar shows the title, subscribing to it. A page with a large title shows it in
    /// the bar only once the large title has scrolled under the bar.
    pub(crate) fn shows_title(self) -> bool {
        !self.large.get() || self.collapsed.get()
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
/// The stack draws the bar. The bar shows a back button, the title of this page, and the controls
/// of [`NavLeading`] and [`NavTrailing`]. The back button shows the title of the page below, or a
/// shorter label when that title does not fit. Outside a stack, a page shows only its content.
///
/// With `large_title`, the content is a scroll container that starts with the title in large type.
/// The bar shows the title when the large title scrolls under the bar. Put the content of such a
/// page directly in it, without a scroll container of its own.
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
    /// Whether the content starts with the title in large type, as on the top page of an iOS app.
    #[prop(default = false)]
    large_title: bool,
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
    let page = PageCx::current();
    if let Some(page) = page {
        let bar = page.bar;
        bar.title.set(Some(title));
        bar.back_blocked.set(Some(back_blocked));
        bar.leading.set(nav_leading.map(|leading| leading.children));
        bar.trailing
            .set(nav_trailing.map(|trailing| trailing.children));
        bar.large.set(large_title);

        // The signals belong to this scope, so the bar forgets them when it ends.
        on_cleanup_local(move || {
            bar.title.try_set(None);
            bar.back_blocked.try_set(None);
            bar.leading.try_set(None);
            bar.trailing.try_set(None);
            bar.large.try_set(false);
            bar.collapsed.try_set(false);
        });
    }

    let own = Attrs::new().attribute(AttrName::new("data-large-title"), move || {
        large_title.then(String::new)
    });
    view! {
        column(
            class = "zui-nav-content",
            on:scroll = move |ev| {
                // How far the content scrolls before the large title is under the bar.
                const COLLAPSE: f32 = 41.0;

                if let Some(page) = page
                    && ev.target == ev.current
                {
                    let collapsed = ev.offset.y.0 >= COLLAPSE;
                    if page.bar.collapsed.get_untracked() != collapsed {
                        page.bar.collapsed.set(collapsed);
                    }
                }
            },
            {..own},
            {..attrs},
            class = class
        ) {
            if move || large_title {
                text(
                    class = "zui-nav-large-title",
                    {..Attrs::new().a11y_from(A11yBinding::new(Role::Heading).level(1_usize))}
                ) {
                    {move || title.get()}
                }
            } else {}
            {children.into_view_once()}
        }
    }
}
