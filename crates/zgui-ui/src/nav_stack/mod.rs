//! A stack of pages with a nav bar, which pushes and pops as iOS does.

mod bar;
mod error;
mod page;
mod path;
mod style;
mod swipe;

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
use zgui::view::CustomPropertyName;
use zgui::view::For;
use zgui::view::TimeoutHandle;
use zgui::view::Timers;

use crate::nav_stack::bar::NavBarRowProps;
use crate::nav_stack::page::PageBar;
use crate::nav_stack::page::PageCx;
use crate::nav_stack::path::Entry;
use crate::nav_stack::path::Motion;
use crate::nav_stack::path::NavCore;
use crate::nav_stack::swipe::SwipeBack;
use crate::nav_stack::swipe::Taker;

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
/// The title stays centered on the bar while it fits there. The back button shows the title of the
/// page below while that fits beside the centered title, then `back_label`, then the chevron only.
/// The title moves off center before the back button loses `back_label`, and truncates last. The
/// leading and trailing controls keep their widths. A screen reader always reads the back button
/// as the title of the page below.
///
/// A drag toward the trailing edge goes back, as on iOS. It starts at the leading edge, or anywhere
/// on the page where no scroll container can scroll sideways. The top page follows the finger, and
/// the page below follows with parallax. On release, the swipe completes past half the width or
/// above 300 CSS pixels per second, and the page continues at the speed of the finger. A vertical
/// drag scrolls. While back is blocked, the stack takes no swipe. With Reduce Motion, the page
/// follows the finger also.
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
    /// What the back button says when the title of the page below does not fit beside the title.
    /// Give the translation of "Back" here.
    #[prop(into, default = Signal::stored_local(String::from("Back")))]
    back_label: Signal<String, LocalStorage>,
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

    let swipe_area = NodeRef::new();
    let swipe = SwipeBack::new(path, swipe_area);
    let pages = For::new(
        move || path.mounted(),
        |entry: &Entry<P>| entry.id,
        move |entry: Entry<P>| {
            let page = Rc::clone(&page);
            AnyView::new(view! {
                NavPageHost(core = core, id = entry.id, settle = settle, swipe = swipe) {
                    {page(entry.page)}
                }
            })
        },
    );
    let bars = For::new(
        move || core.bars(),
        |(id, _): &(PageId, PageBar)| *id,
        move |(id, bar): (PageId, PageBar)| {
            AnyView::new(
                view! { NavBarRow(core = core, id = id, bar = bar, back = back, back_label = back_label) },
            )
        },
    );

    // The swiped pages read where the finger holds them from these properties.
    let own = Attrs::new()
        .custom_property(CustomPropertyName::new("zui-nav-swipe-top"), move || {
            Some(format!("{}%", core.progress() * 100.0))
        })
        .custom_property(CustomPropertyName::new("zui-nav-swipe-below"), move || {
            Some(format!("{}%", (core.progress() - 1.0) * 30.0))
        })
        .custom_property(CustomPropertyName::new("zui-nav-swipe-shown"), move || {
            Some(core.progress().to_string())
        })
        .custom_property(CustomPropertyName::new("zui-nav-swipe-hidden"), move || {
            Some((1.0 - core.progress()).to_string())
        })
        .custom_property(CustomPropertyName::new("zui-nav-swipe-ease"), move || {
            core.ease()
        });

    view! {
        column(class = NavStackStyle::CLASS, {..own}, {..attrs}, class = class) {
            box(class = "zui-nav-bar") {
                {bars}
            }
            // Offered a pan before the scroll containers in the pages, for a swipe from the edge.
            box(
                class = "zui-nav-pages",
                node_ref = swipe_area,
                on:horizontal_pan:capture = move |ev| {
                    if !swipe.handle(**ev, Taker::Edge) {
                        ev.prevent_default();
                    }
                }
            ) {
                {pages}
            }
        }
    }
}

/// Ends a motion that no animation or transition ends, after a deadline.
///
/// The end of the moving page's animation or transition ends the motion. The deadline ends it when
/// that event does not arrive, for example when the style sheet sets no animation.
fn watch_motion(
    motion: Motion,
    deadline: StoredValue<Option<TimeoutHandle>, LocalStorage>,
    settle: UnsyncCallback<()>,
) {
    const DEADLINE: Duration = Duration::from_millis(900);

    deadline.set_value(None);
    // A finger ends a swipe in progress, and no deadline does.
    if matches!(motion, Motion::Still | Motion::Swipe { end: None, .. }) {
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
fn NavPageHost<P>(
    /// The stack.
    core: NavCore,
    /// Which page.
    id: PageId,
    /// Ends the motion.
    settle: UnsyncCallback<()>,
    /// The swipe back of the stack.
    swipe: SwipeBack<P>,
    /// The view of the page.
    children: Children,
) -> impl IntoView
where
    P: Clone + 'static,
{
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
            on:transition_end = move |ev| {
                if ev.target == ev.current && place.get_untracked().ends_motion() {
                    settle.run(());
                }
            },
            // Offered a pan only where no scroll container under the finger scrolls sideways.
            on:horizontal_pan = move |ev| {
                if !swipe.handle(**ev, Taker::Content) {
                    ev.prevent_default();
                }
            },
            {..own}
        ) {
            {children.into_view_once()}
            box(class = "zui-nav-page__dim", a11y:hidden = true)
        }
    }
}
