//! Building only the rows somebody can see, when the rows differ in height.

use zgui::geom::{DevicePx, Point};
use zgui::prelude::*;
use zgui::reactive::{LocalStorage, RenderEffect};
use zgui::view::{ScrollBehavior, ScrollTarget};

use crate::virtualize::css_scale;
use crate::virtualize::measure::MeasuredRows;
use crate::virtualize::measured::Measured;
use crate::virtualize::measured_window::MeasuredWindow;

/// The rows of a long list whose rows differ in height that are worth building, followed live.
///
/// The counterpart of [`Virtualize`](crate::virtualize::Virtualize) for rows that differ in height
/// and are measured. Each row has an estimate until it is built, and the height its layout reports
/// once it is. The window is decided from these heights, so a row that has not been built
/// stands at its estimate in the scroll extent. A view that builds its own rows calls
/// [`MeasuredVirtualize::measure`] for each of them:
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::virtualize::{MeasuredRows, MeasuredVirtualize};
///
/// /// Rows whose text wraps to as many lines as it needs.
/// #[component]
/// fn Wrapped() -> impl IntoView {
///     let port = NodeRef::new();
///     let rows = RwSignal::new_local(10_000_usize);
///     let seen = MeasuredVirtualize::new(port, rows.into(), MeasuredRows::new(24.0), 4);
///
///     view! {
///         scroll(node_ref = port) {
///             column(
///                 style:padding-top = move || Some(format!("{}px", seen.window().lead)),
///                 style:padding-bottom = move || Some(format!("{}px", seen.window().trail))
///             ) {
///                 for index in move || seen.window().indices(), key = |index: &usize| *index {
///                     Line(index = index, seen = seen)
///                 }
///             }
///         }
///     }
/// }
///
/// /// One row, measured while it is built.
/// #[component]
/// fn Line(index: usize, seen: MeasuredVirtualize) -> impl IntoView {
///     let row = NodeRef::new();
///     seen.measure(index, row);
///     view! { text(node_ref = row) {{format!("row {index}")}} }
/// }
/// ```
///
/// When a row above the first visible one changes height, the container scrolls by the same
/// amount, so the rows on screen stay where they are. In a keyed list the same holds when rows are
/// inserted or removed above it. That scroll does not stop a smooth scroll or a fling the
/// container is on. A height reaches the window in the frame that lays the row out.
#[derive(Copy, Clone)]
pub struct MeasuredVirtualize {
    /// The window, recomputed whenever the scroll position, the row count or a height changes.
    window: Memo<MeasuredWindow>,
    /// The heights.
    measured: Measured,
    /// The scroll container.
    viewport: NodeRef,
}

impl MeasuredVirtualize {
    /// Watches `viewport`, a scroll container holding `rows` rows that differ in height.
    ///
    /// Each row has the estimate `measured` gives it until it is built, and the height its layout
    /// reports once [`MeasuredVirtualize::measure`] watches it. `overscan` is how many rows to
    /// build beyond each edge of the port.
    ///
    /// The observation of the scroll position is taken when `viewport` binds, as for
    /// [`Virtualize::new`](crate::virtualize::Virtualize::new). The cost of the heights is in
    /// [`MeasuredRows`].
    #[must_use]
    pub fn new(
        viewport: NodeRef,
        rows: Signal<usize, LocalStorage>,
        measured: MeasuredRows,
        overscan: usize,
    ) -> Self {
        let measured = Measured::new(viewport, rows, measured, viewport.observe_scroll());
        Self {
            window: measured.window(overscan),
            measured,
            viewport,
        }
    }

    /// The rows worth building right now, subscribing to it.
    #[must_use]
    pub fn window(&self) -> MeasuredWindow {
        self.window.try_get().unwrap_or_default()
    }

    /// The same, without subscribing.
    #[must_use]
    pub fn window_untracked(&self) -> MeasuredWindow {
        self.window.try_get_untracked().unwrap_or_default()
    }

    /// The indices worth building right now, which is what a keyed list is driven by.
    #[must_use]
    pub fn indices(&self) -> Vec<usize> {
        self.window().indices()
    }

    /// Measures row `index` from the layout of the element `row` binds to.
    ///
    /// Call it from the scope that builds the row. The row is measured while that scope lives,
    /// and keeps its last measurement after the scope is gone. The element must stand for row
    /// `index` for as long as it lives, as it does in a list keyed by index.
    ///
    /// The row's border box is measured, so a row has no vertical margin.
    pub fn measure(&self, index: usize, row: NodeRef) {
        let measured = self.measured;
        let viewport = self.viewport;
        let size = row.observe_border_size();
        let watching = RenderEffect::new(move |_| {
            if let Some(size) = size.get() {
                measured.record(index, size.height.0 / css_scale(viewport));
            }
        });
        on_cleanup_local(move || {
            drop(watching);
            measured.release(index);
        });
    }

    /// Gives row `index` its estimate again.
    ///
    /// For a row whose content changed while it was not built, so that its last measurement no
    /// longer holds. A built row keeps the height its layout reports.
    pub fn invalidate(&self, index: usize) {
        self.measured.invalidate(index);
    }

    /// Gives every row that is not built its estimate again.
    pub fn reset(&self) {
        self.measured.reset();
    }

    /// Where row `index` sits in the container, in CSS pixels, without subscribing.
    ///
    /// The rows before `index` count at their measurements where they have them and at their
    /// estimates where they do not. `None` while the list has no rows.
    #[must_use]
    pub fn offset_of(&self, index: usize) -> Option<f32> {
        self.measured.offset_of(index)
    }

    /// The heights, for a list built on this one.
    pub(crate) const fn measured(&self) -> Measured {
        self.measured
    }

    /// Scrolls the container so that row `index` starts at the top of the port.
    ///
    /// The offset is where the row sits when this is called. With an instant scroll, the rows
    /// above `index` that are built and measured on arrival move the container on to keep row
    /// `index` at the top. A smooth scroll heads for that offset, and rows measured on the way that
    /// are not above the port by then can leave it short of row `index` or past it.
    pub fn scroll_to_row(&self, index: usize, behavior: ScrollBehavior) {
        let Some(offset) = self.offset_of(index) else {
            return;
        };

        let across = self.viewport.scroll_offset().x;
        let down = DevicePx(offset * css_scale(self.viewport));
        self.viewport
            .scroll_to(ScrollTarget::Offset(Point::new(across, down)), behavior);
    }
}
