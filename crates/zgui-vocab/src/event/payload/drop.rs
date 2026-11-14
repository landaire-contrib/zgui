//! Content dragged onto the window from outside it.

use std::path::PathBuf;

use zgui_geom::{Css, CssPx, Point};

use crate::text::SharedString;

/// What a drag from outside the window carries, in each form the platform could read.
///
/// A drag source offers its content in several forms at once: a file manager offers files, a
/// browser offers a link as an address and as text, and an editor offers a selection as text and
/// as markup. A target takes the form it can use. A form is empty when the source did not offer
/// it or the backend cannot read it, so a target that only takes files sees no files in a drag of
/// text and declines it.
///
/// ```
/// use std::path::PathBuf;
/// use zgui_vocab::DragData;
///
/// let files = DragData::from_paths(vec![PathBuf::from("/tmp/a.png")]);
/// assert_eq!(files.paths.len(), 1);
/// assert!(files.text.is_none());
///
/// let link = DragData::default()
///     .with_uris(vec!["https://example.com/".into()])
///     .with_text("https://example.com/");
/// assert!(link.paths.is_empty());
/// assert!(!link.is_empty());
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct DragData {
    /// Local files, in the order the source listed them.
    pub paths: Vec<PathBuf>,
    /// Addresses that name no local file, such as a link dragged from a browser, in the order the
    /// source listed them.
    pub uris: Vec<SharedString>,
    /// The content as plain text.
    pub text: Option<SharedString>,
    /// The content as markup.
    pub html: Option<SharedString>,
}

impl DragData {
    /// A drag of `paths` and nothing else.
    pub fn from_paths(paths: Vec<PathBuf>) -> Self {
        Self {
            paths,
            ..Self::default()
        }
    }

    /// The same, carrying `uris` as well.
    #[must_use]
    pub fn with_uris(mut self, uris: Vec<SharedString>) -> Self {
        self.uris = uris;
        self
    }

    /// The same, carrying `text` as well.
    #[must_use]
    pub fn with_text(mut self, text: impl Into<SharedString>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// The same, carrying `html` as well.
    #[must_use]
    pub fn with_html(mut self, html: impl Into<SharedString>) -> Self {
        self.html = Some(html.into());
        self
    }

    /// Whether the drag carries nothing in any form.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && self.uris.is_empty() && self.text.is_none() && self.html.is_none()
    }
}

/// What a drop, drag enter or drag over event carries: everything dragged at once, and where.
///
/// A drop is one event carrying the whole drag rather than one event per file. That is not a
/// convenience: a handler told about three files one at a time cannot tell whether a fourth is
/// coming, so it either acts three times or waits for a signal that never arrives.
///
/// ```
/// use std::path::PathBuf;
/// use zgui_geom::{CssPx, Point};
/// use zgui_vocab::{DragData, DropEvent};
///
/// let event = DropEvent {
///     data: DragData::from_paths(vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")]),
///     position: Point::new(CssPx(10.0), CssPx(20.0)),
/// };
/// assert_eq!(event.data.paths.len(), 2);
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct DropEvent {
    /// Everything dragged.
    ///
    /// A drag enter or drag over event has what the platform gave when the drag entered the
    /// window.
    pub data: DragData,
    /// Where the drag is, in CSS pixels from the window's top-left corner.
    pub position: Point<CssPx, Css>,
}

impl DropEvent {
    /// Whether anything was actually dragged.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::{DragData, DropEvent};
    use std::path::PathBuf;
    use zgui_geom::{CssPx, Point};

    #[test]
    fn a_drop_carries_every_path_at_once() {
        let event = DropEvent {
            data: DragData::from_paths(vec![PathBuf::from("/a"), PathBuf::from("/b")]),
            position: Point::new(CssPx(0.0), CssPx(0.0)),
        };
        assert!(!event.is_empty());
        assert_eq!(event.data.paths.len(), 2);
    }

    #[test]
    fn a_drag_of_any_one_form_is_not_empty() {
        assert!(DragData::default().is_empty());
        assert!(
            !DragData::default()
                .with_uris(vec!["https://a/".into()])
                .is_empty()
        );
        assert!(!DragData::default().with_text("a").is_empty());
        assert!(!DragData::default().with_html("<b>a</b>").is_empty());
    }
}
