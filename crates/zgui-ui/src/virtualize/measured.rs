//! The reactive half of a list whose rows are measured.

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, StoredValue, Trigger};
use zgui::view::{ScrollBehavior, ScrollPosition, ScrollTarget};

use crate::virtualize::css_scale;
use crate::virtualize::heights::{Change, RowHeights};
use crate::virtualize::measure::{Estimate, MeasuredRows};
use crate::virtualize::measured_window::MeasuredWindow;

/// The heights of a measured list, and what keeps its window following them.
#[derive(Copy, Clone)]
pub(crate) struct Measured {
    /// The heights, the estimate and the scroll the list has asked for.
    state: StoredValue<State, LocalStorage>,
    /// Counts the list again when the row count, the estimate or a key changes.
    ///
    /// A memo of its own, so that the signals the estimate and the key function read are tracked
    /// by a run that reads them all, and the window memo that reads this one runs after it.
    counted: Memo<u64>,
    /// Notified when a height changes.
    changed: Trigger,
    /// The container's scroll position.
    scroll: Signal<ScrollPosition, LocalStorage>,
    /// The scroll container.
    viewport: NodeRef,
}

/// What a measured list holds outside the reactive graph.
struct State {
    /// The height of each row.
    heights: RowHeights,
    /// What a row is given before it is measured.
    estimate: Estimate,
    /// A scroll the list asked for to keep its first visible row in place, until it is observed.
    pending: Option<Pending>,

    /// How far below the top of the container the first row starts, in CSS pixels.
    origin: f32,
    /// The part of `origin` that the box above the rows takes, in CSS pixels.
    head: f32,
    /// The sum of every scroll the list asked for to keep its first visible row in place, in CSS
    /// pixels.
    shifted: f64,
}

impl State {
    /// The offset into the rows that the port shows at its top, when the container last reported
    /// `observed`.
    fn rows_offset(&self, observed: f32) -> f64 {
        anchored(self.pending, observed) - f64::from(self.origin)
    }

    /// Records the scroll `change` asks for from `observed`.
    fn absorb(&mut self, observed: f32, change: Change) {
        self.pending = follow(self.pending, observed, change);
        self.shifted += change.shift;
    }
}

/// A scroll the list asked for and has not yet seen.
///
/// The observation of the scroll position comes back one delivery after the request. A window
/// decided in between would show the rows the old offset covers under the new heights, so it is
/// decided at the offset that was asked for.
#[derive(Copy, Clone, Debug)]
struct Pending {
    /// The observed offset the scroll was asked from, in CSS pixels.
    from: f32,
    /// How far it was asked to move, in CSS pixels.
    shift: f64,
}

impl Measured {
    /// Follows `rows` rows of the container `viewport`, sized as `measured` says.
    pub(crate) fn new(
        viewport: NodeRef,
        rows: Signal<usize, LocalStorage>,
        measured: MeasuredRows,
        scroll: Signal<ScrollPosition, LocalStorage>,
    ) -> Self {
        let MeasuredRows { estimate, keys } = measured;
        let state = StoredValue::new_local(State {
            heights: RowHeights::new(keys),
            estimate,
            pending: None,
            origin: 0.0,
            head: 0.0,
            shifted: 0.0,
        });

        let counted = Memo::new(move |previous: Option<&u64>| {
            // A count that is gone belongs to a list that is being taken down, which has no rows.
            let count = rows.try_get().unwrap_or(0);
            let observed = observed(scroll, viewport);
            let change = state
                .try_update_value(|state| {
                    let estimate = state.estimate.read(true)?;
                    let offset = state.rows_offset(observed);
                    let change = state.heights.rebuild(count, &|_| estimate, offset);
                    state.absorb(observed, change);
                    Some(change)
                })
                .flatten();
            // The window memo reads this one, so only the scroll is left to do.
            if let Some(change) = change {
                adjust(viewport, change.shift);
            }

            previous.map_or(0, |generation| generation.wrapping_add(1))
        });

        Self {
            state,
            counted,
            changed: Trigger::new(),
            scroll,
            viewport,
        }
    }

    /// The window over these heights, recomputed whenever the scroll position or a height changes.
    pub(crate) fn window(self, overscan: usize) -> Memo<MeasuredWindow> {
        Memo::new(move |_| {
            let (Some(position), Some(_)) = (self.scroll.try_get(), self.counted.try_get()) else {
                return MeasuredWindow::default();
            };
            self.changed.track();

            let scale = css_scale(self.viewport);
            let observed = position.offset.y.0 / scale;
            self.state
                .try_update_value(|state| {
                    // An offset other than the one the scroll was asked from has seen it, or been
                    // overtaken by another. Either way it no longer stands for this offset.
                    if state
                        .pending
                        .is_some_and(|pending| pending.from != observed)
                    {
                        state.pending = None;
                    }

                    let offset = state.rows_offset(observed);
                    state.heights.window(
                        position.scrollport.height.0 / scale,
                        offset as f32,
                        overscan,
                    )
                })
                .unwrap_or_default()
        })
    }

    /// Records the height the layout gave built row `index`, in CSS pixels.
    pub(crate) fn record(self, index: usize, height: f32) {
        self.change(|heights, offset| heights.record(index, height, offset));
    }

    /// Forgets that row `index` is built.
    pub(crate) fn release(self, index: usize) {
        self.state
            .try_update_value(|state| state.heights.release(index));
    }

    /// Gives row `index` its estimate again, unless it is built.
    pub(crate) fn invalidate(self, index: usize) {
        let Some(estimate) = self.estimate() else {
            return;
        };
        self.change(|heights, offset| heights.invalidate(index, estimate, offset));
    }

    /// Gives every row that is not built its estimate again.
    pub(crate) fn reset(self) {
        self.counted.try_get_untracked();
        let observed = self.observed();
        let change = self
            .state
            .try_update_value(|state| {
                let estimate = state.estimate.read(false)?;
                let offset = state.rows_offset(observed);
                let change = state.heights.reset(&|_| estimate, offset);
                state.absorb(observed, change);
                Some(change)
            })
            .flatten();
        self.apply(change);
    }

    /// Where row `index` sits in the container, in CSS pixels, or `None` while there are no rows.
    pub(crate) fn offset_of(self, index: usize) -> Option<f32> {
        self.counted.try_get_untracked()?;
        self.state
            .try_with_value(|state| {
                (state.heights.len() > 0).then(|| state.origin + state.heights.offset_of(index))
            })
            .flatten()
    }

    /// The height row `index` has now, in CSS pixels, or `None` when there is no such row.
    pub(crate) fn height_of(self, index: usize) -> Option<f32> {
        self.counted.try_get_untracked()?;
        self.state
            .try_with_value(|state| state.heights.height(index))
            .flatten()
    }

    /// How far below the top of the container the first row starts, in CSS pixels.
    pub(crate) fn origin(self) -> f32 {
        self.state
            .try_with_value(|state| state.origin)
            .unwrap_or(0.0)
    }

    /// Sets the space above the first row, in CSS pixels.
    ///
    /// `fill` is space that exists only while the rows are shorter than the port, and `head` is the
    /// box above the rows. When `head` changes height and the port shows no part of the space, the
    /// container scrolls by the change, so the rows on screen stay where they are.
    pub(crate) fn set_origin(self, fill: f32, head: f32) {
        // Counted first, so that a recount does not answer from the old origin.
        self.counted.try_get_untracked();
        let observed = self.observed();
        let change = self
            .state
            .try_update_value(|state| {
                let origin = fill + head;
                if state.origin == origin && state.head == head {
                    return None;
                }

                let shift = if anchored(state.pending, observed) >= f64::from(state.origin) {
                    f64::from(head) - f64::from(state.head)
                } else {
                    0.0
                };
                state.origin = origin;
                state.head = head;
                let change = Change { moved: true, shift };
                state.absorb(observed, change);
                Some(change)
            })
            .flatten();
        self.apply(change);
    }

    /// Whether every scroll the list asked for to keep its first visible row in place has been
    /// seen. Read without subscribing.
    pub(crate) fn settled(self) -> bool {
        self.counted.try_get_untracked();
        let observed = self.observed();
        self.state
            .try_with_value(|state| state.pending.is_none_or(|pending| pending.from != observed))
            .unwrap_or(true)
    }

    /// The offset of the top of the port, in CSS pixels, with the scrolls the list asked for and
    /// has not yet seen applied. Read without subscribing.
    pub(crate) fn offset(self) -> f64 {
        self.counted.try_get_untracked();
        let observed = self.observed();
        self.state
            .try_with_value(|state| anchored(state.pending, observed))
            .unwrap_or(f64::from(observed))
    }

    /// The offset of the top of the port less every scroll the list asked for to keep its first
    /// visible row in place, in CSS pixels. Read without subscribing.
    ///
    /// Only a scroll that the list did not ask for changes it.
    pub(crate) fn drift(self) -> f64 {
        self.counted.try_get_untracked();
        let observed = self.observed();
        self.state
            .try_with_value(|state| anchored(state.pending, observed) - state.shifted)
            .unwrap_or(f64::from(observed))
    }

    /// The estimate a row is given, read without subscribing.
    fn estimate(self) -> Option<f32> {
        self.state
            .try_with_value(|state| state.estimate.read(false))
            .flatten()
    }

    /// Changes the heights, and keeps the first visible row in place on screen.
    fn change(self, change: impl FnOnce(&mut RowHeights, f64) -> Change) {
        // Counted before the change, so that an index names the row it names now.
        self.counted.try_get_untracked();
        let observed = self.observed();
        let change = self.state.try_update_value(|state| {
            let offset = state.rows_offset(observed);
            let change = change(&mut state.heights, offset);
            state.absorb(observed, change);
            change
        });
        self.apply(change);
    }

    /// Wakes the window and scrolls the container for what a change did.
    fn apply(self, change: Option<Change>) {
        let Some(change) = change else {
            return;
        };

        if change.moved {
            self.changed.notify();
        }
        adjust(self.viewport, change.shift);
    }

    /// The scroll offset the container last reported, in CSS pixels, without subscribing.
    fn observed(self) -> f32 {
        observed(self.scroll, self.viewport)
    }
}

/// The scroll offset `viewport` last reported in `scroll`, in CSS pixels, without subscribing.
fn observed(scroll: Signal<ScrollPosition, LocalStorage>, viewport: NodeRef) -> f32 {
    // A position that is gone belongs to a list that is being taken down.
    let offset = scroll
        .try_get_untracked()
        .map_or(0.0, |position| position.offset.y.0);
    offset / css_scale(viewport)
}

/// Scrolls `viewport` down by `shift` CSS pixels, keeping the motion of a scroll in progress.
fn adjust(viewport: NodeRef, shift: f64) {
    if shift != 0.0 {
        let by = shift as f32 * css_scale(viewport);
        viewport.scroll_to(
            ScrollTarget::Adjust(Point::new(DevicePx(0.0), DevicePx(by))),
            ScrollBehavior::Instant,
        );
    }
}

/// The offset a window is decided at: the observed one, moved by a scroll asked from it.
fn anchored(pending: Option<Pending>, observed: f32) -> f64 {
    match pending {
        Some(pending) if pending.from == observed => f64::from(observed) + pending.shift,
        _ => f64::from(observed),
    }
}

/// The scroll still unobserved after `change` asks for its shift from `observed`.
fn follow(pending: Option<Pending>, observed: f32, change: Change) -> Option<Pending> {
    if change.shift == 0.0 {
        return pending;
    }

    let before = match pending {
        Some(pending) if pending.from == observed => pending.shift,
        _ => 0.0,
    };
    Some(Pending {
        from: observed,
        shift: before + change.shift,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::collections::BTreeMap;

    use zgui::geom::{DevicePx, Point, Size};
    use zgui::prelude::*;
    use zgui::reactive::{LocalStorage, Mounted, RwSignal};
    use zgui::view::{
        Dom, ElementName, NodeId, ObservedValue, ScrollBehavior, ScrollPosition, ScrollTarget,
    };
    use zgui_testkit_view::Window;

    use crate::virtualize::{MeasuredRows, MeasuredVirtualize};

    /// How tall the port is, in device pixels.
    const PORT: f32 = 400.0;

    /// Rows 0, 4, 8 and so on are 60px tall, and the rest 20px.
    fn every_fourth(index: usize) -> f32 {
        if index.is_multiple_of(4) { 60.0 } else { 20.0 }
    }

    /// Even rows are 50px tall, and odd rows 20px.
    fn even(index: usize) -> f32 {
        if index.is_multiple_of(2) { 50.0 } else { 20.0 }
    }

    /// A measured list of ten thousand rows estimated at 20px, and a layout played by the test.
    ///
    /// The test lays the built rows out by delivering their sizes, and carries out the scrolls the
    /// list asks for by delivering the scroll position they lead to, as a window does.
    struct List {
        /// The window it is built in.
        window: Window,
        /// The scroll container.
        port: NodeId,
        /// How many rows there are.
        rows: RwSignal<usize, LocalStorage>,
        /// The list.
        seen: MeasuredVirtualize,
        /// The element and the scope of each row that is built.
        built: BTreeMap<usize, (NodeId, Mounted)>,
        /// The scroll offset last delivered, in device pixels.
        offset: Cell<f32>,
        /// How many of the host's scroll commands have been carried out.
        carried: Cell<usize>,
    }

    impl List {
        /// The list, with `overscan` rows of slack, scrolled to its top.
        fn open(overscan: usize) -> Self {
            Self::open_with(overscan, || MeasuredRows::new(20.0))
        }

        /// The same, sized as `measured` says. It is called in the window's scope.
        fn open_with(overscan: usize, measured: impl FnOnce() -> MeasuredRows) -> Self {
            let window = Window::open();
            let port = window.dom.create_element(ElementName::new("scroll"));
            window.dom.insert(window.root, port, None);

            let handle = window.scope.with(NodeRef::new);
            let rows = window.scope.with(|| RwSignal::new_local(10_000_usize));
            let seen = window
                .scope
                .with(|| MeasuredVirtualize::new(handle, rows.into(), measured(), overscan));
            handle.bind(port, &window.dom_handle, &window.host_handle);
            window.frame();

            let list = Self {
                window,
                port,
                rows,
                seen,
                built: BTreeMap::new(),
                offset: Cell::new(0.0),
                carried: Cell::new(0),
            };
            list.scroll(0.0);
            list
        }

        /// Reports the port scrolled to `offset`.
        fn scroll(&self, offset: f32) {
            self.offset.set(offset);
            self.window.dom.deliver(
                self.port,
                ObservedValue::ScrollPosition(ScrollPosition {
                    offset: Point::new(DevicePx(0.0), DevicePx(offset)),
                    content_size: Size::new(
                        DevicePx(200.0),
                        DevicePx(self.seen.window_untracked().total()),
                    ),
                    scrollport: Size::new(DevicePx(200.0), DevicePx(PORT)),
                }),
            );
            self.window.frame();
        }

        /// Carries out every scroll the list has asked for since the last call.
        fn carry_out_scrolls(&self) {
            let commands = self.window.host.scroll_commands();
            let mut offset = self.offset.get();
            for (_, target, _) in &commands[self.carried.get()..] {
                offset = match target {
                    ScrollTarget::Adjust(by) => offset + by.y.0,
                    ScrollTarget::Offset(to) => to.y.0,
                    other => panic!("the list asked for {other:?}"),
                };
            }
            self.carried.set(commands.len());
            if offset != self.offset.get() {
                self.scroll(offset);
            }
        }

        /// The scroll commands the list has issued, in order.
        fn scrolls(&self) -> Vec<ScrollTarget> {
            self.window
                .host
                .scroll_commands()
                .into_iter()
                .map(|(_, target, _)| target)
                .collect()
        }

        /// Builds each row of the window that is not built yet and lays it out at `height(index)`,
        /// until the window builds nothing new.
        fn lay_out(&mut self, height: impl Fn(usize) -> f32) {
            for _ in 0..16 {
                let fresh: Vec<usize> = self
                    .seen
                    .window_untracked()
                    .indices()
                    .into_iter()
                    .filter(|index| !self.built.contains_key(index))
                    .collect();
                if fresh.is_empty() {
                    return;
                }

                for index in &fresh {
                    let element = self.window.dom.create_element(ElementName::new("box"));
                    self.window.dom.insert(self.port, element, None);
                    let scope = self.window.scope.with(Mounted::new);
                    let row = scope.with(NodeRef::new);
                    scope.with(|| self.seen.measure(*index, row));
                    row.bind(element, &self.window.dom_handle, &self.window.host_handle);
                    self.built.insert(*index, (element, scope));
                }
                self.window.frame();

                for index in fresh {
                    self.resize(index, height(index));
                }
                self.carry_out_scrolls();
            }
            panic!("the window never stopped building rows");
        }

        /// Lays built row `index` out at `height`.
        fn resize(&self, index: usize, height: f32) {
            self.window.dom.deliver(
                self.built[&index].0,
                ObservedValue::BorderSize(Size::new(DevicePx(200.0), DevicePx(height))),
            );
            self.window.frame();
        }

        /// Where row `index` is on screen, from the top of the port.
        fn on_screen(&self, index: usize) -> f32 {
            self.seen.offset_of(index).expect("the list has rows") - self.offset.get()
        }

        /// Takes down every built row outside the window, as a keyed list does.
        fn take_down_outside(&mut self) {
            let seen = self.seen.window_untracked();
            let outside: Vec<usize> = self
                .built
                .keys()
                .copied()
                .filter(|index| !seen.contains(*index))
                .collect();
            for index in outside {
                if let Some((_, scope)) = self.built.remove(&index) {
                    scope.unmount();
                }
            }
            self.window.frame();
        }
    }

    #[test]
    fn rows_of_mixed_heights_give_the_extent_and_the_window_once_measured() {
        let mut list = List::open(2);
        list.lay_out(every_fourth);

        let seen = list.seen.window_untracked();
        // Every four rows are 120px, so rows 0 to 12 start inside the 400px port.
        assert_eq!(
            (seen.first, seen.end()),
            (0, 15),
            "thirteen rows and two of slack"
        );
        assert_eq!(seen.lead, 0.0);
        assert_eq!(seen.body, 4.0 * 60.0 + 11.0 * 20.0);

        let whole: f32 = (0..10_000)
            .map(|index| {
                if list.built.contains_key(&index) {
                    every_fourth(index)
                } else {
                    20.0
                }
            })
            .sum();
        assert_eq!(
            seen.total(),
            whole,
            "a built row counts at its measurement and the others at the estimate"
        );
        assert!(
            list.scrolls().is_empty(),
            "nothing above the top of the list moves the list"
        );
    }

    #[test]
    fn a_row_that_expands_above_the_port_leaves_the_first_visible_row_where_it_was() {
        let mut list = List::open(2);
        // Ten pixels into row 100.
        list.scroll(2_010.0);
        list.lay_out(|_| 20.0);
        assert_eq!(list.seen.window_untracked().first, 98);
        assert_eq!(list.on_screen(100), -10.0);

        list.resize(99, 140.0);
        assert_eq!(
            list.scrolls().last(),
            Some(&ScrollTarget::Adjust(Point::new(
                DevicePx(0.0),
                DevicePx(120.0)
            ))),
            "the list scrolls by what row 99 grew",
        );
        assert_eq!(
            list.seen.window_untracked().first,
            98,
            "the window already stands at the offset the list asked for"
        );

        list.carry_out_scrolls();
        assert_eq!(list.offset.get(), 2_130.0);
        assert_eq!(
            list.on_screen(100),
            -10.0,
            "row 100 has not moved on screen"
        );

        let asked = list.scrolls().len();
        list.resize(100, 50.0);
        list.resize(104, 90.0);
        assert_eq!(
            list.scrolls().len(),
            asked,
            "the first visible row and the rows below it grow down from where they are"
        );
        assert_eq!(list.on_screen(100), -10.0);
    }

    #[test]
    fn a_jump_to_a_row_lands_on_that_row_and_stays_there_as_it_is_measured() {
        let mut list = List::open(2);
        list.lay_out(even);
        let target = list.seen.offset_of(5_000).expect("the list has rows");
        assert!(
            target > 5_000.0 * 20.0,
            "the rows measured at the top are taller than the estimate"
        );

        list.seen.scroll_to_row(5_000, ScrollBehavior::Instant);
        assert_eq!(
            list.scrolls().last(),
            Some(&ScrollTarget::Offset(Point::new(
                DevicePx(0.0),
                DevicePx(target)
            ))),
        );
        list.carry_out_scrolls();
        assert_eq!(list.seen.window_untracked().first, 4_998);

        // Row 4 998 is measured 30px taller than its estimate, above the row the list jumped to.
        list.lay_out(even);
        assert_eq!(list.seen.window_untracked().first, 4_998);
        assert_eq!(list.offset.get(), target + 30.0);
        assert_eq!(
            list.on_screen(5_000),
            0.0,
            "row 5 000 is at the top of the port"
        );
    }

    #[test]
    fn a_count_that_changes_counts_the_heights_again() {
        let mut list = List::open(2);
        list.lay_out(every_fourth);
        let built = list.built.len();
        let measured: f32 = list.built.keys().map(|index| every_fourth(*index)).sum();
        assert!(built < 30);

        list.rows.set(30);
        list.window.frame();
        let seen = list.seen.window_untracked();
        assert_eq!(seen.rows, 30);
        assert_eq!(
            seen.total(),
            measured + (30 - built) as f32 * 20.0,
            "the measured rows keep their heights and the rest are estimated"
        );
    }

    #[test]
    fn a_row_given_its_estimate_again_leaves_the_first_visible_row_where_it_was() {
        let mut list = List::open(2);
        list.lay_out(every_fourth);
        let top = list.built.len();

        // Five pixels into row 100, with the rows measured at the top taken down.
        let row = list.seen.offset_of(100).expect("the list has rows");
        list.scroll(row + 5.0);
        list.lay_out(every_fourth);
        list.take_down_outside();
        assert!(list.built.keys().all(|index| *index >= top));
        assert_eq!(list.on_screen(100), -5.0);

        list.seen.invalidate(4);
        assert_eq!(
            list.scrolls().last(),
            Some(&ScrollTarget::Adjust(Point::new(
                DevicePx(0.0),
                DevicePx(-40.0)
            ))),
            "row 4 is 20px again"
        );
        list.carry_out_scrolls();
        assert_eq!(list.on_screen(100), -5.0);

        list.seen.reset();
        list.carry_out_scrolls();
        assert_eq!(
            list.seen.offset_of(100),
            Some(100.0 * 20.0),
            "every row above it is at its estimate again"
        );
        assert_eq!(list.on_screen(100), -5.0);
    }

    #[test]
    fn an_estimate_that_changes_leaves_the_first_visible_row_where_it_was() {
        let handed = std::rc::Rc::new(Cell::new(None));
        let estimate = std::rc::Rc::clone(&handed);
        let mut list = List::open_with(2, move || {
            let height = RwSignal::new_local(20.0_f32);
            estimate.set(Some(height));
            MeasuredRows::new(height)
        });
        let estimate = handed.get().expect("the list was opened");

        // Five pixels into row 100, with every row above it unmeasured.
        list.scroll(100.0 * 20.0 + 5.0);
        list.lay_out(|_| 20.0);
        list.take_down_outside();
        assert!(list.built.keys().all(|index| *index >= 98));
        assert_eq!(list.on_screen(100), -5.0);

        estimate.set(30.0);
        // Read as the view reads it, which counts the list again.
        let _ = list.seen.window_untracked();
        assert_eq!(
            list.scrolls().last(),
            Some(&ScrollTarget::Adjust(Point::new(
                DevicePx(0.0),
                DevicePx(98.0 * 10.0)
            ))),
            "rows 0 to 97 are each 10px taller"
        );
        list.carry_out_scrolls();
        assert_eq!(list.on_screen(100), -5.0);
    }

    #[test]
    fn a_keyed_measurement_follows_its_row_when_the_data_is_reordered() {
        let handed = std::rc::Rc::new(Cell::new(None));
        let order = std::rc::Rc::clone(&handed);
        let mut list = List::open_with(2, move || {
            let keys = RwSignal::new_local((0..10_000_u32).collect::<Vec<_>>());
            order.set(Some(keys));
            MeasuredRows::new(20.0).keyed(move |index| keys.with(|keys| keys[index]))
        });
        let keys = handed.get().expect("the list was opened");

        // Keys 0 to 21 are measured at the top, and taken down once the port is far below them.
        list.lay_out(every_fourth);
        let measured = list.built.len();
        list.scroll(100_000.0);
        list.lay_out(|_| 20.0);
        list.take_down_outside();
        let total = list.seen.window_untracked().total();

        keys.update(|keys| keys.reverse());
        list.window.frame();
        assert_eq!(
            list.seen.offset_of(measured),
            Some(measured as f32 * 20.0),
            "the rows at the top hold keys nobody measured"
        );
        let tail: f32 = (0..measured).map(every_fourth).sum();
        let start = 10_000 - measured;
        assert_eq!(
            list.seen.window_untracked().total() - list.seen.offset_of(start).unwrap_or(0.0),
            tail,
            "the measured keys took their heights to the end of the list"
        );
        assert_eq!(list.seen.window_untracked().total(), total);
    }

    #[test]
    fn a_return_to_the_offset_an_adjustment_was_asked_from_shows_the_rows_there() {
        let mut list = List::open(2);
        // Ten pixels into row 100, and row 99 grows by 120px.
        list.scroll(2_010.0);
        list.lay_out(|_| 20.0);
        list.resize(99, 140.0);
        list.carry_out_scrolls();
        assert_eq!(list.offset.get(), 2_130.0);
        assert_eq!(list.seen.window_untracked().first, 98);

        // Back at 2 010px, which is now 30px into row 99.
        list.scroll(2_010.0);
        assert_eq!(
            list.seen.window_untracked().first,
            97,
            "the adjustment was seen, and the offset stands for itself"
        );
    }

    #[test]
    fn space_above_the_rows_that_changes_above_the_port_leaves_the_rows_in_place() {
        let mut list = List::open(2);
        // Ten pixels into row 100.
        list.scroll(2_010.0);
        list.lay_out(|_| 20.0);
        let measured = list.seen.measured();

        measured.set_origin(0.0, 40.0);
        assert_eq!(
            list.scrolls().last(),
            Some(&ScrollTarget::Adjust(Point::new(
                DevicePx(0.0),
                DevicePx(40.0)
            ))),
        );
        assert_eq!(list.seen.window_untracked().first, 98);
        list.carry_out_scrolls();
        assert_eq!(
            list.on_screen(100),
            -10.0,
            "row 100 has not moved on screen"
        );
        assert_eq!(list.seen.window_untracked().first, 98);

        // At the top the port shows the space, and the rows below it move with it.
        list.scroll(0.0);
        let asked = list.scrolls().len();
        measured.set_origin(0.0, 0.0);
        assert_eq!(list.scrolls().len(), asked);
        assert_eq!(list.seen.offset_of(0), Some(0.0));
    }

    #[test]
    fn space_that_fills_a_short_list_moves_nothing_when_it_goes() {
        let mut list = List::open(2);
        list.lay_out(|_| 20.0);
        let measured = list.seen.measured();
        measured.set_origin(75.0, 0.0);
        let asked = list.scrolls().len();

        // The rows grew past the port, and the layout that dropped the fill scrolled to the end.
        list.scroll(125.0);
        measured.set_origin(0.0, 0.0);
        assert_eq!(
            list.scrolls().len(),
            asked,
            "nothing was scrolled past the fill"
        );
        assert_eq!(list.seen.offset_of(0), Some(0.0));
    }

    #[test]
    fn only_a_scroll_the_list_did_not_ask_for_moves_the_drift() {
        let mut list = List::open(2);
        list.scroll(2_010.0);
        list.lay_out(|_| 20.0);
        let measured = list.seen.measured();
        let before = measured.drift();

        list.resize(99, 140.0);
        assert_eq!(measured.drift(), before, "the adjustment is asked for");
        list.carry_out_scrolls();
        assert_eq!(measured.drift(), before, "and seen");

        list.scroll(list.offset.get() - 30.0);
        assert_eq!(measured.drift(), before - 30.0);
    }

    #[test]
    fn a_jump_to_a_row_scrolls_in_device_pixels() {
        let window = Window::open();
        window.host.set_scale(2.0);
        let port = window.dom.create_element(ElementName::new("scroll"));
        window.dom.insert(window.root, port, None);

        let handle = window.scope.with(NodeRef::new);
        let rows = window.scope.with(|| RwSignal::new_local(10_000_usize));
        let seen = window
            .scope
            .with(|| MeasuredVirtualize::new(handle, rows.into(), MeasuredRows::new(20.0), 2));
        handle.bind(port, &window.dom_handle, &window.host_handle);
        window.dom.deliver(
            port,
            ObservedValue::ScrollPosition(ScrollPosition {
                offset: Point::new(DevicePx(0.0), DevicePx(0.0)),
                content_size: Size::new(DevicePx(200.0), DevicePx(400_000.0)),
                scrollport: Size::new(DevicePx(200.0), DevicePx(800.0)),
            }),
        );
        window.frame();

        assert_eq!(seen.offset_of(500), Some(10_000.0));
        seen.scroll_to_row(500, ScrollBehavior::Instant);
        assert_eq!(
            window
                .host
                .scroll_commands()
                .last()
                .map(|(_, target, _)| *target),
            Some(ScrollTarget::Offset(Point::new(
                DevicePx(0.0),
                DevicePx(20_000.0)
            ))),
        );
    }
}
