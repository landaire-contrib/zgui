//! Scripted drags from outside the application.
//!
//! Each method delivers one [`DragEvent`] stage to the first surface, as a backend does when the
//! user drags content over a window. A drag starts with [`Harness::drag_enter`], which carries
//! what is dragged, and ends with [`Harness::drag_leave`] or [`Harness::drag_drop`].

use zgui_geom::{Css, CssPx, Point};
use zgui_platform::{AppHandler, DragEvent, SurfaceEvent};
use zgui_vocab::DragData;

use crate::harness::Harness;

impl<A: AppHandler> Harness<A> {
    /// Moves a drag of `data` onto the first surface at `position`.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, as [`Harness::deliver_to_first`] does.
    pub fn drag_enter(&mut self, data: DragData, position: Point<CssPx, Css>) {
        self.deliver_to_first(SurfaceEvent::Drag(DragEvent::Entered { data, position }));
    }

    /// Moves the drag over the first surface to `position`.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, as [`Harness::deliver_to_first`] does.
    pub fn drag_move(&mut self, position: Point<CssPx, Css>) {
        self.deliver_to_first(SurfaceEvent::Drag(DragEvent::Moved { position }));
    }

    /// Moves the drag off the first surface without a drop.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, as [`Harness::deliver_to_first`] does.
    pub fn drag_leave(&mut self) {
        self.deliver_to_first(SurfaceEvent::Drag(DragEvent::Left));
    }

    /// Drops `data` on the first surface at `position`.
    ///
    /// # Panics
    ///
    /// Panics when no surface exists, as [`Harness::deliver_to_first`] does.
    pub fn drag_drop(&mut self, data: DragData, position: Point<CssPx, Css>) {
        self.deliver_to_first(SurfaceEvent::Drag(DragEvent::Dropped { data, position }));
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use zgui_geom::{CssPx, Point};
    use zgui_platform::{
        AppHandler, DragEvent, PlatformCx, SurfaceAttributes, SurfaceEvent, SurfaceId, WakeReason,
    };
    use zgui_vocab::DragData;

    use crate::harness::Harness;

    /// An application that records each drag stage it receives, with the surface it arrived at.
    #[derive(Default)]
    struct Recorder {
        /// The surface it created.
        surface: Option<SurfaceId>,
        /// The drag stages in arrival order.
        drags: Vec<(SurfaceId, DragEvent)>,
    }

    impl AppHandler for Recorder {
        fn surfaces_available(&mut self, cx: &dyn PlatformCx) {
            let surface = cx
                .create_surface(&SurfaceAttributes::new("drop target"))
                .expect("headless surfaces are always creatable");
            self.surface = Some(surface.id());
        }

        fn surface_event(&mut self, _cx: &dyn PlatformCx, id: SurfaceId, event: SurfaceEvent) {
            if let SurfaceEvent::Drag(drag) = event {
                self.drags.push((id, drag));
            }
        }

        fn wake(&mut self, _cx: &dyn PlatformCx, _reason: WakeReason) {}
    }

    #[test]
    fn each_stage_reaches_the_first_surface_in_order() {
        let mut harness = Harness::new(Recorder::default());
        let data = DragData::from_paths(vec![PathBuf::from("/tmp/a.wav")]);
        let at = Point::new(CssPx(5.0), CssPx(6.0));
        harness.drag_enter(data.clone(), at);
        harness.drag_move(at);
        harness.drag_leave();
        harness.drag_enter(data.clone(), at);
        harness.drag_drop(data.clone(), at);

        let surface = harness.app().surface.expect("a surface was created");
        let expected = [
            DragEvent::Entered {
                data: data.clone(),
                position: at,
            },
            DragEvent::Moved { position: at },
            DragEvent::Left,
            DragEvent::Entered {
                data: data.clone(),
                position: at,
            },
            DragEvent::Dropped { data, position: at },
        ]
        .map(|drag| (surface, drag));
        assert_eq!(harness.app().drags, expected);
    }
}
