//! What a list whose rows differ in height assumes about a row before the row is measured.

use core::hash::Hash;
use std::collections::HashMap;

use zgui::prelude::*;
use zgui::reactive::LocalStorage;

/// How a virtualised list learns the height of rows that differ in height.
///
/// A row that has not been built is given an estimate. A row that is built is measured from its
/// layout, and the measurement replaces the estimate for as long as the list holds it. The window,
/// the two spacers and the scroll extent are all decided from these heights.
///
/// Measurements are held per row index. A list whose rows can move to another index, by a sort, a
/// filter or an insertion, names a key for each row with [`MeasuredRows::keyed`], and a measurement
/// then follows its row.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::RwSignal;
/// use zgui::{component, view};
/// use zgui_ui::prelude::*;
///
/// /// Notes of any length, each wrapped to as many lines as it needs.
/// #[component]
/// fn Notes() -> impl IntoView {
///     let notes = RwSignal::new_local(vec![(7_u64, "short".to_owned())]);
///     view! {
///         VirtualList(
///             count = Signal::derive_local(move || notes.with(Vec::len)),
///             measured = MeasuredRows::new(24.0).keyed(move |index| notes.with(|notes| notes[index].0)),
///             label = "Notes",
///             row = move |index: usize| view! {
///                 text {{move || notes.with(|notes| notes[index].1.clone())}}
///             }
///         )
///     }
/// }
/// ```
///
/// # Cost
///
/// The list keeps a height and a running total for each row: about thirteen bytes a row, and one
/// key a row with [`MeasuredRows::keyed`]. Finding the window and the place of a row takes `O(log n)`
/// time, and so does a new measurement. A change to the row count, to the estimate or to a key
/// estimates the list again in `O(n)` time.
pub struct MeasuredRows {
    /// What a row is given before it is measured.
    pub(crate) estimate: Estimate,
    /// Where measurements are held per key, when they are.
    pub(crate) keys: Option<Box<dyn RowKeys>>,
}

impl MeasuredRows {
    /// Rows `estimate` CSS pixels tall until they are measured.
    ///
    /// A signal here estimates every unmeasured row again when it changes.
    #[must_use]
    pub fn new(estimate: impl Into<Signal<f32, LocalStorage>>) -> Self {
        Self {
            estimate: Estimate(estimate.into()),
            keys: None,
        }
    }

    /// Holds each measurement under `key(index)`, so that it follows its row to another index.
    ///
    /// `key` is read reactively for every row. When a signal it reads changes, every row takes the
    /// measurement held under its key, or the estimate when there is none. A measurement whose
    /// key no row has any more is dropped.
    #[must_use]
    pub fn keyed<K, F>(self, key: F) -> Self
    where
        K: Hash + Eq + Clone + 'static,
        F: Fn(usize) -> K + 'static,
    {
        Self {
            keys: Some(Box::new(Keyed {
                key,
                rows: Vec::new(),
                heights: HashMap::new(),
            })),
            ..self
        }
    }
}

/// The height every row is given before it is measured.
#[derive(Clone)]
pub(crate) struct Estimate(Signal<f32, LocalStorage>);

impl Estimate {
    /// The height in CSS pixels, or `None` once its signal is gone.
    ///
    /// `tracked` decides whether reading it subscribes the caller. An estimate below one pixel, or
    /// one that is not a number, is taken as one pixel. With unmeasured rows of no height, one port
    /// would show every unmeasured row and the list would build all of them.
    pub(crate) fn read(&self, tracked: bool) -> Option<f32> {
        let height = if tracked {
            self.0.try_get()
        } else {
            self.0.try_get_untracked()
        }?;

        Some(if height.is_finite() {
            height.max(1.0)
        } else {
            1.0
        })
    }
}

/// Measurements held per row key, with the key type hidden.
pub(crate) trait RowKeys {
    /// Gives rows `0..rows` their keys, and drops every measurement whose key no row has.
    ///
    /// The key function is read here and nowhere else, so that a row asked about later has the
    /// key it had when the list was last counted.
    fn bind(&mut self, rows: usize);

    /// The measurement held under the key of row `index`.
    fn height(&self, index: usize) -> Option<f32>;

    /// Holds `height` under the key of row `index`.
    fn record(&mut self, index: usize, height: f32);

    /// Drops the measurement held under the key of row `index`.
    fn forget(&mut self, index: usize);

    /// Drops every measurement.
    fn clear(&mut self);
}

/// Measurements held under the key a function gives each row.
struct Keyed<K, F> {
    /// What names each row.
    key: F,
    /// The key of each row, as of the last count.
    rows: Vec<K>,
    /// The measurement held under each key.
    heights: HashMap<K, f32>,
}

impl<K, F> RowKeys for Keyed<K, F>
where
    K: Hash + Eq + Clone,
    F: Fn(usize) -> K,
{
    fn bind(&mut self, rows: usize) {
        self.rows = (0..rows).map(&self.key).collect();

        let mut kept = HashMap::with_capacity(self.heights.len().min(rows));
        for key in &self.rows {
            if let Some((key, height)) = self.heights.remove_entry(key) {
                kept.insert(key, height);
            }
        }

        self.heights = kept;
    }

    fn height(&self, index: usize) -> Option<f32> {
        let key = self.rows.get(index)?;
        self.heights.get(key).copied()
    }

    fn record(&mut self, index: usize, height: f32) {
        let Some(key) = self.rows.get(index) else {
            return;
        };
        match self.heights.get_mut(key) {
            Some(held) => *held = height,
            None => {
                self.heights.insert(key.clone(), height);
            }
        }
    }

    fn forget(&mut self, index: usize) {
        if let Some(key) = self.rows.get(index) {
            self.heights.remove(key);
        }
    }

    fn clear(&mut self) {
        self.heights.clear();
    }
}
