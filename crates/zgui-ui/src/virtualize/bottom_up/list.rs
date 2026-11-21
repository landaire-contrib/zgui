//! The view of a bottom-up list.

use core::hash::Hash;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::{component, view};

use crate::virtualize::bottom_up::BottomUp;
use crate::virtualize::bottom_up::style::BottomUpListStyle;
use crate::virtualize::list::RowProps;

/// What the bottom-up list's rules are installed under.
const SHEET: &str = "zui-bottom-up-list";

/// A list that grows at its bottom and is read from there, as a conversation is.
///
/// Only the rows near the port are built, as in a [`VirtualList`](crate::virtualize::VirtualList)
/// with measured rows. `row` builds row `index`. [`BottomUp`] decides what the list keeps in view
/// and tells the host what the reader does; its documentation has an example.
///
/// When the rows are shorter than the port, they stand at the bottom of the port.
///
/// # Edge slots
///
/// `top` is shown above the first row and `bottom` below the last row, for example a spinner while
/// a page loads, or a mark at the start of the history. Each is a closure that returns an
/// [`AnyView`](zgui::view::AnyView). They are part of the scroll extent. When `top` changes height
/// while the port is below it, the rows on screen stay where they are.
///
/// Give the port no vertical padding, because the rows are placed from the top of its content. Put
/// space above and below the rows in the slots.
///
/// # What a reader is told
///
/// The container is a list, and each row carries its position and the number of rows held.
#[component]
pub fn BottomUpList<K, V, F>(
    /// What the list keeps in view.
    list: BottomUp<K>,
    /// What one row looks like.
    row: F,
    /// What is shown above the first row.
    #[prop(optional)]
    top: Option<Children>,
    /// What is shown below the last row.
    #[prop(optional)]
    bottom: Option<Children>,
    /// What the list is called, for a reader.
    #[prop(into, optional)]
    label: Option<String>,
    /// Classes merged after the list's own.
    #[prop(into, optional)]
    class: Classes,
    /// Anything else the caller forwarded.
    #[prop(attrs)]
    attrs: Attrs,
) -> impl IntoView
where
    K: Hash + Eq + Clone + 'static,
    V: IntoView + 'static,
    F: Fn(usize) -> V + 'static,
{
    install_stylesheet(SHEET, BottomUpListStyle::CSS);
    let seen = list.virtualize();
    let count = list.rows();
    let row = Rc::new(row);

    let mut semantics = A11yBinding::new(Role::List);
    if let Some(text) = label {
        semantics = semantics.label(text);
    }
    let own = Attrs::new()
        .class_toggle(zgui::view::ClassName::new("zui-bottom-up-list"), true)
        .a11y_from(semantics);

    let lead = move || Some(format!("{}px", seen.window().lead));
    let trail = move || Some(format!("{}px", seen.window().trail));

    // The reader takes the port with the wheel, a pointer or a key.
    view! {
        scroll(
            node_ref = list.port(),
            class = BottomUpListStyle::CLASS,
            on:wheel = move |_| list.let_go(),
            on:pointer_down = move |_| list.let_go(),
            on:key_down = move |_| list.let_go(),
            {..own},
            {..attrs},
            class = class
        ) {
            box(node_ref = list.fill(), class = "zui-bottom-up-list__fill")
            box(node_ref = list.head(), class = "zui-bottom-up-list__head") {
                {top.map(Children::into_view_once)}
            }
            box(
                class = "zui-bottom-up-list__pane",
                style:padding-top = lead,
                style:padding-bottom = trail
            ) {
                for index in move || seen.indices(), key = |index: &usize| *index {
                    Row(index = index, total = count, row = Rc::clone(&row), measured = Some(seen), {..Attrs::new()})
                }
            }
            box(class = "zui-bottom-up-list__foot") {
                {bottom.map(Children::into_view_once)}
            }
        }
    }
}
