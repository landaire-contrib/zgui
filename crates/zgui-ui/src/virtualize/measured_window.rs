//! Which rows of a list whose rows differ in height are worth building.

/// The rows a [`MeasuredVirtualize`](crate::virtualize::MeasuredVirtualize) builds, and the space
/// that replaces the ones it did not.
///
/// The counterpart of [`VirtualWindow`](crate::virtualize::VirtualWindow) for rows that differ in
/// height: there is no one row height, so the height of the built rows is carried in [`body`].
/// Every length is in CSS pixels.
///
/// ```
/// use zgui_ui::virtualize::MeasuredWindow;
///
/// // Rows 10 to 29 are built, under 400px of rows above and over 1200px of rows below.
/// let seen = MeasuredWindow {
///     rows: 100,
///     first: 10,
///     count: 20,
///     lead: 400.0,
///     trail: 1_200.0,
///     body: 900.0,
///     compressed: false,
/// };
/// assert_eq!(seen.end(), 30);
/// assert_eq!(seen.total(), 2_500.0);
/// ```
///
/// [`body`]: MeasuredWindow::body
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub struct MeasuredWindow {
    /// How many rows the list has, built and unbuilt alike.
    pub rows: usize,
    /// The index of the first row that is built.
    pub first: usize,
    /// How many rows are built, counting from [`MeasuredWindow::first`].
    pub count: usize,

    /// The height of everything before the first built row, in CSS pixels.
    pub lead: f32,
    /// The height of everything after the last built row, in CSS pixels.
    pub trail: f32,
    /// The height of the built rows together, in CSS pixels.
    pub body: f32,

    /// Whether the rows together are taller than [`MAX_EXTENT`], so that the container stands for
    /// more than its height can hold one by one.
    ///
    /// [`MAX_EXTENT`]: crate::virtualize::MAX_EXTENT
    pub compressed: bool,
}

impl MeasuredWindow {
    /// The indices that are built, in order.
    #[must_use]
    pub fn indices(&self) -> Vec<usize> {
        (self.first..self.end()).collect()
    }

    /// Whether `index` is one of the rows that is built.
    #[must_use]
    pub const fn contains(&self, index: usize) -> bool {
        index >= self.first && index < self.end()
    }

    /// The index one past the last built row.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.first + self.count
    }

    /// The height of the whole list, built and unbuilt alike, in CSS pixels.
    ///
    /// That is [`MAX_EXTENT`](crate::virtualize::MAX_EXTENT) for a compressed list.
    #[must_use]
    pub fn total(&self) -> f32 {
        self.lead + self.body + self.trail
    }
}
