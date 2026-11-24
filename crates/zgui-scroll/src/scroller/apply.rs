//! Writing an offset: a delta shared out along a chain, and a destination asked for outright.

use smallvec::SmallVec;
use zgui_dom::NodeKey;
use zgui_geom::{Device, DevicePx, Point, Size};
use zgui_layout::LayoutStore;
use zgui_layout::scroll_region::code_region_of_element;

use crate::chain;
use crate::motion::{Behavior, Momentum, Motion, Pace, Tween};
use crate::scroller::Scroller;
use crate::stretch::Stretch;

impl Scroller {
    /// Shares `delta` out along `chain`, innermost container first.
    ///
    /// Each container takes what it has room for and hands the rest outwards; what survives the
    /// outermost one displaces an edge past its end, elastically, on each axis some container in
    /// the chain scrolls. The result names every
    /// container whose *composed* position changed, which is what the caller marks — and that
    /// includes one displaced past its end, because the content is drawn somewhere else even though
    /// its reported offset did not move.
    ///
    /// A container that is asked to move is taken off any motion it was on: a wheel turned during a
    /// smooth scroll means the person has taken over, and continuing to interpolate towards a
    /// destination they have since scrolled away from is the "it fights me" bug.
    pub fn scroll_by(
        &mut self,
        store: &LayoutStore,
        chain: &[NodeKey],
        delta: Size<DevicePx, Device>,
        stretch: Stretch,
    ) -> SmallVec<[NodeKey; 2]> {
        let mut moved: SmallVec<[NodeKey; 2]> = SmallVec::new();
        if chain::negligible(delta) {
            return moved;
        }
        let mut left = self.unwind(store, chain, delta, &mut moved);
        for container in chain.iter().copied() {
            let at = self.offset_of(container);
            let share = chain::absorb(at, self.hand_limit_for(store, container), left);
            left = share.left;
            if !chain::negligible(share.taken) {
                self.motions.remove(&container);
                let to = Point::new(
                    DevicePx(at.x.0 + share.taken.width.0),
                    DevicePx(at.y.0 + share.taken.height.0),
                );
                let landed = self.offsets.scroll_to(store, container, to);
                self.compose(container);
                self.record(container, at, landed);
                moved.push(container);
            }
            if chain::negligible(left) {
                break;
            }
        }
        if stretch.is_permitted() && !chain::negligible(left) {
            for container in self.stretch_past_end(store, chain, left) {
                if !moved.contains(&container) {
                    moved.push(container);
                }
            }
        }
        moved
    }

    /// Puts one container at `to`, now or over the next few frames.
    ///
    /// Returns where it landed for an instant scroll and where it is heading for a smooth one, or
    /// nothing when the container does not scroll at all.
    pub fn scroll_to(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        to: Point<DevicePx, Device>,
        behavior: Behavior,
    ) -> Option<Point<DevicePx, Device>> {
        let limit = code_region_of_element(store, container)?.limit();
        let clamped = Point::new(
            DevicePx(to.x.0.clamp(0.0, limit.x.0)),
            DevicePx(to.y.0.clamp(0.0, limit.y.0)),
        );
        let at = self.offset_of(container);
        match behavior {
            Behavior::Auto | Behavior::Instant => {
                self.motions.remove(&container);
                if at == clamped {
                    return Some(clamped);
                }
                let landed = self.offsets.scroll_to(store, container, clamped);
                self.compose(container);
                self.record(container, at, landed);
                Some(landed)
            }
            Behavior::Smooth => self.travel_to(store, container, to, Pace::Standard),
        }
    }

    /// Moves one container to `to` over the next few frames, at `pace`.
    ///
    /// Returns where it is heading, or nothing when the container does not scroll at all.
    pub fn travel_to(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        to: Point<DevicePx, Device>,
        pace: Pace,
    ) -> Option<Point<DevicePx, Device>> {
        let limit = code_region_of_element(store, container)?.limit();
        let clamped = Point::new(
            DevicePx(to.x.0.clamp(0.0, limit.x.0)),
            DevicePx(to.y.0.clamp(0.0, limit.y.0)),
        );
        let at = self.offset_of(container);
        if at == clamped {
            self.motions.remove(&container);
            return Some(clamped);
        }
        self.motions.insert(
            container,
            Motion::Smooth(Tween::new(at, clamped).with_pace(pace)),
        );
        Some(clamped)
    }

    /// Where one container is heading: the destination of its smooth scroll, or where it is.
    pub fn heading(&self, container: NodeKey) -> Point<DevicePx, Device> {
        match self.motion_of(container) {
            Some(Motion::Smooth(tween)) => tween.destination(),
            _ => self.offset_of(container),
        }
    }

    /// Moves one container by `delta` at once, and keeps any motion it is on.
    ///
    /// For content that changed height above the scrollport, which the offset follows so that what
    /// is on screen stays put. A smooth scroll keeps going, its start and destination moved by as
    /// much as the offset was. A fling reads the offset as it advances, so it keeps its speed.
    ///
    /// Returns where it landed, or nothing when the container does not scroll at all.
    pub fn adjust_by(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        delta: Size<DevicePx, Device>,
    ) -> Option<Point<DevicePx, Device>> {
        let limit = code_region_of_element(store, container)?.limit();
        let at = self.offset_of(container);
        let clamped = Point::new(
            DevicePx((at.x.0 + delta.width.0).clamp(0.0, limit.x.0)),
            DevicePx((at.y.0 + delta.height.0).clamp(0.0, limit.y.0)),
        );
        if at == clamped {
            return Some(clamped);
        }

        let landed = self.offsets.scroll_to(store, container, clamped);
        if let Some(Motion::Smooth(tween)) = self.motion_mut(container) {
            tween.translate(Size::new(
                DevicePx(landed.x.0 - at.x.0),
                DevicePx(landed.y.0 - at.y.0),
            ));
        }
        self.compose(container);
        self.record(container, at, landed);

        Some(landed)
    }

    /// Hands one container the speed a gesture let go of it with, in device pixels per second.
    ///
    /// The fling decays as UIScrollView's does. When it reaches an end, it bounces past it if
    /// `stretch` permits and the content overflows on that axis, and stops there if not.
    ///
    /// A fling towards an edge the container is already at is not installed: it would spend the
    /// next twenty frames asking to move a container that cannot, which is a deadline per frame for
    /// nothing.
    pub fn fling(
        &mut self,
        store: &LayoutStore,
        container: NodeKey,
        velocity: Size<DevicePx, Device>,
        stretch: Stretch,
    ) {
        if crate::motion::stopped(velocity) {
            return;
        }
        let at = self.offset_of(container);
        let share = chain::absorb(at, self.hand_limit_for(store, container), velocity);
        if chain::negligible(share.taken) {
            return;
        }
        self.motions
            .insert(container, Motion::Fling(Momentum::new(velocity, stretch)));
    }
}
