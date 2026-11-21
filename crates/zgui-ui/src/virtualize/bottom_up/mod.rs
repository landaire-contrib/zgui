//! A list that grows at its bottom and is read from there, as a conversation is.

mod edge;
mod follow;
mod list;
mod style;
mod visible;

use core::hash::Hash;
use core::ops::Range;
use std::rc::Rc;

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect, StoredValue, Trigger};
use zgui::view::{ScrollBehavior, ScrollPosition, ScrollTarget};

use crate::virtualize::bottom_up::edge::Edge;
use crate::virtualize::bottom_up::follow::{Extent, Follow, Hold, Placed, Sighting, Step};
use crate::virtualize::bottom_up::visible::on_screen;
use crate::virtualize::css_scale;
use crate::virtualize::{MeasuredRows, MeasuredVirtualize};

pub use crate::virtualize::bottom_up::list::{BottomUpList, BottomUpListProps};
pub use crate::virtualize::bottom_up::style::BottomUpListStyle;

/// What a [`BottomUpList`] keeps in view, and what it tells its host.
///
/// Make one in the component that shows the list, and give it to a [`BottomUpList`]. It is
/// `Copy`, so a button or an effect can keep one.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Messages, newest at the bottom, with a button that goes back to the newest.
/// #[component]
/// fn Thread() -> impl IntoView {
///     let ids = RwSignal::new_local(vec![1_u64]);
///     let texts = RwSignal::new_local(vec!["hello".to_owned()]);
///     let count = Signal::derive_local(move || ids.with(Vec::len));
///     let list = BottomUp::new(
///         count,
///         MeasuredRows::new(48.0),
///         move |index| ids.with(|ids| ids[index]),
///         6,
///     )
///     .on_near_start(|| { /* ask for older messages */ });
///
///     view! {
///         column {
///             BottomUpList(
///                 list = list,
///                 label = "Messages",
///                 row = move |index: usize| view! {
///                     text {{move || texts.with(|texts| texts[index].clone())}}
///                 }
///             )
///             if move || !list.at_end() {
///                 Button(on:click = move |_| list.follow_end()) {"Newest"}
///             } else {}
///         }
///     }
/// }
/// ```
///
/// # What it keeps in view
///
/// - **The end.** A list starts at the end and follows it: when rows are added or grow, it scrolls
///   to the end again. When the reader scrolls up, it stops. When the reader scrolls back to the
///   end, it follows again. [`BottomUp::follow_end`] goes to the end and follows it.
/// - **A row.** [`BottomUp::scroll_to_row`] and [`BottomUp::scroll_to_key`] put a row at the top
///   of the port and hold it there while heights settle and rows come in. The hold stops when the
///   reader uses the wheel, presses a pointer or presses a key in the list. To start at a row, call
///   one of them before the list is shown.
/// - **The place the reader scrolled to.** Rows that are added, removed or change height above
///   the port leave the rows on screen where they are, to the pixel.
///
/// # What it tells the host
///
/// - [`BottomUp::at_end`] says whether the port shows the newest rows.
/// - [`BottomUp::visible`] says which rows are on screen.
/// - [`BottomUp::on_near_start`] and [`BottomUp::on_near_end`] are called when the reader comes
///   near the first or the last row held, so that the host can load more rows.
pub struct BottomUp<K: 'static> {
    /// The window and the heights.
    seen: MeasuredVirtualize,
    /// The scroll container.
    port: NodeRef,
    /// The space that puts short rows at the bottom of the port.
    fill: NodeRef,
    /// The box above the rows.
    head: NodeRef,
    /// How many rows the list holds.
    rows: Signal<usize, LocalStorage>,
    /// What the list keeps in view and what it tells the host.
    state: StoredValue<State<K>, LocalStorage>,
    /// Whether the port shows the newest rows.
    at_end: RwSignal<bool, LocalStorage>,
    /// The rows on screen.
    visible: Memo<Range<usize>>,
    /// Asks the list to look at its port again.
    poke: Trigger,
}

impl<K> Clone for BottomUp<K> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<K> Copy for BottomUp<K> {}

impl<K> BottomUp<K>
where
    K: Hash + Eq + Clone + 'static,
{
    /// A list of `rows` rows, each named by `key(index)`.
    ///
    /// The rows are measured as [`MeasuredVirtualize`] measures them, with the estimate
    /// `measured` gives. The list holds each measurement under the row's key, so a row keeps its
    /// height and its place on screen when rows are inserted or removed above it. A key given by
    /// [`MeasuredRows::keyed`] is replaced, and its cost applies: a change to a signal that `key`
    /// reads counts the rows again. `overscan` is how many rows to build beyond each edge of the
    /// port.
    ///
    /// Call it in the scope that shows the list. The list starts at the end.
    #[must_use]
    pub fn new<F>(
        rows: Signal<usize, LocalStorage>,
        measured: MeasuredRows,
        key: F,
        overscan: usize,
    ) -> Self
    where
        F: Fn(usize) -> K + 'static,
    {
        const END_SLACK: f64 = 4.0;
        const EDGE_DISTANCE: f64 = 1_000.0;

        let key: Rc<dyn Fn(usize) -> K> = Rc::new(key);
        let keyed = Rc::clone(&key);
        let port = NodeRef::new();
        let seen = MeasuredVirtualize::new(
            port,
            rows,
            measured.keyed(move |index| keyed(index)),
            overscan,
        );
        let scroll = port.observe_scroll();

        let state = StoredValue::new_local(State {
            key,
            follow: Follow::new(END_SLACK),
            asked: None,
            found: None,
            counted: None,
            start: Edge::new(),
            end: Edge::new(),
            distance: EDGE_DISTANCE,
            on_near_start: None,
            on_near_end: None,
            newer: None,
        });
        let list = Self {
            seen,
            port,
            fill: NodeRef::new(),
            head: NodeRef::new(),
            rows,
            state,
            at_end: RwSignal::new_local(true),
            visible: Memo::new(move |_| visible(seen, port, scroll)),
            poke: Trigger::new(),
        };
        list.watch(scroll);
        list
    }

    /// Says whether the host has rows after the last row the list holds.
    ///
    /// While it is true, the reader that scrolls to the end does not make the list follow it, and
    /// [`BottomUp::at_end`] is false. [`BottomUp::follow_end`] follows the end of the rows held.
    #[must_use]
    pub fn newer(self, newer: impl Into<Signal<bool, LocalStorage>>) -> Self {
        let newer = newer.into();
        self.state
            .try_update_value(|state| state.newer = Some(newer));
        self.poke.notify();
        self
    }

    /// Sets how near the end, in CSS pixels, counts as at the end. Four pixels unless set.
    #[must_use]
    pub fn end_slack(self, slack: f32) -> Self {
        self.state
            .try_update_value(|state| state.follow.set_slack(f64::from(slack)));
        self
    }

    /// Sets how near an end of the rows held, in CSS pixels, the reader comes before the host is
    /// told. A thousand pixels unless set.
    #[must_use]
    pub fn edge_distance(self, distance: f32) -> Self {
        self.state
            .try_update_value(|state| state.distance = f64::from(distance));
        self
    }

    /// Calls `near` when the reader comes near the first row held.
    ///
    /// It is called once for each approach. A new first row, such as a page of older rows that came
    /// in, starts a new approach. It is called during the frame, and the signals it reads wake the
    /// list.
    #[must_use]
    pub fn on_near_start(self, near: impl Fn() + 'static) -> Self {
        let near: Rc<dyn Fn()> = Rc::new(near);
        self.state
            .try_update_value(|state| state.on_near_start = Some(near));
        self
    }

    /// Calls `near` when the reader comes near the last row held while the host has newer rows.
    ///
    /// It is called once for each approach. A new last row starts a new approach. It is called
    /// during the frame, and the signals it reads wake the list.
    #[must_use]
    pub fn on_near_end(self, near: impl Fn() + 'static) -> Self {
        let near: Rc<dyn Fn()> = Rc::new(near);
        self.state
            .try_update_value(|state| state.on_near_end = Some(near));
        self
    }

    /// Whether the port shows the newest rows, subscribing to it.
    ///
    /// True while the list follows the end or the port shows the end, and the host has no newer
    /// rows.
    #[must_use]
    pub fn at_end(&self) -> bool {
        self.at_end.try_get().unwrap_or(false)
    }

    /// The same, without subscribing.
    #[must_use]
    pub fn at_end_untracked(&self) -> bool {
        self.at_end.try_get_untracked().unwrap_or(false)
    }

    /// The rows on screen, subscribing to them.
    ///
    /// A row is on screen when half of it or more is in the port, or when it fills half of the
    /// port or more.
    #[must_use]
    pub fn visible(&self) -> Range<usize> {
        self.visible.try_get().unwrap_or_default()
    }

    /// The same, without subscribing.
    #[must_use]
    pub fn visible_untracked(&self) -> Range<usize> {
        self.visible.try_get_untracked().unwrap_or_default()
    }

    /// Goes to the end at once, and follows it.
    pub fn follow_end(&self) {
        let followed = self.state.try_update_value(|state| {
            state.follow.follow_end();
            state.asked = None;
        });
        if followed.is_some() {
            self.scroll_to_end();
            self.poke.notify();
        }
    }

    /// Puts row `index` at the top of the port, and holds it there until the reader scrolls.
    ///
    /// The row is held by its key, so it stays at the top when rows are inserted above it. Answers
    /// false, and does nothing, when the list has no row `index`.
    pub fn scroll_to_row(&self, index: usize) -> bool {
        if index >= self.rows.try_get_untracked().unwrap_or(0) {
            return false;
        }

        // The list reads the row's key when it next looks, so that the caller does not subscribe
        // to the rows.
        let asked = self
            .state
            .try_update_value(|state| state.asked = Some(index));
        self.poke.notify();
        asked.is_some()
    }

    /// Puts the row with `key` at the top of the port, and holds it there until the reader
    /// scrolls.
    ///
    /// When the list has no row with `key`, it waits for one. [`BottomUp::follow_end`], another
    /// row to hold, or the reader, ends the wait.
    pub fn scroll_to_key(&self, key: K) {
        self.state.try_update_value(|state| {
            state.follow.hold_row(key);
            state.asked = None;
            state.found = None;
        });
        self.poke.notify();
    }

    /// The window and the heights the list is built on.
    ///
    /// For [`MeasuredVirtualize::invalidate`] and [`MeasuredVirtualize::reset`], when rows change
    /// in a way that their measurements do not show.
    #[must_use]
    pub const fn virtualize(&self) -> MeasuredVirtualize {
        self.seen
    }

    /// The scroll container, once a [`BottomUpList`] has built it.
    #[must_use]
    pub const fn port(&self) -> NodeRef {
        self.port
    }

    /// The space that puts short rows at the bottom of the port.
    pub(crate) const fn fill(&self) -> NodeRef {
        self.fill
    }

    /// The box above the rows.
    pub(crate) const fn head(&self) -> NodeRef {
        self.head
    }

    /// How many rows the list holds.
    pub(crate) const fn rows(&self) -> Signal<usize, LocalStorage> {
        self.rows
    }

    /// Stops holding a row, because the reader took the port.
    pub(crate) fn let_go(&self) {
        self.state.try_update_value(|state| {
            state.follow.let_go();
            state.asked = None;
        });
    }

    /// Follows the port and the space above the rows for as long as the calling scope lives.
    fn watch(self, scroll: Signal<ScrollPosition, LocalStorage>) {
        let fill = self.fill.observe_border_size();
        let head = self.head.observe_border_size();
        let measured = self.seen.measured();
        let port = self.port;
        let origin = RenderEffect::new(move |_| {
            let (fill, head) = (fill.get(), head.get());
            if fill.is_none() && head.is_none() {
                return;
            }

            // A box not yet laid out takes no space.
            let scale = css_scale(port);
            let height = |size: Option<zgui::geom::Size<DevicePx, zgui::geom::Device>>| {
                size.map_or(0.0, |size| size.height.0 / scale)
            };
            measured.set_origin(height(fill), height(head));
        });

        let following = RenderEffect::new(move |_| {
            let Some(position) = scroll.try_get() else {
                return;
            };
            let count = self.rows.try_get().unwrap_or(0);
            self.poke.track();
            let newer = self
                .state
                .try_with_value(|state| state.newer)
                .flatten()
                .and_then(|newer| newer.try_get())
                .unwrap_or(false);
            self.look(position, count, newer);
        });

        on_cleanup_local(move || {
            drop(origin);
            drop(following);
        });
    }

    /// Looks at what the port shows, and does what the list keeps in view asks for.
    fn look(self, position: ScrollPosition, count: usize, newer: bool) {
        let Some((key, asked)) = self
            .state
            .try_update_value(|state| (Rc::clone(&state.key), state.asked.take()))
        else {
            return;
        };
        if let Some(index) = asked.filter(|index| *index < count) {
            let wanted = key(index);
            self.state.try_update_value(|state| {
                state.follow.hold_row(wanted);
                state.found = Some(index);
            });
        }

        let laid_out = position.content_size.height.0 > 0.0 && position.scrollport.height.0 > 0.0;
        let Some((follow_end, recounted)) = self.state.try_update_value(|state| {
            let recounted = state.counted.replace(count) != Some(count);
            if !laid_out {
                state.follow.forget_motion();
            }
            (*state.follow.hold() == Hold::End, recounted)
        }) else {
            return;
        };

        // Rows added while the list follows the end are scrolled to in the frame that adds them.
        if follow_end && recounted {
            self.scroll_to_end();
        }
        if !laid_out {
            self.show_end(follow_end && !newer);
            return;
        }

        let measured = self.seen.measured();
        let scale = css_scale(self.port);
        let offset = measured.offset();
        let extent = Extent {
            content: position.content_size.height.0 / scale,
            port: position.scrollport.height.0 / scale,
        };
        let held = self.held(&*key, count);
        // Read from one layout. Rows inserted above the port lengthen the content in a later one.
        let sighting = Sighting {
            remaining: f64::from(position.remaining().height.0 / scale),
            drift: measured.drift(),
            settled: measured.settled(),
            extent,
            newer,
            held: held.and_then(|index| {
                Some(Placed {
                    row: f64::from(measured.offset_of(index)?),
                    port: offset,
                })
            }),
        };

        let Some((step, at_end, waiting)) = self.state.try_update_value(|state| {
            let step = state.follow.see(sighting);
            let waiting = matches!(state.follow.hold(), Hold::Row(_)) && held.is_none();
            (step, state.follow.at_end(&sighting), waiting)
        }) else {
            return;
        };
        self.show_end(at_end);

        match step {
            Step::Stay => {}
            Step::ScrollToEnd => self.scroll_to_end(),
            Step::ScrollToRow => {
                if let Some(index) = held {
                    self.seen.scroll_to_row(index, ScrollBehavior::Instant);
                }
            }
        }

        // The edges are decided where the port rests once the scrolls asked for are done.
        if step != Step::Stay || waiting || (follow_end && recounted) {
            return;
        }
        self.edges(&*key, count, offset, extent, newer);
    }

    /// Sets whether the port shows the newest rows.
    fn show_end(self, at_end: bool) {
        if self
            .at_end
            .try_get_untracked()
            .is_some_and(|shown| shown != at_end)
        {
            self.at_end.set(at_end);
        }
    }

    /// The index of the held row, when a row is held and the list has it.
    fn held(self, key: &dyn Fn(usize) -> K, count: usize) -> Option<usize> {
        let (wanted, found) = self
            .state
            .try_with_value(|state| match state.follow.hold() {
                Hold::Row(wanted) => Some((wanted.clone(), state.found)),
                Hold::End | Hold::Free => None,
            })??;

        if let Some(index) = found
            && index < count
            && key(index) == wanted
        {
            return Some(index);
        }

        let index = (0..count).find(|index| key(*index) == wanted);
        self.state.try_update_value(|state| state.found = index);
        index
    }

    /// Tells the host when the port comes near an end of the rows held.
    fn edges(
        self,
        key: &dyn Fn(usize) -> K,
        count: usize,
        offset: f64,
        extent: Extent,
        newer: bool,
    ) {
        let measured = self.seen.measured();
        let top = f64::from(measured.origin());
        let bottom = top + f64::from(self.seen.window_untracked().total());
        let first = (count > 0).then(|| key(0));
        let last = count.checked_sub(1).map(key);

        let Some((start, end)) = self.state.try_update_value(|state| {
            let start = state.start.see(first, offset - top < state.distance);
            let near_end = bottom - (offset + f64::from(extent.port)) < state.distance;
            let end = state.end.see(last, newer && near_end);
            (
                state.on_near_start.clone().filter(|_| start),
                state.on_near_end.clone().filter(|_| end),
            )
        }) else {
            return;
        };

        if let Some(near) = start {
            near();
        }
        if let Some(near) = end {
            near();
        }
    }

    /// Scrolls to the end of the content at once.
    ///
    /// The container stops the offset at the end of the content as it is laid out, and carries
    /// the scroll again after each layout of the frame.
    fn scroll_to_end(self) {
        let across = self.port.scroll_offset().x;
        self.port.scroll_to(
            ScrollTarget::Offset(Point::new(across, DevicePx(f32::MAX))),
            ScrollBehavior::Instant,
        );
    }
}

/// What a [`BottomUp`] holds outside the reactive graph.
struct State<K> {
    /// The key of each row.
    key: Rc<dyn Fn(usize) -> K>,
    /// What the list keeps in view.
    follow: Follow<K>,
    /// A row the host asked to hold by its index, until the list reads its key.
    asked: Option<usize>,
    /// Where the held row was found last.
    found: Option<usize>,
    /// How many rows the list held when it last looked.
    counted: Option<usize>,

    /// The first row held.
    start: Edge<K>,
    /// The last row held.
    end: Edge<K>,
    /// How near an end of the rows held, in CSS pixels, counts as near.
    distance: f64,
    /// Called when the reader comes near the first row held.
    on_near_start: Option<Rc<dyn Fn()>>,
    /// Called when the reader comes near the last row held.
    on_near_end: Option<Rc<dyn Fn()>>,

    /// Whether the host has rows after the last row held.
    newer: Option<Signal<bool, LocalStorage>>,
}

/// The rows of `seen` on screen in the port `viewport`, subscribing to them.
fn visible(
    seen: MeasuredVirtualize,
    viewport: NodeRef,
    scroll: Signal<ScrollPosition, LocalStorage>,
) -> Range<usize> {
    let Some(position) = scroll.try_get() else {
        return 0..0;
    };
    let window = seen.window();
    let measured = seen.measured();

    let mut top = f64::from(measured.origin() + window.lead);
    let rows = (window.first..window.end()).map_while(|index| {
        let height = f64::from(measured.height_of(index)?);
        let row = (index, top, top + height);
        top += height;
        Some(row)
    });
    let height = f64::from(position.scrollport.height.0 / css_scale(viewport));
    on_screen(rows, measured.offset(), height)
}
