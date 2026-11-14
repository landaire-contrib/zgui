//! Content being dragged over a surface from outside it.

use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::DragData;

/// A stage of a drag that started outside the application.
///
/// Everything dragged arrives at once, with a position, at every stage that carries it. That
/// shape is not a convenience: told about one file at a time, a drop target cannot know whether
/// more are coming, so it either acts once per file or waits for an end that never comes. It also
/// lets a target decide whether it *would* accept the drop while the pointer is still moving, which
/// is what makes a highlight appear before the user lets go rather than after.
///
/// A drag over a surface ends with [`DragEvent::Left`] or [`DragEvent::Dropped`]. A backend sends
/// [`DragEvent::Entered`] when it knows what is dragged. This can be some time after the pointer
/// arrived, and a drop before that has no `Entered`. A backend that gets no pointer motion during
/// a drag sends no [`DragEvent::Moved`]. The backend documentation tells what each backend sends,
/// and which forms of [`DragData`] it can read.
///
/// ```
/// use std::path::PathBuf;
/// use zgui_geom::{CssPx, Point};
/// use zgui_platform::DragEvent;
/// use zgui_vocab::DragData;
///
/// let entered = DragEvent::Entered {
///     data: DragData::from_paths(vec![PathBuf::from("/tmp/a.png")]),
///     position: Point::new(CssPx(4.0), CssPx(4.0)),
/// };
/// assert_eq!(entered.data().map(|data| data.paths.len()), Some(1));
/// assert!(!entered.is_drop());
/// ```
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DragEvent {
    /// A drag came over the surface, carrying `data`.
    Entered {
        /// Everything being dragged.
        data: DragData,
        /// Where the pointer is, in CSS pixels from the surface's top-left corner.
        position: Point<CssPx, Css>,
    },
    /// The drag moved while over the surface.
    Moved {
        /// Where the pointer is now.
        position: Point<CssPx, Css>,
    },
    /// The drag was released over the surface.
    Dropped {
        /// Everything dropped.
        data: DragData,
        /// Where it was dropped.
        position: Point<CssPx, Css>,
    },
    /// The drag left the surface without being dropped.
    Left,
}

impl DragEvent {
    /// Everything being dragged, on the stages that carry it.
    pub const fn data(&self) -> Option<&DragData> {
        match self {
            Self::Entered { data, .. } | Self::Dropped { data, .. } => Some(data),
            Self::Moved { .. } | Self::Left => None,
        }
    }

    /// Where the pointer is, when this stage says.
    pub fn position(&self) -> Option<Point<CssPx, Css>> {
        match self {
            Self::Entered { position, .. }
            | Self::Moved { position }
            | Self::Dropped { position, .. } => Some(*position),
            Self::Left => None,
        }
    }

    /// Whether this stage is the release.
    pub const fn is_drop(&self) -> bool {
        matches!(self, Self::Dropped { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::DragEvent;
    use std::path::PathBuf;
    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::DragData;

    fn at(x: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(0.0))
    }

    #[test]
    fn only_the_carrying_stages_report_what_is_dragged() {
        let data = DragData::from_paths(vec![PathBuf::from("/a")]);
        let entered = DragEvent::Entered {
            data: data.clone(),
            position: at(1.0),
        };
        assert_eq!(entered.data(), Some(&data));
        assert_eq!(DragEvent::Moved { position: at(2.0) }.data(), None);
        assert_eq!(DragEvent::Left.data(), None);
    }

    #[test]
    fn leaving_reports_no_position_and_every_other_stage_does() {
        assert_eq!(DragEvent::Left.position(), None);
        assert_eq!(
            DragEvent::Moved { position: at(3.0) }.position(),
            Some(at(3.0))
        );
    }

    #[test]
    fn exactly_one_stage_is_the_release() {
        let dropped = DragEvent::Dropped {
            data: DragData::default(),
            position: at(0.0),
        };
        assert!(dropped.is_drop());
        assert!(!DragEvent::Left.is_drop());
    }
}
