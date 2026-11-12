//! The wake edge: how anything that is not an input event asks for a frame.
//!
//! A parked event loop is blocked on the windowing system, not on the reactive layer. So a signal
//! written on a worker thread, a future resolving, an image finishing its decode and a background
//! build completing all mark work ready and then wait for the user to move the mouse — unless
//! something pings the loop from where the work happened. That ping is this module.
//!
//! It has two halves, and the second is what keeps the first from being a stampede. A wake raised
//! from *outside* a frame goes to the platform, which asks the surfaces it belongs to for a frame.
//! A wake raised from *inside* one is folded into "another frame is owed" instead: an effect that
//! writes a signal would otherwise ask the loop for a frame from inside the frame that is already
//! running, once per write.

use std::fmt;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use zgui_platform::{SurfaceId, WakeReason, Waker};
use zgui_reactive::FrameWaker;

// The `loom` feature uses the model checker's atomic, so a test can explore each interleaving of a
// wake with a frame.
#[cfg(feature = "loom")]
use loom::sync::atomic::AtomicU8;
#[cfg(not(feature = "loom"))]
use std::sync::atomic::AtomicU8;

/// Whether a frame is in flight, and whether one is owed when it ends.
///
/// Shared with every waker built over it, and read from arbitrary threads: a wake arrives from
/// wherever the work finished, which is precisely why this cannot be thread-local state.
///
/// The two facts share one atomic state. A request tests for a frame in flight and records itself
/// in one step, so a frame cannot end between the test and the record. A request releases, and
/// [`requests_serviced`](Self::requests_serviced) and [`end_frame`](Self::end_frame) acquire, so
/// the frame that takes a request also sees the work published before it.
#[derive(Debug, Default)]
pub struct FrameGate {
    /// Whether a frame is in flight, and whether another is owed.
    state: AtomicGateState,
}

impl FrameGate {
    /// A gate with no frame in flight and nothing owed.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a frame is in flight.
    pub fn in_frame(&self) -> bool {
        self.state.load(Ordering::Relaxed) != GateState::Idle
    }

    /// Whether another frame is owed.
    pub fn needs_another_frame(&self) -> bool {
        self.state.load(Ordering::Relaxed) == GateState::Owed
    }

    /// Records that a request arrived, and reports whether it was deferred rather than forwarded.
    ///
    /// Every in-frame requester routes through here — a mutation closing its batch, a timer's
    /// callback pinging the executor, an observation delivery writing a slot, and the reactive
    /// flush itself — so that the frame's last phase converts all of them into exactly one request
    /// rather than each of them producing one.
    pub fn request(&self) -> bool {
        self.state
            .fetch_update(Ordering::Release, Ordering::Relaxed, |state| match state {
                GateState::Idle => None,
                GateState::InFrame | GateState::Owed => Some(GateState::Owed),
            })
            .is_ok()
    }

    /// Records that the stage which services what has been asked for is about to run.
    ///
    /// A frame flushes the reactive graph part-way through itself, so a wake raised before that
    /// point — by a handler running in this frame's own dispatch, by a timer's callback, by a task
    /// that finished while this frame was starting — is answered by this frame and owes nothing.
    /// Without this every interaction bought a second frame that damaged nothing and presented a
    /// surface identical to the one just presented.
    ///
    /// It is called immediately before the flush, and that is what makes it safe against a wake
    /// raised on another thread: a wake is sent after the work it announces has been published, so
    /// a wake this clears was published before the flush that follows and is therefore serviced by
    /// it. Called *after* the flush, the same line would lose the work of anything that arrived in
    /// between.
    pub fn requests_serviced(&self) {
        // The closure stores a state for each state, so the update always succeeds and acquires.
        let _ =
            self.state
                .fetch_update(Ordering::Acquire, Ordering::Relaxed, |state| match state {
                    GateState::Owed => Some(GateState::InFrame),
                    GateState::Idle | GateState::InFrame => Some(state),
                });
    }

    /// Marks the start of a frame, clearing what the last one owed.
    pub fn begin_frame(&self) {
        self.state.swap(GateState::InFrame, Ordering::Relaxed);
    }

    /// Marks the end of a frame, and reports whether anything asked for another during it.
    pub fn end_frame(&self) -> bool {
        self.state.swap(GateState::Idle, Ordering::Acquire) == GateState::Owed
    }
}

/// The reactive layer's wake, routed to the platform.
///
/// The surfaces are named rather than left implicit, because an image decoding for one window is
/// not a reason to redraw another.
pub struct RuntimeWaker {
    /// Where a wake goes when no frame is running.
    platform: Arc<dyn Waker>,
    /// Whether a frame is running, and what it owes.
    gate: Arc<FrameGate>,
    /// The surfaces this wake concerns.
    surfaces: Mutex<Vec<SurfaceId>>,
}

impl RuntimeWaker {
    /// A waker that pings `platform` for the surfaces registered on it.
    pub fn new(platform: Arc<dyn Waker>, gate: Arc<FrameGate>) -> Self {
        Self {
            platform,
            gate,
            surfaces: Mutex::new(Vec::new()),
        }
    }

    /// Whether a frame is running, and what it owes.
    pub fn gate(&self) -> &Arc<FrameGate> {
        &self.gate
    }

    /// Adds a surface to the set a wake concerns.
    pub fn owns(&self, surface: SurfaceId) {
        let mut surfaces = self.surfaces.lock().expect("the set is not poisoned");
        if !surfaces.contains(&surface) {
            surfaces.push(surface);
        }
    }

    /// Removes a surface from that set, which a closed window does.
    pub fn disowns(&self, surface: SurfaceId) {
        self.surfaces
            .lock()
            .expect("the set is not poisoned")
            .retain(|held| *held != surface);
    }
}

impl FrameWaker for RuntimeWaker {
    fn wake(&self) {
        if self.gate.request() {
            return;
        }
        // With no surface the wake still goes out, naming none. An application with no window open
        // has no frame to flush its work, and the runtime flushes it on the wake instead.
        let surfaces = self
            .surfaces
            .lock()
            .expect("the set is not poisoned")
            .clone();
        self.platform.wake(WakeReason::ReactiveWork {
            surfaces: surfaces.into_boxed_slice(),
        });
    }
}

/// The state of a [`FrameGate`].
///
/// A request is owed only while a frame is in flight, so there is no state for an owed request
/// with no frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum GateState {
    /// No frame is in flight.
    Idle = 0,
    /// A frame is in flight and owes nothing.
    InFrame = 1,
    /// A frame is in flight, and something asked for another frame during it.
    Owed = 2,
}

impl GateState {
    /// The state that `bits` encodes.
    fn from_bits(bits: u8) -> Self {
        match bits {
            0 => Self::Idle,
            1 => Self::InFrame,
            2 => Self::Owed,
            _ => unreachable!("an AtomicGateState stores only GateState values, not {bits}"),
        }
    }
}

/// A [`GateState`] that threads read and write atomically.
struct AtomicGateState(AtomicU8);

impl AtomicGateState {
    /// An atomic that holds `state`.
    fn new(state: GateState) -> Self {
        Self(AtomicU8::new(state as u8))
    }

    /// The state, read with `order`.
    fn load(&self, order: Ordering) -> GateState {
        GateState::from_bits(self.0.load(order))
    }

    /// Stores `state` with `order`, and gives the state it replaced.
    fn swap(&self, state: GateState, order: Ordering) -> GateState {
        GateState::from_bits(self.0.swap(state as u8, order))
    }

    /// Stores the state that `update` gives for the current state. Where another thread changes
    /// the state between the read and the store, it reads the state again and calls `update` again.
    ///
    /// Gives `Ok` with the replaced state, or `Err` with the current state where `update` gives
    /// `None`. The orderings are those of [`AtomicU8::fetch_update`].
    fn fetch_update(
        &self,
        set_order: Ordering,
        fetch_order: Ordering,
        mut update: impl FnMut(GateState) -> Option<GateState>,
    ) -> Result<GateState, GateState> {
        self.0
            .fetch_update(set_order, fetch_order, |bits| {
                update(GateState::from_bits(bits)).map(|state| state as u8)
            })
            .map(GateState::from_bits)
            .map_err(GateState::from_bits)
    }
}

impl Default for AtomicGateState {
    fn default() -> Self {
        Self::new(GateState::Idle)
    }
}

impl fmt::Debug for AtomicGateState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("AtomicGateState")
            .field(&self.load(Ordering::Relaxed))
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameGate, RuntimeWaker};
    use std::sync::{Arc, Mutex};
    use zgui_platform::{SurfaceId, WakeReason, Waker};
    use zgui_reactive::FrameWaker;

    /// A platform waker that records what it was handed.
    #[derive(Default)]
    struct Recording(Mutex<Vec<Vec<SurfaceId>>>);

    impl Waker for Recording {
        fn wake(&self, reason: WakeReason) {
            self.0
                .lock()
                .expect("not poisoned")
                .push(reason.surfaces().to_vec());
        }
    }

    #[test]
    fn a_wake_outside_a_frame_reaches_the_platform_naming_its_own_surfaces() {
        let platform = Arc::new(Recording::default());
        let gate = Arc::new(FrameGate::new());
        let waker = RuntimeWaker::new(Arc::clone(&platform) as Arc<dyn Waker>, gate);
        waker.owns(SurfaceId::new(7));

        waker.wake();

        let recorded = platform.0.lock().expect("not poisoned");
        assert_eq!(recorded.as_slice(), [vec![SurfaceId::new(7)]]);
    }

    #[test]
    fn a_wake_inside_a_frame_is_owed_rather_than_sent() {
        let platform = Arc::new(Recording::default());
        let gate = Arc::new(FrameGate::new());
        let waker = RuntimeWaker::new(Arc::clone(&platform) as Arc<dyn Waker>, Arc::clone(&gate));
        waker.owns(SurfaceId::new(1));

        gate.begin_frame();
        for _ in 0..1_000 {
            waker.wake();
        }
        assert!(
            platform.0.lock().expect("not poisoned").is_empty(),
            "a thousand in-frame wakes must not become a thousand redraw requests"
        );
        assert!(gate.end_frame(), "and the frame still owes exactly one");
        assert!(!gate.needs_another_frame(), "which is taken by reading it");
    }

    #[test]
    fn a_wake_belonging_to_no_surface_wakes_the_loop_and_names_no_surface() {
        // An application with no window open still has work to flush, so the loop must wake. No
        // surface is named, so no surface draws for it.
        let platform = Arc::new(Recording::default());
        let gate = Arc::new(FrameGate::new());
        let waker = RuntimeWaker::new(Arc::clone(&platform) as Arc<dyn Waker>, gate);
        waker.owns(SurfaceId::new(3));
        waker.disowns(SurfaceId::new(3));
        waker.wake();
        let recorded = platform.0.lock().expect("not poisoned");
        assert_eq!(recorded.as_slice(), [Vec::<SurfaceId>::new()]);
    }
}
