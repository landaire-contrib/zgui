//! The height of each row of a list whose rows differ in height.

use std::collections::HashMap;

use crate::virtualize::measure::RowKeys;
use crate::virtualize::measured_window::MeasuredWindow;
use crate::virtualize::sums::PrefixSums;
use crate::virtualize::window::{MAX_EXTENT, compressed_offset, length};

/// The height of every row of a measured list, and the running totals over them.
pub(crate) struct RowHeights {
    /// Each row's height in CSS pixels: its measurement, or its estimate.
    heights: Vec<f32>,
    /// Whether each row's height is a measurement.
    measured: Vec<bool>,
    /// The running totals over `heights`.
    sums: PrefixSums,

    /// The rows that are built, with the height their layout last reported.
    ///
    /// A built row keeps this height when the list is counted again, because its layout keeps it
    /// until it reports another.
    built: HashMap<usize, f32>,
    /// Where measurements are held per key, when they are.
    keys: Option<Box<dyn RowKeys>>,
}

/// What a change to the heights did.
#[derive(Copy, Clone, PartialEq, Debug, Default)]
pub(crate) struct Change {
    /// Whether a height changed.
    pub(crate) moved: bool,
    /// How far down the first visible row moved, in CSS pixels.
    ///
    /// The scroll offset moves by this much to keep the row where it was on screen.
    pub(crate) shift: f64,
}

impl RowHeights {
    /// A list of no rows, holding measurements in `keys` when it is given.
    pub(crate) fn new(keys: Option<Box<dyn RowKeys>>) -> Self {
        Self {
            heights: Vec::new(),
            measured: Vec::new(),
            sums: PrefixSums::default(),
            built: HashMap::new(),
            keys,
        }
    }

    /// How many rows there are.
    pub(crate) fn len(&self) -> usize {
        self.heights.len()
    }

    /// Counts the list again at `rows` rows, and estimates each row that holds no measurement.
    ///
    /// `offset` is the scroll offset, and the change answers how far it moved the row there.
    pub(crate) fn rebuild(
        &mut self,
        rows: usize,
        estimate: &dyn Fn(usize) -> f32,
        offset: f64,
    ) -> Change {
        self.anchored(offset, |heights| {
            heights.recount(rows, estimate);
            true
        })
    }

    /// Sets the heights of `rows` rows from the held measurements and `estimate`.
    fn recount(&mut self, rows: usize, estimate: &dyn Fn(usize) -> f32) {
        if let Some(keys) = self.keys.as_mut() {
            keys.bind(rows);
            self.heights.clear();
            self.measured.clear();
            for index in 0..rows {
                let held = keys.height(index);
                self.measured.push(held.is_some());
                self.heights.push(held.unwrap_or_else(|| estimate(index)));
            }
        } else {
            self.heights.truncate(rows);
            self.measured.truncate(rows);
            for (index, (height, measured)) in
                self.heights.iter_mut().zip(&self.measured).enumerate()
            {
                if !measured {
                    *height = estimate(index);
                }
            }
            for index in self.heights.len()..rows {
                self.heights.push(estimate(index));
                self.measured.push(false);
            }
        }

        // A built row keeps its laid-out height under whichever key it holds now. Its layout
        // reports nothing new when the key changes and the height does not.
        for (index, height) in &self.built {
            if *index < rows {
                self.heights[*index] = *height;
                self.measured[*index] = true;
                if let Some(keys) = self.keys.as_mut() {
                    keys.record(*index, *height);
                }
            }
        }

        self.sums = PrefixSums::new(&self.heights);
    }

    /// Records the height the layout gave built row `index`.
    ///
    /// `offset` is the scroll offset, and the change answers how far it moved the row there.
    pub(crate) fn record(&mut self, index: usize, height: f32, offset: f64) -> Change {
        self.built.insert(index, height);
        if index >= self.len() {
            return Change::default();
        }

        if let Some(keys) = self.keys.as_mut() {
            keys.record(index, height);
        }

        self.anchored(offset, |heights| heights.set(index, height, true))
    }

    /// Forgets that row `index` is built. Its measurement stays.
    pub(crate) fn release(&mut self, index: usize) {
        self.built.remove(&index);
    }

    /// Gives row `index` the height `estimate` again, unless it is built.
    pub(crate) fn invalidate(&mut self, index: usize, estimate: f32, offset: f64) -> Change {
        if index >= self.len() || self.built.contains_key(&index) {
            return Change::default();
        }

        if let Some(keys) = self.keys.as_mut() {
            keys.forget(index);
        }

        self.anchored(offset, |heights| heights.set(index, estimate, false))
    }

    /// Gives every row that is not built its estimate again.
    pub(crate) fn reset(&mut self, estimate: &dyn Fn(usize) -> f32, offset: f64) -> Change {
        if let Some(keys) = self.keys.as_mut() {
            keys.clear();
            for (index, height) in &self.built {
                keys.record(*index, *height);
            }
        }

        self.anchored(offset, |heights| {
            let mut moved = false;
            for index in 0..heights.len() {
                let (height, measured) = match heights.built.get(&index) {
                    Some(height) => (*height, true),
                    None => (estimate(index), false),
                };
                moved |= heights.heights[index] != height;
                heights.heights[index] = height;
                heights.measured[index] = measured;
            }

            heights.sums = PrefixSums::new(&heights.heights);
            moved
        })
    }

    /// Where row `index` sits in the container, in CSS pixels.
    pub(crate) fn offset_of(&self, index: usize) -> f32 {
        if self.is_compressed() {
            return compressed_offset(self.len(), index);
        }

        self.sums.before(index) as f32
    }

    /// The first visible row at `offset`, found as [`window`](Self::window) finds it.
    fn anchor_at(&self, offset: f64) -> usize {
        let rows = self.len();
        if rows > 0 && self.is_compressed() {
            let fraction = (offset / f64::from(MAX_EXTENT)).clamp(0.0, 1.0);
            ((fraction * (rows - 1) as f64) as usize).min(rows - 1)
        } else {
            self.row_at(offset)
        }
    }

    /// The rows worth building for a port `viewport` CSS pixels tall scrolled to `offset`, with
    /// `overscan` rows beyond each edge.
    ///
    /// The measured form of [`window`](crate::virtualize::window): the rows that cover the port
    /// are found from the running totals.
    pub(crate) fn window(&self, viewport: f32, offset: f32, overscan: usize) -> MeasuredWindow {
        let rows = self.len();
        if rows == 0 {
            return MeasuredWindow::default();
        }

        let viewport = f64::from(length(viewport));
        let offset = f64::from(length(offset));
        let total = self.sums.total();

        // As for one row height: past the cap the offset names a fraction of the rows, the row it
        // names sits at the top of the port, and the spacers fill the rest of the capped extent.
        if total > f64::from(MAX_EXTENT) {
            let extent = f64::from(MAX_EXTENT);
            let fraction = (offset / extent).clamp(0.0, 1.0);
            let anchor = ((fraction * (rows - 1) as f64) as usize).min(rows - 1);
            let top = self.sums.before(anchor);
            let first = anchor.saturating_sub(overscan);
            let end = self
                .visible_end(anchor, top + viewport)
                .saturating_add(overscan)
                .min(rows);

            let start = self.sums.before(first);
            let body = self.sums.before(end) - start;
            let room = (extent - body).max(0.0);
            let lead = (offset - (top - start)).clamp(0.0, room);
            return MeasuredWindow {
                rows,
                first,
                count: end - first,
                lead: lead as f32,
                trail: (extent - body - lead).max(0.0) as f32,
                body: body as f32,
                compressed: true,
            };
        }

        let anchor = self.row_at(offset);
        let first = anchor.saturating_sub(overscan);
        let end = self
            .visible_end(anchor, offset + viewport)
            .saturating_add(overscan)
            .min(rows);

        let start = self.sums.before(first);
        let stop = self.sums.before(end);
        MeasuredWindow {
            rows,
            first,
            count: end - first,
            lead: start as f32,
            trail: (total - stop) as f32,
            body: (stop - start) as f32,
            compressed: false,
        }
    }

    /// Writes one row's height. Answers whether it changed.
    fn set(&mut self, index: usize, height: f32, measured: bool) -> bool {
        self.measured[index] = measured;
        let before = self.heights[index];
        if before == height {
            return false;
        }

        self.heights[index] = height;
        self.sums.add(index, f64::from(height) - f64::from(before));

        true
    }

    /// Applies `change`, and measures how far it moved the row that covers `offset`.
    ///
    /// That row is the first visible one. In an ordinary list a row above it that grows or
    /// shrinks moves it, and a change to the row itself does not. In a compressed list the offset
    /// names a fraction of the rows and the row sits at the top of the port, so only a change of
    /// the row count moves it. A change that crosses the cap carries the row from a running total
    /// to a fraction of the rows, or back. A first visible row that the change removed has no
    /// position to keep.
    fn anchored(&mut self, offset: f64, change: impl FnOnce(&mut Self) -> bool) -> Change {
        let compressed = self.is_compressed();
        let rows = self.len();
        let anchor = self.anchor_at(offset);
        let before = self.sums.before(anchor);

        let moved = change(self);

        let shift =
            if anchor >= self.len() || (compressed && self.is_compressed() && rows == self.len()) {
                0.0
            } else if self.is_compressed() {
                self.compressed_target(anchor) - offset
            } else if compressed {
                self.sums.before(anchor) - offset
            } else {
                self.sums.before(anchor) - before
            };

        Change { moved, shift }
    }

    /// An offset at which a compressed window starts at row `index`.
    ///
    /// The middle of the span of offsets that name the row, so that rounding the offset to the
    /// precision of the container keeps the row.
    fn compressed_target(&self, index: usize) -> f64 {
        let last = self.len().saturating_sub(1);
        if last == 0 {
            return 0.0;
        }
        let extent = f64::from(MAX_EXTENT);
        ((index as f64 + 0.5) / last as f64 * extent).min(extent)
    }

    /// The row that covers `offset`: the first that ends past it, or the last row.
    fn row_at(&self, offset: f64) -> usize {
        self.sums
            .rows_within(offset)
            .min(self.len().saturating_sub(1))
    }

    /// One past the last row that starts before `bottom`, and always past `anchor`.
    fn visible_end(&self, anchor: usize, bottom: f64) -> usize {
        (self.sums.rows_before(bottom) + 1)
            .min(self.len())
            .max(anchor + 1)
    }

    /// Whether the rows are taller together than a container can be.
    fn is_compressed(&self) -> bool {
        self.sums.total() > f64::from(MAX_EXTENT)
    }
}

#[cfg(test)]
mod tests {
    use super::RowHeights;
    use crate::virtualize::MAX_EXTENT;
    use crate::virtualize::measure::MeasuredRows;

    /// Every row 20px until measured.
    fn estimate(_: usize) -> f32 {
        20.0
    }

    /// A thousand rows of the estimate, held per index.
    fn thousand() -> RowHeights {
        let mut heights = RowHeights::new(None);
        heights.rebuild(1_000, &estimate, 0.0);
        heights
    }

    #[test]
    fn measured_rows_of_mixed_heights_give_the_extent_and_the_window() {
        let mut heights = thousand();
        // Rows 0 to 9 measured: every third one is 80px tall and the rest 30px.
        for index in 0..10_usize {
            let height = if index.is_multiple_of(3) { 80.0 } else { 30.0 };
            heights.record(index, height, 0.0);
        }
        let measured = 4.0 * 80.0 + 6.0 * 30.0;

        let seen = heights.window(100.0, 0.0, 1);
        assert_eq!(seen.total(), measured + 990.0 * 20.0);
        assert_eq!(seen.first, 0);
        assert_eq!(
            seen.end(),
            3,
            "rows 0 and 1 start inside the 100px port, and one row of slack"
        );
        assert_eq!(seen.lead, 0.0);
        assert_eq!(seen.body, 80.0 + 30.0 + 30.0);

        // Scrolled into row 4, which starts at 220px: rows 4 to 6 cover 225px to 325px.
        let seen = heights.window(100.0, 225.0, 1);
        assert_eq!((seen.first, seen.end()), (3, 8));
        assert_eq!(seen.lead, 80.0 + 30.0 + 30.0);
        assert_eq!(seen.total(), measured + 990.0 * 20.0);
        assert_eq!(heights.offset_of(4), 220.0);
    }

    #[test]
    fn a_row_above_the_first_visible_one_moves_it_and_answers_by_how_much() {
        let mut heights = thousand();
        // Scrolled 10px into row 50, which starts at 1000px.
        let offset = 1_010.0;

        let grown = heights.record(48, 140.0, offset);
        assert!(grown.moved);
        assert_eq!(grown.shift, 120.0, "row 48 grew from 20px to 140px");
        assert_eq!(heights.offset_of(50), 1_120.0);

        // At the new offset the same row is first, the same 10px in.
        let seen = heights.window(400.0, (offset + grown.shift) as f32, 0);
        assert_eq!(seen.first, 50);

        let below = heights.record(60, 90.0, offset + grown.shift);
        assert_eq!(
            below.shift, 0.0,
            "a row below the first visible one moves nothing"
        );
        let itself = heights.record(50, 90.0, offset + grown.shift);
        assert_eq!(
            itself.shift, 0.0,
            "the first visible row grows down from where it is"
        );
    }

    #[test]
    fn invalidating_a_row_gives_it_its_estimate_unless_it_is_built() {
        let mut heights = thousand();
        heights.record(3, 50.0, 0.0);
        heights.record(4, 50.0, 0.0);
        heights.release(3);

        assert!(heights.invalidate(3, 20.0, 0.0).moved);
        assert_eq!(heights.offset_of(4), 80.0, "row 3 is 20px again");
        assert!(
            !heights.invalidate(4, 20.0, 0.0).moved,
            "row 4 is built, and its layout is still 50px"
        );

        heights.record(7, 70.0, 0.0);
        heights.release(7);
        heights.reset(&estimate, 0.0);
        assert_eq!(
            heights.offset_of(10),
            9.0 * 20.0 + 50.0,
            "only built row 4 stays"
        );
    }

    #[test]
    fn a_count_that_changes_keeps_the_measurements_of_rows_that_remain() {
        let mut heights = thousand();
        heights.record(2, 100.0, 0.0);
        heights.release(2);
        heights.record(900, 100.0, 0.0);
        heights.release(900);

        heights.rebuild(500, &estimate, 0.0);
        assert_eq!(heights.len(), 500);
        assert_eq!(heights.window(10.0, 0.0, 0).total(), 499.0 * 20.0 + 100.0);

        heights.rebuild(1_000, &estimate, 0.0);
        assert_eq!(
            heights.window(10.0, 0.0, 0).total(),
            999.0 * 20.0 + 100.0,
            "row 900 came back unmeasured"
        );
    }

    #[test]
    fn a_keyed_measurement_follows_its_row_to_another_index() {
        let order = std::rc::Rc::new(std::cell::RefCell::new(vec![10_u32, 11, 12, 13]));
        let read = std::rc::Rc::clone(&order);
        let MeasuredRows { keys, .. } =
            MeasuredRows::new(20.0).keyed(move |index| read.borrow()[index]);
        let mut heights = RowHeights::new(keys);
        heights.rebuild(4, &estimate, 0.0);
        heights.record(1, 60.0, 0.0);
        heights.release(1);

        // Key 11 moves from row 1 to row 3, and a new key takes row 1.
        *order.borrow_mut() = vec![10, 14, 12, 11];
        heights.rebuild(4, &estimate, 0.0);
        assert_eq!(
            heights.offset_of(2),
            40.0,
            "row 1 holds a key nobody measured"
        );
        assert_eq!(heights.offset_of(4), 3.0 * 20.0 + 60.0);

        // Key 11 leaves the list, and its measurement with it.
        *order.borrow_mut() = vec![10, 14, 12];
        heights.rebuild(3, &estimate, 0.0);
        *order.borrow_mut() = vec![10, 14, 12, 11];
        heights.rebuild(4, &estimate, 0.0);
        assert_eq!(heights.offset_of(4), 80.0);
    }

    #[test]
    fn a_built_row_keeps_its_laid_out_height_when_its_key_changes() {
        let order = std::rc::Rc::new(std::cell::RefCell::new(vec![1_u32, 2]));
        let read = std::rc::Rc::clone(&order);
        let MeasuredRows { keys, .. } =
            MeasuredRows::new(20.0).keyed(move |index| read.borrow()[index]);
        let mut heights = RowHeights::new(keys);
        heights.rebuild(2, &estimate, 0.0);
        heights.record(0, 45.0, 0.0);

        // Row 0 now shows key 3. Its layout says 45px until it reports otherwise.
        *order.borrow_mut() = vec![3, 2];
        heights.rebuild(2, &estimate, 0.0);
        assert_eq!(heights.offset_of(1), 45.0);

        // Row 0 is taken down, and key 3 moves to row 1 with the height it was laid out at.
        heights.release(0);
        *order.borrow_mut() = vec![2, 3];
        heights.rebuild(2, &estimate, 0.0);
        assert_eq!(heights.offset_of(1), 20.0, "key 2 was never measured");
        assert_eq!(heights.offset_of(2), 20.0 + 45.0);
    }

    #[test]
    fn rows_taller_together_than_the_cap_are_compressed_as_rows_of_one_height_are() {
        let mut heights = RowHeights::new(None);
        let rows = 300_000;
        heights.rebuild(rows, &estimate, 0.0);

        let top = heights.window(700.0, 0.0, 4);
        assert!(top.compressed);
        assert!((top.total() - MAX_EXTENT).abs() < 1.0);
        assert!(top.count < 100);

        let bottom = heights.window(700.0, MAX_EXTENT, 4);
        assert_eq!(bottom.end(), rows);
        assert!((bottom.total() - MAX_EXTENT).abs() < 1.0);
        assert_eq!(heights.offset_of(rows - 1), MAX_EXTENT);

        let middle = heights.window(700.0, MAX_EXTENT / 2.0, 4);
        let grown = heights.record(middle.first, 500.0, f64::from(MAX_EXTENT / 2.0));
        assert_eq!(
            grown.shift, 0.0,
            "a compressed list keeps its first row at the top"
        );
    }

    /// The first row of the window at `offset` moved by `shift`.
    fn first_after(heights: &RowHeights, offset: f64, shift: f64) -> usize {
        heights.window(700.0, (offset + shift) as f32, 0).first
    }

    #[test]
    fn an_estimate_that_crosses_the_cap_keeps_the_first_visible_row() {
        let mut heights = RowHeights::new(None);
        let rows = 100_000;
        heights.rebuild(rows, &estimate, 0.0);
        assert!(!heights.window(700.0, 0.0, 0).compressed);
        let offset = f64::from(heights.offset_of(50_000));

        let change = heights.rebuild(rows, &|_| 200.0, offset);
        assert!(heights.window(700.0, 0.0, 0).compressed);
        assert_eq!(first_after(&heights, offset, change.shift), 50_000);
    }

    #[test]
    fn an_estimate_that_falls_below_the_cap_keeps_the_first_visible_row() {
        let mut heights = RowHeights::new(None);
        let rows = 100_000;
        heights.rebuild(rows, &|_| 200.0, 0.0);
        assert!(heights.window(700.0, 0.0, 0).compressed);
        let offset = heights.compressed_target(50_000);
        assert_eq!(first_after(&heights, offset, 0.0), 50_000);

        let change = heights.rebuild(rows, &estimate, offset);
        assert!(!heights.window(700.0, 0.0, 0).compressed);
        assert_eq!(first_after(&heights, offset, change.shift), 50_000);
    }

    #[test]
    fn a_count_that_changes_in_a_compressed_list_keeps_the_first_visible_row() {
        let mut heights = RowHeights::new(None);
        heights.rebuild(300_000, &estimate, 0.0);
        let offset = heights.compressed_target(100_000);
        assert_eq!(first_after(&heights, offset, 0.0), 100_000);

        let change = heights.rebuild(600_000, &estimate, offset);
        assert!(heights.window(700.0, 0.0, 0).compressed);
        assert_ne!(
            change.shift, 0.0,
            "the row is a smaller fraction of the rows"
        );
        assert_eq!(first_after(&heights, offset, change.shift), 100_000);
    }
}
