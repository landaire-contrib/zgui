//! The breadth of a scroll container's scrollbars, which this build cannot read from CSS.
//!
//! A tab strip that scrolls sideways has to scroll by wheel, by touch and to keep the selected tab
//! in view, and it must not show a bar under the tabs. CSS says that with `scrollbar-width: none`.
//! The engine's sources define that longhand for another target only, so the parser does not know
//! the name and drops a declaration of it.
//!
//! The custom property
//! [`SCROLLBAR_WIDTH`](crate::values::scrollbar::SCROLLBAR_WIDTH) takes the same keywords.

use crate::parity::support::{AbsentReason, Support};

crate::register_properties! {
    scrollbar_width => Support::Absent(AbsentReason::GeckoOnly),
}

#[cfg(test)]
mod tests {
    use super::REGISTERED;

    #[test]
    fn the_row_is_the_one_property_that_sets_the_breadth_of_a_bar() {
        let names: Vec<String> = REGISTERED.iter().map(|row| row.css_name()).collect();
        assert_eq!(names, ["scrollbar-width"]);
    }
}
