//! A long list, of which only the visible part exists.

use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::view::CustomPropertyName;
use zgui::{component, view};

use crate::virtualize::style::VirtualListStyle;
use crate::virtualize::{MeasuredRows, MeasuredVirtualize, Virtualize};

/// What the virtualised list's rules are installed under.
const SHEET: &str = "zui-virtual-list";

/// A list of any length that costs the same as a list the size of its scrollport.
///
/// The rows are built by the `row` closure, which is handed an index and returns a view. Only the
/// indices in view — plus `overscan` rows of slack at each edge — are ever asked for, so a list of
/// a hundred thousand rows builds a couple of dozen elements and replaces the rest with two lengths
/// of padding. Scrolling within one row rebuilds nothing at all; scrolling past one destroys one
/// row and builds one row.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Every line of a long log file.
/// #[component]
/// fn Log() -> impl IntoView {
///     let lines = RwSignal::new_local(100_000_usize);
///     view! {
///         VirtualList(
///             count = lines,
///             row_size = 22.0,
///             label = "Log",
///             row = move |index: usize| view! { text {{move || format!("line {index}")}} }
///         )
///     }
/// }
/// ```
///
/// # What a reader is told
///
/// A list whose rows are a window onto a longer one has to say so, or an assistive technology
/// announces "row 3 of 25" in a list of a hundred thousand. Each row therefore carries its true
/// position and the true length of the list, and the container is a list rather than a box.
///
/// # What a row is keyed by
///
/// Its position. Row 4 200 is row 4 200 whatever the list now holds, which is what makes a scroll
/// of one row cost one row and nothing else. A `row` closure that captures the data it draws
/// therefore reaches the rows on screen through a rebuild of the list, which is what a keyed list
/// builds every row again for. A `row` closure that reads a signal updates without one.
///
/// # Rows that differ in height
///
/// With `measured`, each row is as tall as its content and is measured as it is built. A row that
/// has not been built stands at the estimate `measured` gives it. When a row above the first
/// visible one changes height, the list scrolls by the same amount, so the rows on screen stay
/// where they are. [`MeasuredRows`] says what the measurements cost.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Every message of a long thread, each as tall as its text.
/// #[component]
/// fn Thread() -> impl IntoView {
///     let messages = RwSignal::new_local(20_000_usize);
///     view! {
///         VirtualList(
///             count = messages,
///             measured = MeasuredRows::new(48.0),
///             label = "Thread",
///             row = move |index: usize| view! { text {{move || format!("message {index}")}} }
///         )
///     }
/// }
/// ```
///
/// # Keyboard
///
/// None of its own. A virtualised list is a scroll container, and scrolling is the framework's;
/// rows that are operable are operable because the views `row` returns are, and each of them is an
/// ordinary tab stop while it exists. A list whose rows are focusable and whose window is thousands
/// of rows wide is a list where <kbd>Tab</kbd> cannot reach every row — which is a property of
/// virtualisation itself rather than of this component, and is why an operable virtualised list
/// wants a roving tab stop of its own.
#[component]
pub fn VirtualList<V, F>(
    /// How many rows there are in total.
    #[prop(into)]
    count: Signal<usize, LocalStorage>,
    /// How tall one row is, in CSS pixels.
    ///
    /// Declared rather than measured: the window is decided before its rows are built, so a height
    /// taken from the rows would mean building all of them to find out which to build. The list
    /// writes the declaration into `--zui-virtual-row`, which is what makes every row that tall, so
    /// a signal here moves the rows and the scroll extent together. Unused when `measured` is
    /// given.
    #[prop(into, default = Signal::stored_local(32.0))]
    row_size: Signal<f32, LocalStorage>,
    /// Measures each row as it is built, so that the rows can differ in height.
    #[prop(optional)]
    measured: Option<MeasuredRows>,
    /// How many rows beyond each edge of the port to build, so a fast scroll shows rows rather
    /// than a gap.
    #[prop(default = 4)]
    overscan: usize,
    /// What one row looks like.
    row: F,
    /// What the list is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Where to record the scroll container, for a caller that wants to scroll it.
    #[prop(optional)]
    node_ref: Option<NodeRef>,
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    V: IntoView + 'static,
    F: Fn(usize) -> V + 'static,
{
    install_stylesheet(SHEET, VirtualListStyle::CSS);
    let viewport = node_ref.unwrap_or_default();
    let seen = match measured {
        Some(measured) => {
            Seen::Measured(MeasuredVirtualize::new(viewport, count, measured, overscan))
        }
        None => Seen::Declared(Virtualize::new(viewport, count, row_size, overscan)),
    };
    let measuring = seen.measured();
    let row = Rc::new(row);

    let mut semantics = A11yBinding::new(Role::List);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }

    // A measured row is as tall as its content, and `auto` is the height that says so.
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-virtual-list"), true)
        .custom_property(CustomPropertyName::new("zui-virtual-row"), move || {
            if measuring.is_some() {
                return Some("auto".to_owned());
            }
            row_size.try_get().map(|size| format!("{size}px"))
        })
        .a11y_from(semantics);

    // Read once per frame into one value, so the two paddings and the row set are three reads of
    // one memo rather than three independent recomputations of the same window.
    let window = Signal::derive_local(move || seen.span());

    view! {
        scroll(node_ref = viewport, class = VirtualListStyle::CLASS, {..own}, {..attrs}, class = class) {
            box(
                class = "zui-virtual-list__pane",
                style:padding-top = move || window.try_get().map(|seen| px(seen.lead)),
                style:padding-bottom = move || window.try_get().map(|seen| px(seen.trail))
            ) {
                for index in move || window.try_get().map(|seen| seen.indices()).unwrap_or_default(),
                    key = |index: &usize| *index {
                    Row(index = index, total = count, row = Rc::clone(&row), measured = measuring, {..Attrs::new()})
                }
            }
        }
    }
}

/// The window a [`VirtualList`] follows, whichever way its rows are sized.
#[derive(Copy, Clone)]
enum Seen {
    /// Every row is the declared height.
    Declared(Virtualize),
    /// Each row is measured.
    Measured(MeasuredVirtualize),
}

impl Seen {
    /// The rows to build and the space around them, subscribing to it.
    fn span(self) -> Span {
        match self {
            Self::Declared(seen) => {
                let window = seen.window();
                Span {
                    first: window.first,
                    count: window.count,
                    lead: window.lead,
                    trail: window.trail,
                }
            }
            Self::Measured(seen) => {
                let window = seen.window();
                Span {
                    first: window.first,
                    count: window.count,
                    lead: window.lead,
                    trail: window.trail,
                }
            }
        }
    }

    /// The handle that measures the rows, when they are measured.
    const fn measured(self) -> Option<MeasuredVirtualize> {
        match self {
            Self::Declared(_) => None,
            Self::Measured(seen) => Some(seen),
        }
    }
}

/// The rows a [`VirtualList`] builds and the two spacers around them, in CSS pixels.
#[derive(Copy, Clone, PartialEq, Debug)]
struct Span {
    /// The index of the first row that is built.
    first: usize,
    /// How many rows are built.
    count: usize,
    /// The height of everything before the first built row.
    lead: f32,
    /// The height of everything after the last built row.
    trail: f32,
}

impl Span {
    /// The indices that are built, in order.
    fn indices(self) -> Vec<usize> {
        (self.first..self.first + self.count).collect()
    }
}

/// One row of a [`VirtualList`], carrying where it really sits.
///
/// It takes a forwarded bundle like every component here that renders an element of its own, and
/// the list hands it an empty one: what a *caller* wrote lands on the scroll container, which is
/// the element they can name, rather than on one of a window's worth of rows that come and go.
#[component]
fn Row<V, F>(
    /// Which row this is, counting from zero, in the whole list rather than in the window.
    index: usize,
    /// How long the whole list is.
    #[prop(into)]
    total: Signal<usize, LocalStorage>,
    /// What builds the row's content.
    row: Rc<F>,
    /// The list's measured window, which measures the row, when its rows differ in height.
    measured: Option<MeasuredVirtualize>,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    V: IntoView + 'static,
    F: Fn(usize) -> V + 'static,
{
    let semantics = A11yBinding::new(Role::ListItem).step(move |a11y| {
        a11y.set_position(
            index + 1,
            total.try_get().unwrap_or_default().max(index + 1),
        )
    });
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-virtual-list__row"), true)
        .attribute(
            zgui::view::AttrName::new("data-index"),
            Some(index.to_string()),
        )
        .a11y_from(semantics);

    let element = zgui::elements::r#box()
        .attrs(own)
        .attrs(attrs)
        .child(row(index));
    let Some(measured) = measured else {
        return element;
    };

    let node = NodeRef::new();
    measured.measure(index, node);
    element.node_ref(node)
}

/// A length as CSS text.
fn px(value: f32) -> String {
    format!("{value}px")
}
