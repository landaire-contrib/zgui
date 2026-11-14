//! Content dragged over a window from outside the application.

use std::path::PathBuf;

use zgui_geom::{Css, CssPx, Device, DevicePx, Point, Size};
use zgui_platform::DragEvent;
use zgui_vocab::DragData;

/// A drag in flight over one window.
///
/// The platform reports dragged files **one at a time**, with no event that says the set is
/// complete. The contract carries the whole set at every stage, and that difference is not
/// cosmetic: a drop target told about one file at a time cannot know whether more are coming, so it
/// either acts once per file or waits for an end that never arrives.
///
/// So the paths are gathered as they arrive and handed over as one event when the turn of the loop
/// that delivered them is finished. That is the earliest moment at which the set is known to be
/// complete, because the platform delivers a drag's files within a single turn.
///
/// # What a drop target gets
///
/// * [`DragEvent::Entered`] with every path, at the end of the turn that reported them.
/// * No [`DragEvent::Moved`]. winit reports no pointer motion while an external drag is over the
///   window, so `Entered` and `Dropped` have the last pointer position inside the window from
///   before the drag, or the window's centre if the pointer was never inside (see
///   [`Drag::position`]).
/// * [`DragEvent::Left`] when the platform cancels the hover.
/// * [`DragEvent::Dropped`] with every path.
///
/// Only files arrive: winit reports no other form of dragged content, so [`DragData::paths`] is
/// the only field it fills. winit reports no external drags on Wayland. `zgui-platform-wayland`
/// reports them, in every form it reads.
#[derive(Debug, Default)]
pub struct Drag {
    /// Paths reported as hovering and not yet announced.
    hovering: Vec<PathBuf>,
    /// Paths reported as dropped and not yet announced.
    dropped: Vec<PathBuf>,
    /// Whether the drag left without being dropped.
    left: bool,
    /// The window's extent in CSS pixels, when known.
    extent: Option<Size<CssPx, Css>>,
    /// The last pointer position inside the window.
    ///
    /// A position outside is not kept. X11 reports one as the pointer leaves for the drag source,
    /// and a drop routed to it reaches no element.
    inside: Option<Point<CssPx, Css>>,
}

impl Drag {
    /// Records a path being dragged over the window.
    pub fn hovering(&mut self, path: PathBuf) {
        self.hovering.push(path);
    }

    /// Records a path being dropped on the window.
    pub fn dropped(&mut self, path: PathBuf) {
        self.dropped.push(path);
    }

    /// Records the drag leaving without a drop.
    pub const fn left(&mut self) {
        self.left = true;
    }

    /// Whether anything is waiting to be announced.
    pub const fn is_pending(&self) -> bool {
        self.left || !self.hovering.is_empty() || !self.dropped.is_empty()
    }

    /// Records the window's extent of `size` device pixels at `scale_factor`.
    pub fn resized(&mut self, size: Size<DevicePx, Device>, scale_factor: f64) {
        self.extent = Some(css_extent(size, scale_factor));
    }

    /// Records a pointer position, and keeps it for a drag if it is inside the window.
    pub fn pointer_moved(&mut self, position: Point<CssPx, Css>) {
        if contains(self.extent, position) {
            self.inside = Some(position);
        }
    }

    /// The position for a drag over a window of `size` device pixels at `scale_factor`.
    ///
    /// This is the last pointer position inside the window. If there is no such position, or the
    /// window became smaller than it, this is the window's centre, so a target that covers the
    /// whole window still gets the drop.
    pub fn position(&self, size: Size<DevicePx, Device>, scale_factor: f64) -> Point<CssPx, Css> {
        let extent = css_extent(size, scale_factor);
        self.inside
            .filter(|inside| contains(Some(extent), *inside))
            .unwrap_or_else(|| {
                Point::new(CssPx(extent.width.0 / 2.0), CssPx(extent.height.0 / 2.0))
            })
    }

    /// Takes everything gathered, as the events a drop target is written against.
    ///
    /// A drop is announced after an entry, in that order, because a target that is told about a
    /// drop it was never told to expect has had no chance to say whether it would accept one.
    pub fn take(&mut self, position: Point<CssPx, Css>) -> Vec<DragEvent> {
        let mut events = Vec::new();
        if !self.hovering.is_empty() {
            events.push(DragEvent::Entered {
                data: DragData::from_paths(core::mem::take(&mut self.hovering)),
                position,
            });
        }
        if !self.dropped.is_empty() {
            events.push(DragEvent::Dropped {
                data: DragData::from_paths(core::mem::take(&mut self.dropped)),
                position,
            });
        }
        if core::mem::take(&mut self.left) {
            events.push(DragEvent::Left);
        }
        events
    }
}

/// The extent of `size` device pixels in CSS pixels at `scale_factor`.
fn css_extent(size: Size<DevicePx, Device>, scale_factor: f64) -> Size<CssPx, Css> {
    let scale = if scale_factor > 0.0 {
        scale_factor
    } else {
        1.0
    };
    Size::new(
        CssPx((f64::from(size.width.0) / scale) as f32),
        CssPx((f64::from(size.height.0) / scale) as f32),
    )
}

/// Whether `position` is inside `extent`. With no known extent, only the far edges are unchecked.
fn contains(extent: Option<Size<CssPx, Css>>, position: Point<CssPx, Css>) -> bool {
    position.x.0 >= 0.0
        && position.y.0 >= 0.0
        && extent
            .is_none_or(|extent| position.x.0 < extent.width.0 && position.y.0 < extent.height.0)
}

#[cfg(test)]
mod tests {
    use super::Drag;
    use std::path::PathBuf;
    use zgui_geom::{Css, CssPx, DevicePx, Point, Size};
    use zgui_platform::DragEvent;

    fn at(x: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(0.0))
    }

    fn point(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn a_drag_goes_to_the_last_position_inside_the_window() {
        let size = Size::new(DevicePx(800.0), DevicePx(600.0));
        let mut drag = Drag::default();
        drag.resized(size, 2.0);
        drag.pointer_moved(point(50.0, 30.0));
        // X11 reports (-1, -1) as the pointer leaves for the drag source.
        drag.pointer_moved(point(-1.0, -1.0));
        drag.pointer_moved(point(450.0, 30.0));
        assert_eq!(drag.position(size, 2.0), point(50.0, 30.0));
    }

    #[test]
    fn a_drag_with_no_position_inside_the_window_goes_to_the_centre() {
        let size = Size::new(DevicePx(800.0), DevicePx(600.0));
        let mut drag = Drag::default();
        drag.pointer_moved(point(-1.0, -1.0));
        assert_eq!(drag.position(size, 2.0), point(200.0, 150.0));

        // With no extent known, a position is kept until a drag finds it outside the window.
        drag.pointer_moved(point(500.0, 200.0));
        assert_eq!(drag.position(size, 2.0), point(200.0, 150.0));
    }

    #[test]
    fn three_files_dropped_at_once_arrive_as_one_event() {
        // Told about them one at a time, a target cannot know when the set is complete, so it
        // either acts three times or waits for an end that never comes.
        let mut drag = Drag::default();
        for name in ["/a.png", "/b.png", "/c.png"] {
            drag.dropped(PathBuf::from(name));
        }
        let events = drag.take(at(4.0));
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data().map(|data| data.paths.len()), Some(3));
        assert!(events[0].is_drop());
    }

    #[test]
    fn a_target_is_told_what_is_coming_before_it_is_told_it_arrived() {
        let mut drag = Drag::default();
        drag.hovering(PathBuf::from("/a.png"));
        drag.dropped(PathBuf::from("/a.png"));
        let events = drag.take(at(1.0));
        assert!(matches!(events[0], DragEvent::Entered { .. }));
        assert!(events[1].is_drop());
    }

    #[test]
    fn nothing_gathered_announces_nothing() {
        let mut drag = Drag::default();
        assert!(!drag.is_pending());
        assert!(drag.take(at(0.0)).is_empty());
    }

    #[test]
    fn a_drag_that_left_without_dropping_says_so_and_carries_no_paths() {
        let mut drag = Drag::default();
        drag.hovering(PathBuf::from("/a.png"));
        let _ = drag.take(at(0.0));
        drag.left();
        assert!(drag.is_pending());
        let events = drag.take(at(0.0));
        assert_eq!(events, vec![DragEvent::Left]);
        assert!(!drag.is_pending(), "the departure was announced twice");
    }
}
