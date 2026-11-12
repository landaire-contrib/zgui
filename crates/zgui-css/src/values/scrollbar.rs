//! How broad a scroll container's scrollbars are.
//!
//! CSS sets this with `scrollbar-width`. The engine generates that longhand for another target
//! only, so the parser drops a declaration of it. The custom property [`SCROLLBAR_WIDTH`] takes the
//! same three keywords:
//!
//! ```css
//! .tabs { overflow-x: auto; --zgui-scrollbar-width: none }
//! ```
//!
//! A custom property inherits, and `scrollbar-width` does not. A scroll container inside one that
//! sets this property therefore takes the same value. Write `auto` on the inner container to get
//! the default bar back.

use crate::computed::style::ComputedStyle;
use crate::values::custom::Property;
use crate::values::size::OverflowValue;

/// `--zgui-scrollbar-width`, which takes `auto`, `thin` or `none`.
pub static SCROLLBAR_WIDTH: Property = Property::new("zgui-scrollbar-width");

/// The breadth of the scrollbars of one scroll container.
///
/// ```
/// use zgui_css::StyleDraft;
/// use zgui_css::values::size::OverflowValue;
/// use zgui_css::values::scrollbar::ScrollbarWidth;
///
/// let mut draft = StyleDraft::initial().with_custom_property("zgui-scrollbar-width", "thin");
/// draft.box_group().overflow_y = OverflowValue::Scroll;
/// assert_eq!(ScrollbarWidth::of(&draft.build()), ScrollbarWidth::Thin);
/// assert_eq!(ScrollbarWidth::Thin.breadth(15.0), 8.0);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ScrollbarWidth {
    /// The breadth the theme gives a scrollbar.
    #[default]
    Auto,
    /// Half the breadth the theme gives a scrollbar.
    Thin,
    /// No scrollbar and no gutter. The content still scrolls by wheel, touch, keyboard and
    /// program.
    None,
}

impl ScrollbarWidth {
    /// The keyword `text` names, or `None` when it names none of the three.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        [
            ("auto", Self::Auto),
            ("thin", Self::Thin),
            ("none", Self::None),
        ]
        .into_iter()
        .find_map(|(keyword, value)| text.eq_ignore_ascii_case(keyword).then_some(value))
    }

    /// The value the box with `style` uses.
    ///
    /// Only a box whose overflow is `auto` or `scroll` on one axis or both has scrollbars, so every
    /// other box uses [`Auto`](Self::Auto) and its custom properties are not read. A value that
    /// names none of the three keywords is invalid at computed-value time, and CSS then uses the
    /// initial value, which is `auto`.
    pub fn of(style: &ComputedStyle) -> Self {
        let box_ = style.get_box();
        let scrolls = |value| matches!(value, OverflowValue::Auto | OverflowValue::Scroll);
        if !scrolls(box_.overflow_x) && !scrolls(box_.overflow_y) {
            return Self::Auto;
        }

        SCROLLBAR_WIDTH
            .text(style)
            .and_then(Self::parse)
            .unwrap_or_default()
    }

    /// The breadth of a bar of this width, in the unit of `auto_breadth`.
    ///
    /// `auto_breadth` is the breadth of an `auto` bar. A thin bar is half of it, rounded to the
    /// nearest whole unit so that its edges stay on whole device pixels.
    pub fn breadth(self, auto_breadth: f32) -> f32 {
        match self {
            Self::Auto => auto_breadth,
            Self::Thin => (auto_breadth * 0.5).round(),
            Self::None => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::StyleDraft;
    use crate::values::size::OverflowValue;

    use super::ScrollbarWidth;

    fn scroller(value: Option<&str>, overflow: OverflowValue) -> ScrollbarWidth {
        let draft = StyleDraft::initial();
        let mut draft = match value {
            Some(value) => draft.with_custom_property("zgui-scrollbar-width", value),
            None => draft,
        };
        draft.box_group().overflow_x = overflow;
        ScrollbarWidth::of(&draft.build())
    }

    #[test]
    fn each_keyword_parses_and_anything_else_does_not() {
        assert_eq!(ScrollbarWidth::parse(" auto "), Some(ScrollbarWidth::Auto));
        assert_eq!(ScrollbarWidth::parse("THIN"), Some(ScrollbarWidth::Thin));
        assert_eq!(ScrollbarWidth::parse("none"), Some(ScrollbarWidth::None));
        assert_eq!(ScrollbarWidth::parse("8px"), None);
        assert_eq!(ScrollbarWidth::parse(""), None);
    }

    #[test]
    fn a_scroll_container_uses_the_keyword_it_was_given() {
        assert_eq!(
            scroller(Some("none"), OverflowValue::Auto),
            ScrollbarWidth::None
        );
        assert_eq!(
            scroller(Some("thin"), OverflowValue::Scroll),
            ScrollbarWidth::Thin
        );
        assert_eq!(scroller(None, OverflowValue::Scroll), ScrollbarWidth::Auto);
        assert_eq!(
            scroller(Some("wide"), OverflowValue::Scroll),
            ScrollbarWidth::Auto,
            "an invalid value computes to the initial value"
        );
    }

    #[test]
    fn a_box_that_does_not_scroll_uses_auto() {
        for overflow in [
            OverflowValue::Visible,
            OverflowValue::Hidden,
            OverflowValue::Clip,
        ] {
            assert_eq!(
                scroller(Some("none"), overflow),
                ScrollbarWidth::Auto,
                "{overflow:?}"
            );
        }
    }

    #[test]
    fn a_thin_bar_is_half_as_broad_on_whole_pixels_and_no_bar_has_no_breadth() {
        assert_eq!(ScrollbarWidth::Auto.breadth(15.0), 15.0);
        assert_eq!(ScrollbarWidth::Thin.breadth(15.0), 8.0);
        assert_eq!(ScrollbarWidth::Thin.breadth(6.0), 3.0);
        assert_eq!(ScrollbarWidth::None.breadth(15.0), 0.0);
    }
}
