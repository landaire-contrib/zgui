//! The wake edge: how work that becomes ready outside an input event reaches the frame loop.
//!
//! The task pool's own waker unparks the thread it lives on. That is worthless in a UI, where
//! the thread is not parked but blocked in the platform's event loop waiting for a window
//! message. Without a second edge, a task woken by a worker thread, a resolved async value or a
//! fired timer is marked ready and then waits for the user to move the mouse.
//!
//! So every spawned future is polled through a waker that does both: waking it queues the task in
//! the pool *and* pings each registered [`FrameWaker`], which asks the platform for a redraw. The
//! redraw runs the frame, the frame flushes the executor, and the task is polled.

use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};

/// The platform's redraw request, as the reactive layer sees it.
///
/// A wake can arrive on any thread at any time, so the implementation must be safe to call from
/// a worker thread, from a timer, and re-entrantly. It must also be *idempotent*: a hundred
/// wakes between two frames must cost one frame, not a hundred.
///
/// The windowing backend provides the real implementation; [`TestWaker`] is the one to use in
/// tests and in headless harnesses.
pub trait FrameWaker: Send + Sync + 'static {
    /// Requests that a frame be run soon.
    ///
    /// Called from arbitrary threads. Must not block, and must not assume a current owner or
    /// the UI thread.
    fn wake(&self);
}

/// Where wakes are sent on one UI thread.
///
/// Shared by every task waker built on that thread, so a waker registered after effects already
/// exist still gets their wakes.
#[derive(Default)]
pub(crate) struct WakeTarget {
    /// The registered wakers. A change replaces the whole list, so a ping clones one pointer and
    /// calls the wakers without the lock.
    wakers: RwLock<Arc<[Registered]>>,
    /// The identifier of the next registration.
    next: AtomicU64,
}

/// The identifier of one registration in a [`WakeTarget`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct RegistrationId(u64);

/// One registered waker.
#[derive(Clone)]
struct Registered {
    /// The registration that holds this waker.
    id: RegistrationId,
    /// The waker.
    waker: Arc<dyn FrameWaker>,
}

impl WakeTarget {
    /// Pings each registered waker.
    pub(crate) fn ping(&self) {
        let wakers = Arc::clone(&self.wakers.read().unwrap_or_else(PoisonError::into_inner));
        for registered in wakers.iter() {
            registered.waker.wake();
        }
    }

    /// Adds `waker` to the list until the returned registration is dropped.
    fn register(self: &Arc<Self>, waker: Arc<dyn FrameWaker>) -> FrameWakerRegistration {
        let id = RegistrationId(self.next.fetch_add(1, Ordering::Relaxed));
        let mut wakers = self.wakers.write().unwrap_or_else(PoisonError::into_inner);
        *wakers = wakers
            .iter()
            .cloned()
            .chain(std::iter::once(Registered { id, waker }))
            .collect();
        FrameWakerRegistration {
            target: Arc::clone(self),
            id,
        }
    }

    /// Removes the waker of registration `id`.
    fn unregister(&self, id: RegistrationId) {
        let mut wakers = self.wakers.write().unwrap_or_else(PoisonError::into_inner);
        let kept = wakers
            .iter()
            .filter(|registered| registered.id != id)
            .cloned()
            .collect();
        let removed = std::mem::replace(&mut *wakers, kept);
        drop(wakers);
        // The removed waker can hold a registration on this target, so it is dropped after the
        // lock is released.
        drop(removed);
    }
}

thread_local! {
    /// This thread's wake target. Created eagerly so task wakers can hold it before a platform
    /// waker exists.
    static TARGET: Arc<WakeTarget> = Arc::new(WakeTarget::default());

    /// The registration that [`set_frame_waker`] made last on this thread.
    static SET: RefCell<Option<FrameWakerRegistration>> = const { RefCell::new(None) };
}

/// Keeps a [`FrameWaker`] registered on the thread that registered it.
///
/// Drop the registration to remove the waker. The waker then gets no more wakes.
#[must_use = "dropping the registration removes the waker"]
pub struct FrameWakerRegistration {
    /// The wake target that holds the waker.
    target: Arc<WakeTarget>,
    /// The waker's entry in that target.
    id: RegistrationId,
}

impl std::fmt::Debug for FrameWakerRegistration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FrameWakerRegistration")
            .field("id", &self.id.0)
            .finish_non_exhaustive()
    }
}

impl Drop for FrameWakerRegistration {
    fn drop(&mut self) {
        self.target.unregister(self.id);
    }
}

/// Sends reactive wakes on this thread to `waker` until the returned registration is dropped.
///
/// Call from the UI thread, as soon as a window exists. Until a waker is registered, a task woken
/// from another thread is queued, but nothing asks for the frame that polls it. An application
/// with no waker then updates only when something else causes a frame.
///
/// A thread can have many wakers, for example one for each application that runs on it. Each
/// wake goes to all of them.
///
/// Wakes raised *during* a flush are not forwarded: the flush polls what they woke before it
/// returns, so an effect writing a signal costs nothing beyond the poll. A wake the flush could
/// not service is folded into
/// [`FlushOutcome::needs_another_frame`](crate::executor::FlushOutcome::needs_another_frame)
/// instead, which is one frame rather than a redraw request per write.
///
/// ```
/// use std::sync::Arc;
/// use zgui_reactive::{TestWaker, register_frame_waker};
///
/// let waker = Arc::new(TestWaker::default());
/// let registration = register_frame_waker(waker.clone());
/// drop(registration);
/// ```
pub fn register_frame_waker(waker: Arc<dyn FrameWaker>) -> FrameWakerRegistration {
    TARGET.with(|target| target.register(waker))
}

/// Sends reactive wakes on this thread to `waker` until the thread ends.
///
/// Each call removes the waker of the previous call on this thread. Wakers registered with
/// [`register_frame_waker`] stay registered.
pub fn set_frame_waker(waker: Arc<dyn FrameWaker>) {
    let registration = register_frame_waker(waker);
    let previous = SET.with(|set| set.borrow_mut().replace(registration));
    drop(previous);
}

/// This thread's wake target.
pub(crate) fn target() -> Arc<WakeTarget> {
    TARGET.with(Arc::clone)
}

/// A [`FrameWaker`] that counts wakes instead of asking a platform for a frame.
///
/// Useful wherever there is no event loop: a test asserts that work became ready by asserting
/// that the count moved, and a headless harness polls the count to decide whether to run
/// another frame.
///
/// ```
/// use std::sync::Arc;
/// use zgui_reactive::{FrameWaker, TestWaker};
///
/// let waker = Arc::new(TestWaker::default());
/// assert_eq!(waker.count(), 0);
/// waker.wake();
/// assert_eq!(waker.count(), 1);
/// assert_eq!(waker.take(), 1);
/// assert_eq!(waker.count(), 0);
/// ```
#[derive(Debug, Default)]
pub struct TestWaker {
    /// Wakes recorded since the last [`TestWaker::take`].
    count: std::sync::atomic::AtomicUsize,
}

impl TestWaker {
    /// The number of wakes recorded since the last [`TestWaker::take`].
    #[must_use]
    pub fn count(&self) -> usize {
        self.count.load(std::sync::atomic::Ordering::Acquire)
    }

    /// Returns the number of recorded wakes and resets the count to zero.
    pub fn take(&self) -> usize {
        self.count.swap(0, std::sync::atomic::Ordering::AcqRel)
    }
}

impl FrameWaker for TestWaker {
    fn wake(&self) {
        self.count.fetch_add(1, std::sync::atomic::Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pings this thread's wake target from a worker thread, as a remote task wake does.
    fn ping_from_worker() {
        let target = target();
        std::thread::spawn(move || target.ping())
            .join()
            .expect("the worker does not panic");
    }

    #[test]
    fn each_wake_goes_to_every_registered_waker() {
        let first = Arc::new(TestWaker::default());
        let second = Arc::new(TestWaker::default());
        let _first = register_frame_waker(first.clone());
        let _second = register_frame_waker(second.clone());

        ping_from_worker();

        assert_eq!(first.take(), 1);
        assert_eq!(second.take(), 1);
    }

    #[test]
    fn a_dropped_registration_gets_no_more_wakes_and_the_others_still_do() {
        let kept = Arc::new(TestWaker::default());
        let dropped = Arc::new(TestWaker::default());
        let _kept = register_frame_waker(kept.clone());
        let registration = register_frame_waker(dropped.clone());

        drop(registration);
        ping_from_worker();

        assert_eq!(kept.take(), 1);
        assert_eq!(dropped.take(), 0);
        assert_eq!(
            Arc::strong_count(&dropped),
            1,
            "the target released the waker"
        );
    }

    #[test]
    fn a_waker_that_holds_a_registration_can_be_unregistered() {
        /// A waker that owns the registration of another waker on the same thread.
        struct Holder {
            /// Dropped with the holder.
            _inner: FrameWakerRegistration,
        }

        impl FrameWaker for Holder {
            fn wake(&self) {}
        }

        let inner = Arc::new(TestWaker::default());
        let holder = Holder {
            _inner: register_frame_waker(inner.clone()),
        };
        let registration = register_frame_waker(Arc::new(holder));

        drop(registration);
        ping_from_worker();

        assert_eq!(inner.take(), 0, "the inner waker went with its holder");
    }

    #[test]
    fn set_frame_waker_replaces_its_own_waker_and_keeps_registered_ones() {
        let registered = Arc::new(TestWaker::default());
        let replaced = Arc::new(TestWaker::default());
        let current = Arc::new(TestWaker::default());
        let _registered = register_frame_waker(registered.clone());
        set_frame_waker(replaced.clone());
        set_frame_waker(current.clone());

        ping_from_worker();

        assert_eq!(registered.take(), 1);
        assert_eq!(replaced.take(), 0);
        assert_eq!(current.take(), 1);
    }
}
