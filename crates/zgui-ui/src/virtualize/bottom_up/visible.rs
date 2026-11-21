//! Which rows of a bottom-up list are on screen.

use core::ops::Range;

/// The rows on screen in a port `height` CSS pixels tall whose top is at `top`.
///
/// `rows` gives each row as its index, its top and its bottom, in CSS pixels from the top of the
/// container, in order. A row is on screen when half of it or more is in the port, or when it
/// fills half of the port or more.
pub(crate) fn on_screen(
    rows: impl IntoIterator<Item = (usize, f64, f64)>,
    top: f64,
    height: f64,
) -> Range<usize> {
    let bottom = top + height;
    let mut seen: Option<Range<usize>> = None;
    for (index, start, end) in rows {
        let shown = end.min(bottom) - start.max(top);
        if shown <= 0.0 || shown < (end - start).min(height) / 2.0 {
            continue;
        }

        seen = Some(seen.map_or(index..index + 1, |rows| rows.start..index + 1));
    }

    seen.unwrap_or(0..0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows `heights` tall from `first` on, stacked from `top`.
    fn stacked(first: usize, top: f64, heights: &[f64]) -> Vec<(usize, f64, f64)> {
        let mut start = top;
        heights
            .iter()
            .enumerate()
            .map(|(offset, height)| {
                let row = (first + offset, start, start + height);
                start += height;
                row
            })
            .collect()
    }

    #[test]
    fn a_row_is_on_screen_when_half_of_it_is() {
        // Rows 10 to 15 are 100px tall from 1 000px. The port shows 1 050px to 1 450px.
        let rows = stacked(10, 1_000.0, &[100.0; 6]);
        assert_eq!(on_screen(rows.clone(), 1_050.0, 400.0), 10..15);
        assert_eq!(on_screen(rows, 1_051.0, 400.0), 11..15, "row 10 shows 49px");
    }

    #[test]
    fn a_row_taller_than_the_port_is_on_screen_while_it_fills_half_of_it() {
        let rows = stacked(0, 0.0, &[100.0, 1_000.0, 100.0]);
        assert_eq!(on_screen(rows.clone(), 300.0, 400.0), 1..2);
        assert_eq!(on_screen(rows, 0.0, 400.0), 0..2);
    }

    #[test]
    fn a_port_that_shows_no_row_shows_none() {
        let rows = stacked(0, 400.0, &[50.0, 50.0]);
        assert_eq!(on_screen(rows, 0.0, 400.0), 0..0);
    }
}
