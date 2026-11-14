//! Running totals over the heights of a list whose rows differ in height.

/// The heights of a list's rows, summed so that where a row starts and which row covers an offset
/// are each found in `O(log n)`.
///
/// A Fenwick tree. Slot `i`, counting from one, holds the sum of the `i & i.wrapping_neg()` heights
/// that end with row `i - 1`. The sums are `f64` so that a height changed many times over does not
/// move a total that a single-precision sum would round.
#[derive(Clone, Debug, Default)]
pub(crate) struct PrefixSums {
    /// The partial sums, with slot zero unused.
    tree: Vec<f64>,
}

impl PrefixSums {
    /// The sums over `heights`, built in `O(n)`.
    pub(crate) fn new(heights: &[f32]) -> Self {
        let mut tree = Vec::with_capacity(heights.len() + 1);
        tree.push(0.0);
        tree.extend(heights.iter().map(|height| f64::from(*height)));

        for slot in 1..tree.len() {
            let parent = slot + lowest_bit(slot);
            if parent < tree.len() {
                tree[parent] += tree[slot];
            }
        }

        Self { tree }
    }

    /// How many rows are summed.
    pub(crate) fn len(&self) -> usize {
        self.tree.len().saturating_sub(1)
    }

    /// Adds `delta` to the height of row `index`.
    pub(crate) fn add(&mut self, index: usize, delta: f64) {
        let mut slot = index + 1;
        while slot < self.tree.len() {
            self.tree[slot] += delta;
            slot += lowest_bit(slot);
        }
    }

    /// The sum of the first `count` heights, which is where row `count` starts.
    pub(crate) fn before(&self, count: usize) -> f64 {
        let mut slot = count.min(self.len());
        let mut sum = 0.0;
        while slot > 0 {
            sum += self.tree[slot];
            slot -= lowest_bit(slot);
        }

        sum
    }

    /// The sum of every height.
    pub(crate) fn total(&self) -> f64 {
        self.before(self.len())
    }

    /// The most rows from the start whose heights together are at most `extent`.
    ///
    /// The row with this index is the one that covers `extent`. A row of no height at `extent`
    /// is counted, so the answer is the first row that ends past it.
    pub(crate) fn rows_within(&self, extent: f64) -> usize {
        self.descend(extent, |sum, rest| sum <= rest)
    }

    /// The most rows from the start whose heights together are less than `extent`.
    ///
    /// For a positive `extent`, the row with this index is the last one that starts before it.
    pub(crate) fn rows_before(&self, extent: f64) -> usize {
        self.descend(extent, |sum, rest| sum < rest)
    }

    /// The largest count of rows whose sum `fits` within `extent`, found by one walk down the tree.
    ///
    /// The heights are never negative, so the sums grow with the count and one walk finds it.
    fn descend(&self, extent: f64, fits: impl Fn(f64, f64) -> bool) -> usize {
        let len = self.len();
        if len == 0 {
            return 0;
        }

        let mut position = 0;
        let mut rest = extent;
        let mut step = 1 << len.ilog2();
        while step > 0 {
            let next = position + step;
            if next <= len && fits(self.tree[next], rest) {
                position = next;
                rest -= self.tree[next];
            }
            step >>= 1;
        }

        position
    }
}

/// The lowest set bit of `slot`.
const fn lowest_bit(slot: usize) -> usize {
    slot & slot.wrapping_neg()
}

#[cfg(test)]
mod tests {
    use super::PrefixSums;

    /// Heights with runs of rows of no height in them, which the searches must step over.
    const HEIGHTS: [f32; 12] = [
        20.0, 0.0, 0.0, 35.5, 20.0, 120.0, 0.0, 20.0, 64.0, 20.0, 0.0, 41.0,
    ];

    /// The sum of the first `count` of `heights`, added one at a time.
    fn naive(heights: &[f32], count: usize) -> f64 {
        heights[..count]
            .iter()
            .map(|height| f64::from(*height))
            .sum()
    }

    #[test]
    fn every_prefix_is_the_sum_of_the_heights_before_it() {
        let sums = PrefixSums::new(&HEIGHTS);
        assert_eq!(sums.len(), HEIGHTS.len());
        for count in 0..=HEIGHTS.len() {
            assert_eq!(sums.before(count), naive(&HEIGHTS, count), "prefix {count}");
        }
        assert_eq!(sums.total(), naive(&HEIGHTS, HEIGHTS.len()));
    }

    #[test]
    fn a_changed_height_moves_every_prefix_after_it_and_none_before() {
        let mut heights = HEIGHTS;
        let mut sums = PrefixSums::new(&heights);

        sums.add(5, -100.0);
        heights[5] -= 100.0;
        sums.add(0, 4.0);
        heights[0] += 4.0;

        for count in 0..=heights.len() {
            assert_eq!(sums.before(count), naive(&heights, count), "prefix {count}");
        }
    }

    #[test]
    fn the_searches_agree_with_a_walk_from_the_start() {
        let sums = PrefixSums::new(&HEIGHTS);
        let total = naive(&HEIGHTS, HEIGHTS.len());
        let mut extent = -1.0;
        while extent <= total + 2.0 {
            let within = (0..=HEIGHTS.len())
                .rev()
                .find(|count| naive(&HEIGHTS, *count) <= extent)
                .unwrap_or(0);
            let before = (0..=HEIGHTS.len())
                .rev()
                .find(|count| naive(&HEIGHTS, *count) < extent)
                .unwrap_or(0);
            assert_eq!(sums.rows_within(extent), within, "within {extent}");
            assert_eq!(sums.rows_before(extent), before, "before {extent}");
            extent += 0.5;
        }
    }

    #[test]
    fn rows_of_no_height_are_stepped_over_to_the_row_that_covers_an_offset() {
        let sums = PrefixSums::new(&HEIGHTS);
        assert_eq!(sums.rows_within(20.0), 3, "rows 1 and 2 have no height");
        assert_eq!(
            sums.rows_before(20.0),
            0,
            "and row 0 is the last to start before 20px"
        );
    }

    #[test]
    fn an_empty_list_has_no_rows_to_find() {
        let sums = PrefixSums::new(&[]);
        assert_eq!(sums.total(), 0.0);
        assert_eq!(sums.rows_within(100.0), 0);
        assert_eq!(sums.rows_before(100.0), 0);
    }
}
