//! Model checks for a wake that arrives from another thread while a frame runs.
//!
//! Run with `cargo test -p zgui-runtime --features loom --test frame_gate_loom`. Without the
//! feature the gate uses the standard library's atomic and there is no model to explore, so this
//! file compiles to nothing.
//!
//! The property is that the gate loses no wake. However the waking thread and the frame
//! interleave, the frame sees the work the wake announces, the frame owes another frame, or the
//! wake goes to the platform. A lost wake stalls the window until the next input event.

#![cfg(feature = "loom")]

use std::sync::Arc;

use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use zgui_platform::{SurfaceId, WakeReason, Waker};
use zgui_reactive::FrameWaker;
use zgui_runtime::{FrameGate, RuntimeWaker};

/// A platform waker that counts the wakes it gets.
#[derive(Default)]
struct Counting(AtomicUsize);

impl Waker for Counting {
    fn wake(&self, _reason: WakeReason) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }
}

/// A gate, and a waker over it that names one surface.
struct Rig {
    /// The gate the frame runs under.
    gate: Arc<FrameGate>,
    /// Where a wake goes when no frame is in flight.
    platform: Arc<Counting>,
    /// The wake route a worker thread uses.
    waker: Arc<RuntimeWaker>,
}

impl Rig {
    /// A rig with no frame in flight.
    fn new() -> Self {
        let gate = Arc::new(FrameGate::new());
        let platform = Arc::new(Counting::default());
        let waker = Arc::new(RuntimeWaker::new(
            Arc::clone(&platform) as Arc<dyn Waker>,
            Arc::clone(&gate),
        ));
        waker.owns(SurfaceId::new(1));

        Self {
            gate,
            platform,
            waker,
        }
    }

    /// How many wakes reached the platform.
    fn sent(&self) -> usize {
        self.platform.0.load(Ordering::Relaxed)
    }
}

#[test]
fn a_wake_beside_the_end_of_a_frame_asks_for_exactly_one_frame() {
    loom::model(|| {
        let rig = Rig::new();
        rig.gate.begin_frame();

        let worker = {
            let waker = Arc::clone(&rig.waker);
            loom::thread::spawn(move || waker.wake())
        };
        let owed = rig.gate.end_frame();
        worker.join().expect("the waking thread panicked");

        let sent = rig.sent();
        assert_eq!(
            usize::from(owed) + sent,
            1,
            "the frame owed {owed} and the platform got {sent} wakes"
        );
    });
}

#[test]
fn a_wake_at_any_point_of_a_frame_is_serviced_or_asks_for_one_frame() {
    loom::model(|| {
        let rig = Rig::new();
        let work = Arc::new(AtomicBool::new(false));

        let worker = {
            let waker = Arc::clone(&rig.waker);
            let work = Arc::clone(&work);
            loom::thread::spawn(move || {
                work.store(true, Ordering::Release);
                waker.wake();
            })
        };
        rig.gate.begin_frame();
        rig.gate.requests_serviced();
        let flushed = work.load(Ordering::Acquire);
        let owed = rig.gate.end_frame();
        // Read before the join, which orders the work before this thread by itself.
        let visible_at_end = work.load(Ordering::Acquire);
        worker.join().expect("the waking thread panicked");

        let sent = rig.sent();
        assert!(
            flushed || owed || sent > 0,
            "the frame cleared the wake, did not see its work, and asked for no frame"
        );
        assert!(
            !owed || visible_at_end,
            "the frame took a request and did not see the work published before it"
        );
        assert!(
            usize::from(owed) + sent <= 1,
            "one wake made the frame owe {owed} and reached the platform {sent} times"
        );
    });
}
