//! A stack of pages with a nav bar, which pushes and pops as iOS does.

mod error;
mod page;
mod path;
mod style;

pub use crate::nav_stack::error::PopError;
pub use crate::nav_stack::page::NavLeading;
pub use crate::nav_stack::page::NavPage;
pub use crate::nav_stack::page::NavPageProps;
pub use crate::nav_stack::page::NavTrailing;
pub use crate::nav_stack::path::NavPath;
pub use crate::nav_stack::path::PageId;
pub use crate::nav_stack::style::NavStackStyle;

use core::time::Duration;
use std::rc::Rc;

use zgui::component;
use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RenderEffect;
use zgui::reactive::StoredValue;
use zgui::reactive::UnsyncCallback;
use zgui::view;
use zgui::view::For;
use zgui::view::TimeoutHandle;
use zgui::view::Timers;
use zgui_ui_icons::prelude::*;
use zgui_ui_icons::set::chevron::CHEVRON_LEFT;

use crate::nav_stack::page::PageBar;
use crate::nav_stack::page::PageCx;
use crate::nav_stack::path::Entry;
use crate::nav_stack::path::Motion;
use crate::nav_stack::path::NavCore;

/// What the navigation stack's rules are installed under.
pub(crate) const NAV_NAME: &str = "zui-nav-stack";

/// A stack of pages under one nav bar.
///
/// `path` holds the routes, and `page` builds the view of one route. The stack builds each pushed
/// page once and keeps it mounted until it is popped, so the pages below the top page keep their
/// state and their scroll position. A page declares its title and its bar controls with
/// [`NavPage`].
///
/// A push slides the new page in from the trailing edge. The page below moves 30 percent of the
/// width the other way and dims. A pop does the reverse. The bar titles cross-fade. With Reduce
/// Motion, the pages cross-fade in place.
///
/// The pages below the top page are hidden, inert, and absent from the accessibility tree.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// #[derive(Clone)]
/// enum Route {
///     Groups,
///     Group(String),
/// }
///
/// /// The groups of a vault, and the entries of each.
/// #[component]
/// fn Vault() -> impl IntoView {
///     let path = NavPath::new(Route::Groups);
///     view! {
///         NavStack(path = path, page = move |route: Route| match route {
///             Route::Groups => AnyView::new(view! {
///                 NavPage(title = "Groups") {
///                     Button(on:click = move |_| path.push(Route::Group("Bank".into()))) {"Bank"}
///                 }
///             }),
///             Route::Group(name) => AnyView::new(view! {
///                 NavPage(title = name) { text {"Entries"} }
///             }),
///         })
///     }
/// }
/// ```
///
/// # Keyboard
///
/// None of its own. The back button is an ordinary control.
#[component]
pub fn NavStack<P, V, F>(
    /// The routes in the stack.
    path: NavPath<P>,
    /// Builds the view of one route.
    page: F,
    /// Classes merged after the stack's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded, which lands on the stack.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    P: Clone + 'static,
    V: IntoView + 'static,
    F: Fn(P) -> V + 'static,
{
    install_stylesheet(NAV_NAME, NavStackStyle::CSS);
    let core = path.core();
    let page = Rc::new(page);

    let settle = UnsyncCallback::new(move |()| path.settle());
    let deadline = StoredValue::new_local(None::<TimeoutHandle>);
    let watching = RenderEffect::new(move |_| watch_motion(core.motion(), deadline, settle));
    on_cleanup_local(move || {
        drop(watching);
        deadline.try_set_value(None);
    });

    // The back button shows only while a pop can succeed.
    let back = UnsyncCallback::new(move |()| {
        path.pop().ok();
    });

    let pages = For::new(
        move || path.mounted(),
        |entry: &Entry<P>| entry.id,
        move |entry: Entry<P>| {
            let page = Rc::clone(&page);
            AnyView::new(view! {
                NavPageHost(core = core, id = entry.id, settle = settle) {
                    {page(entry.page)}
                }
            })
        },
    );
    let bars = For::new(
        move || core.bars(),
        |(id, _): &(PageId, PageBar)| *id,
        move |(id, bar): (PageId, PageBar)| {
            AnyView::new(view! { NavBarRow(core = core, id = id, bar = bar, back = back) })
        },
    );

    view! {
        column(class = NavStackStyle::CLASS, {..attrs}, class = class) {
            box(class = "zui-nav-bar") {
                {bars}
            }
            box(class = "zui-nav-pages") {
                {pages}
            }
        }
    }
}

/// Ends a motion that no animation ends, after a deadline.
///
/// The end of the moving page's animation ends the motion. The deadline ends it when that event
/// does not arrive, for example when the style sheet sets no animation.
fn watch_motion(
    motion: Motion,
    deadline: StoredValue<Option<TimeoutHandle>, LocalStorage>,
    settle: UnsyncCallback<()>,
) {
    const DEADLINE: Duration = Duration::from_millis(900);

    deadline.set_value(None);
    if motion == Motion::Still {
        return;
    }

    let Some(clock) = Timers::current() else {
        settle.run(());
        return;
    };
    deadline.set_value(Some(clock.set_timeout(DEADLINE, move || settle.run(()))));
}

/// The element of one page, which carries where the page is drawn.
#[component]
fn NavPageHost(
    /// The stack.
    core: NavCore,
    /// Which page.
    id: PageId,
    /// Ends the motion.
    settle: UnsyncCallback<()>,
    /// The view of the page.
    children: Children,
) -> impl IntoView {
    let bar = PageBar::new();
    core.register(id, bar);
    on_cleanup_local(move || core.unregister(id));
    PageCx { bar }.provide();

    let place = Signal::derive_local(move || core.place(id));
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-place"), move || {
            Some(place.get().name().to_owned())
        })
        .attribute(zgui::view::AttrName::new("inert"), move || {
            (!place.get().is_active()).then(String::new)
        })
        .a11y_from(A11yBinding::unspecified().hidden(move || !place.get().is_active()));

    view! {
        column(
            class = "zui-nav-page",
            on:animation_end = move |ev| {
                if ev.target == ev.current && place.get_untracked().ends_motion() {
                    settle.run(());
                }
            },
            {..own}
        ) {
            {children.into_view_once()}
            box(class = "zui-nav-page__dim", a11y:hidden = true)
        }
    }
}

/// The bar of one page: back button or leading controls, title, trailing controls.
#[component]
fn NavBarRow(
    /// The stack.
    core: NavCore,
    /// Which page.
    id: PageId,
    /// What the page declared.
    bar: PageBar,
    /// Goes back.
    back: UnsyncCallback<()>,
) -> impl IntoView {
    let place = Signal::derive_local(move || core.place(id));
    let own = Attrs::new()
        .attribute(zgui::view::AttrName::new("data-place"), move || {
            Some(place.get().name().to_owned())
        })
        .attribute(zgui::view::AttrName::new("inert"), move || {
            (!place.get().is_active()).then(String::new)
        })
        .a11y_from(A11yBinding::unspecified().hidden(move || !place.get().is_active()));

    let back_title = move || core.bar_below(id).map(PageBar::title).unwrap_or_default();
    let back_button = move || {
        if !core.has_page_below(id) || bar.is_back_blocked() {
            return None;
        }

        Some(view! {
            control(
                class = "zui-nav-back",
                tabindex = {Focus::Sequential},
                on:click = move |_| back.run(()),
                {..Attrs::new().a11y_from(A11yBinding::new(Role::Button).label("Back"))}
            ) {
                Icon(icon = CHEVRON_LEFT)
                text(class = "zui-nav-back__title") {{back_title}}
            }
        })
    };
    let leading = move || bar.leading().map(|leading| leading.view());
    let trailing = move || bar.trailing().map(|trailing| trailing.view());

    view! {
        row(class = "zui-nav-bar__row", {..own}) {
            box(class = "zui-nav-bar__leading") {
                {back_button}
                {leading}
            }
            text(
                class = "zui-nav-bar__title",
                {..Attrs::new().a11y_from(A11yBinding::new(Role::Heading).level(1_usize))}
            ) {
                {move || bar.title()}
            }
            box(class = "zui-nav-bar__trailing") {{trailing}}
        }
    }
}
