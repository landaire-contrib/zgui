//! The pages of a navigation stack, and the motion between them.

use zgui::prelude::*;
use zgui::reactive::LocalStorage;
use zgui::reactive::RwSignal;
use zgui::reactive::StoredValue;

use crate::nav_stack::error::PopError;
use crate::nav_stack::page::PageBar;

/// The identity of one pushed page.
///
/// Each push makes a new identity, so the same route pushed twice gives two pages.
#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct PageId(u64);

/// One page of a [`NavPath`].
#[derive(Clone)]
pub(crate) struct Entry<P> {
    /// The identity of the page.
    pub(crate) id: PageId,
    /// The route the caller pushed.
    pub(crate) page: P,
}

/// The motion that is in progress.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Motion {
    /// No page moves.
    Still,
    /// `to` slides in over `from`.
    Push {
        /// The page that becomes covered.
        from: PageId,
        /// The page that becomes the top page.
        to: PageId,
    },
    /// `from` slides out and shows `to`.
    Pop {
        /// The page that goes away.
        from: PageId,
        /// The page that becomes the top page.
        to: PageId,
    },
    /// A finger drags `from` aside to show `to`, or has let go of it.
    Swipe {
        /// The top page.
        from: PageId,
        /// The page below it.
        to: PageId,
        /// How the swipe ends, once the finger has let go.
        end: Option<SwipeEnd>,
    },
}

/// How a swipe back ends.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum SwipeEnd {
    /// The top page slides out, as in a pop.
    Complete,
    /// The top page slides back.
    Cancel,
}

/// Where one mounted page is drawn.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum PagePlace {
    /// The top page, at rest.
    Current,
    /// A page below the top page, at rest. It is hidden.
    Covered,
    /// The pushed page, which slides in.
    Entering,
    /// The page that the pushed page covers.
    Departing,
    /// The popped page, which slides out.
    Leaving,
    /// The page that the popped page shows.
    Returning,
    /// The top page, which a finger drags aside.
    Swiped,
    /// The page below a dragged page.
    Uncovered,
    /// The top page, which slides out after a swipe.
    SwipedOut,
    /// The page that a completed swipe shows.
    SwipedIn,
    /// The top page, which slides back after a cancelled swipe.
    SwipedBack,
    /// The page below the top page, which a cancelled swipe covers again.
    Recovered,
}

impl PagePlace {
    /// The value of the `data-place` attribute, which the style sheet selects on.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Current => "current",
            Self::Covered => "covered",
            Self::Entering => "push-in",
            Self::Departing => "push-out",
            Self::Leaving => "pop-out",
            Self::Returning => "pop-in",
            Self::Swiped => "swipe-top",
            Self::Uncovered => "swipe-below",
            Self::SwipedOut => "swipe-out",
            Self::SwipedIn => "swipe-in",
            Self::SwipedBack => "swipe-back",
            Self::Recovered => "swipe-cover",
        }
    }

    /// Whether the page is the top page, or becomes it when the motion ends.
    pub(crate) const fn is_active(self) -> bool {
        matches!(
            self,
            Self::Current
                | Self::Entering
                | Self::Returning
                | Self::Swiped
                | Self::SwipedIn
                | Self::SwipedBack
        )
    }

    /// Whether the end of this page's animation or transition ends the motion.
    pub(crate) const fn ends_motion(self) -> bool {
        matches!(
            self,
            Self::Entering | Self::Leaving | Self::SwipedOut | Self::SwipedBack
        )
    }
}

/// The part of a path that is the same for every route type.
#[derive(Copy, Clone)]
pub(crate) struct NavCore {
    /// The identities of the mounted pages, bottom first. A popped page that still moves is last.
    order: RwSignal<Vec<PageId>, LocalStorage>,
    /// The motion in progress.
    motion: RwSignal<Motion, LocalStorage>,
    /// The bar of each mounted page that declared one.
    bars: RwSignal<Vec<(PageId, PageBar)>, LocalStorage>,
    /// How far a swipe has dragged the top page, from 0 to 1 of the width.
    progress: RwSignal<f32, LocalStorage>,
    /// The curve of the motion after a swipe, which starts at the speed of the finger.
    ease: RwSignal<Option<String>, LocalStorage>,
    /// The next identity to give out.
    next: StoredValue<u64, LocalStorage>,
}

impl NavCore {
    fn new() -> Self {
        Self {
            order: RwSignal::new_local(Vec::new()),
            motion: RwSignal::new_local(Motion::Still),
            bars: RwSignal::new_local(Vec::new()),
            progress: RwSignal::new_local(0.0),
            ease: RwSignal::new_local(None),
            next: StoredValue::new_local(0),
        }
    }

    fn allocate(self) -> PageId {
        let id = self.next.get_value();
        self.next.set_value(id + 1);

        PageId(id)
    }

    fn dispose(self) {
        self.order.dispose();
        self.motion.dispose();
        self.bars.dispose();
        self.progress.dispose();
        self.ease.dispose();
        self.next.dispose();
    }

    /// The motion in progress, subscribing to it.
    pub(crate) fn motion(self) -> Motion {
        self.motion.get()
    }

    /// How far a swipe has dragged the top page, subscribing to it.
    pub(crate) fn progress(self) -> f32 {
        self.progress.get()
    }

    /// Moves the page that a finger drags to `progress` of the width.
    pub(crate) fn drag_to(self, progress: f32) {
        self.progress.set(progress.clamp(0.0, 1.0));
    }

    /// The curve of the motion after a swipe, subscribing to it.
    pub(crate) fn ease(self) -> Option<String> {
        self.ease.get()
    }

    /// How far a swipe has dragged the top page.
    pub(crate) fn progress_untracked(self) -> f32 {
        self.progress.get_untracked()
    }

    /// Where the page `id` is drawn, subscribing to the stack.
    pub(crate) fn place(self, id: PageId) -> PagePlace {
        match self.motion.get() {
            Motion::Push { from, to } => {
                if id == to {
                    PagePlace::Entering
                } else if id == from {
                    PagePlace::Departing
                } else {
                    PagePlace::Covered
                }
            }
            Motion::Pop { from, to } => {
                if id == from {
                    PagePlace::Leaving
                } else if id == to {
                    PagePlace::Returning
                } else {
                    PagePlace::Covered
                }
            }
            Motion::Swipe { from, to, end } => {
                let (top, below) = match end {
                    None => (PagePlace::Swiped, PagePlace::Uncovered),
                    Some(SwipeEnd::Complete) => (PagePlace::SwipedOut, PagePlace::SwipedIn),
                    Some(SwipeEnd::Cancel) => (PagePlace::SwipedBack, PagePlace::Recovered),
                };
                if id == from {
                    top
                } else if id == to {
                    below
                } else {
                    PagePlace::Covered
                }
            }
            Motion::Still => {
                let top = self.order.with(|order| order.last().copied());
                if top == Some(id) {
                    PagePlace::Current
                } else {
                    PagePlace::Covered
                }
            }
        }
    }

    /// Records the bar that the page `id` declared.
    pub(crate) fn register(self, id: PageId, bar: PageBar) {
        self.bars.update(|bars| {
            bars.retain(|(page, _)| *page != id);
            bars.push((id, bar));
        });
    }

    /// Forgets the bar of the page `id`. Does nothing once the stack itself is gone.
    pub(crate) fn unregister(self, id: PageId) {
        self.bars
            .try_update(|bars| bars.retain(|(page, _)| *page != id));
    }

    /// The bars of the mounted pages, bottom first, subscribing to them.
    pub(crate) fn bars(self) -> Vec<(PageId, PageBar)> {
        let order = self.order.get();
        let bars = self.bars.get();

        order
            .into_iter()
            .filter_map(|id| bars.iter().find(|(page, _)| *page == id).copied())
            .collect()
    }

    /// The bar of the page under the page `id`, subscribing to the stack.
    pub(crate) fn bar_below(self, id: PageId) -> Option<PageBar> {
        let below = self.order.with(|order| {
            let at = order.iter().position(|page| *page == id)?;
            at.checked_sub(1).map(|below| order[below])
        })?;

        self.bars.with(|bars| {
            bars.iter()
                .find(|(page, _)| *page == below)
                .map(|(_, bar)| *bar)
        })
    }

    /// Whether the page `id` is not the bottom page, subscribing to the stack.
    pub(crate) fn has_page_below(self, id: PageId) -> bool {
        self.order
            .with(|order| order.first().is_some_and(|first| *first != id))
    }

    fn is_blocked(self, id: PageId) -> bool {
        self.bars.with_untracked(|bars| {
            bars.iter()
                .find(|(page, _)| *page == id)
                .is_some_and(|(_, bar)| bar.is_back_blocked_untracked())
        })
    }
}

/// The pages of a [`NavStack`](crate::nav_stack::NavStack), bottom first.
///
/// A path is generic over the route type `P` of the application. The stack builds one page view
/// for each pushed route and keeps it mounted until the route is popped, so a page below the top
/// page keeps its state and its scroll position.
///
/// `Copy`, so an event handler can hold one. Make it in the scope that holds the stack.
///
/// ```
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui::nav_stack::{NavPath, PopError};
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let path = NavPath::new("groups");
///     path.push("group");
///     path.push("entry");
///     assert_eq!(path.pages(), ["groups", "group", "entry"]);
///
///     assert_eq!(path.pop(), Ok(()));
///     assert_eq!(path.top(), "group");
///     assert_eq!(path.pop_to_root(), Ok(()));
///     assert_eq!(path.pop(), Err(PopError::AtRoot));
/// });
/// scope.unmount();
/// ```
pub struct NavPath<P: 'static> {
    /// The pages in the stack.
    entries: RwSignal<Vec<Entry<P>>, LocalStorage>,
    /// A popped page that still moves.
    leaving: RwSignal<Option<Entry<P>>, LocalStorage>,
    /// What is the same for every route type.
    core: NavCore,
}

impl<P: 'static> Clone for NavPath<P> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<P: 'static> Copy for NavPath<P> {}

impl<P> NavPath<P>
where
    P: Clone + 'static,
{
    /// A path that holds only `root`.
    #[must_use]
    pub fn new(root: P) -> Self {
        let core = NavCore::new();
        let id = core.allocate();
        core.order.set(vec![id]);

        Self {
            entries: RwSignal::new_local(vec![Entry { id, page: root }]),
            leaving: RwSignal::new_local(None),
            core,
        }
    }

    /// Puts `page` on top of the stack, and slides it in.
    pub fn push(self, page: P) {
        self.settle();

        let Some(from) = self
            .entries
            .with_untracked(|entries| entries.last().map(|e| e.id))
        else {
            return;
        };
        let to = self.core.allocate();
        self.entries
            .update(|entries| entries.push(Entry { id: to, page }));
        self.core.order.update(|order| order.push(to));

        self.core.motion.set(Motion::Push { from, to });
    }

    /// Takes the top page off the stack, and slides it out.
    ///
    /// # Errors
    ///
    /// [`PopError::AtRoot`] when only the root page is left, and [`PopError::Blocked`] while the top
    /// page blocks back.
    pub fn pop(self) -> Result<(), PopError> {
        self.check_pop()?;
        self.settle();

        self.pop_to(1)
    }

    /// Takes every page above the root page off the stack, and slides the top page out.
    ///
    /// # Errors
    ///
    /// [`PopError::AtRoot`] when only the root page is left, and [`PopError::Blocked`] while the top
    /// page blocks back.
    pub fn pop_to_root(self) -> Result<(), PopError> {
        self.check_pop()?;
        self.settle();

        let count = self.entries.with_untracked(Vec::len) - 1;
        self.pop_to(count)
    }

    /// The route of the top page, subscribing to the stack.
    #[must_use]
    pub fn top(self) -> P {
        self.entries.with(|entries| {
            entries
                .last()
                .map(|entry| entry.page.clone())
                .expect("a path always holds its root page")
        })
    }

    /// The routes in the stack, bottom first, subscribing to it.
    #[must_use]
    pub fn pages(self) -> Vec<P> {
        self.entries
            .with(|entries| entries.iter().map(|entry| entry.page.clone()).collect())
    }

    /// The routes in the stack, bottom first.
    #[must_use]
    pub fn pages_untracked(self) -> Vec<P> {
        self.entries
            .with_untracked(|entries| entries.iter().map(|entry| entry.page.clone()).collect())
    }

    /// Frees the path, for a path made outside a scope that ends with the stack. Any later use
    /// panics.
    pub fn dispose(self) {
        self.entries.dispose();
        self.leaving.dispose();
        self.core.dispose();
    }

    /// How many pages the stack holds, the root page included. Subscribes to the stack.
    #[must_use]
    pub fn count(self) -> usize {
        self.entries.with(Vec::len)
    }

    /// Whether only the root page is left. Subscribes to the stack.
    #[must_use]
    pub fn is_at_root(self) -> bool {
        self.count() == 1
    }

    /// Starts a swipe back: a finger drags the top page aside. Returns whether it started.
    ///
    /// Refused when [`NavPath::pop`] would refuse.
    pub(crate) fn begin_swipe(self) -> bool {
        if self.check_pop().is_err() {
            return false;
        }
        self.settle();

        let Some((to, from)) = self.entries.with_untracked(|entries| {
            let [.., below, top] = entries.as_slice() else {
                return None;
            };
            Some((below.id, top.id))
        }) else {
            return false;
        };
        self.core.progress.set(0.0);
        self.core.ease.set(None);
        self.core.motion.set(Motion::Swipe {
            from,
            to,
            end: None,
        });

        true
    }

    /// Lets go of a swipe back. A completed swipe pops the top page. Returns false when no swipe
    /// was in progress, for example after a push during the drag.
    ///
    /// `ease` is the curve of the motion that follows, when it carries the speed of the finger.
    pub(crate) fn end_swipe(self, end: SwipeEnd, ease: Option<String>) -> bool {
        let Motion::Swipe {
            from,
            to,
            end: None,
        } = self.core.motion.get_untracked()
        else {
            return false;
        };

        if end == SwipeEnd::Complete {
            let mut entries = self.entries.get_untracked();
            let popped = entries.pop();
            self.entries.set(entries);
            self.leaving.set(popped);
        }
        self.core.ease.set(ease);
        self.core.motion.set(Motion::Swipe {
            from,
            to,
            end: Some(end),
        });

        true
    }

    /// The pages to mount: the stack, then a popped page that still moves.
    pub(crate) fn mounted(self) -> Vec<Entry<P>> {
        let mut mounted = self.entries.get();
        if let Some(leaving) = self.leaving.get() {
            mounted.push(leaving);
        }

        mounted
    }

    pub(crate) fn core(self) -> NavCore {
        self.core
    }

    /// Ends the motion in progress, and unmounts a popped page.
    pub(crate) fn settle(self) {
        if self.core.motion.get_untracked() == Motion::Still {
            return;
        }

        self.core.motion.set(Motion::Still);
        if self.leaving.get_untracked().is_some() {
            self.leaving.set(None);
            let ids: Vec<PageId> = self
                .entries
                .with_untracked(|entries| entries.iter().map(|entry| entry.id).collect());
            self.core.order.set(ids);
        }
    }

    fn check_pop(self) -> Result<(), PopError> {
        let top = self.entries.with_untracked(|entries| {
            if entries.len() < 2 {
                return None;
            }
            entries.last().map(|entry| entry.id)
        });
        let Some(top) = top else {
            return Err(PopError::AtRoot);
        };

        if self.core.is_blocked(top) {
            return Err(PopError::Blocked);
        }

        Ok(())
    }

    /// Takes `count` pages off the stack. The top page slides out, and the others unmount at once.
    fn pop_to(self, count: usize) -> Result<(), PopError> {
        let mut entries = self.entries.get_untracked();
        let keep = entries.len().saturating_sub(count).max(1);
        if keep == entries.len() {
            return Err(PopError::AtRoot);
        }

        let from = entries.pop().expect("the stack holds more than one page");
        entries.truncate(keep);
        let to = entries.last().expect("the root page stays").id;

        let mut order: Vec<PageId> = entries.iter().map(|entry| entry.id).collect();
        order.push(from.id);

        self.entries.set(entries);
        self.leaving.set(Some(from.clone()));
        self.core.order.set(order);
        self.core.motion.set(Motion::Pop { from: from.id, to });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use zgui::reactive::Mounted;
    use zgui::reactive::install;

    use super::*;

    fn mounted(body: impl FnOnce()) {
        install().ok();
        let scope = Mounted::new();
        scope.with(body);
        scope.unmount();
    }

    fn ids(path: NavPath<&'static str>) -> Vec<&'static str> {
        path.mounted().into_iter().map(|entry| entry.page).collect()
    }

    #[test]
    fn a_popped_page_stays_mounted_until_the_motion_settles() {
        mounted(|| {
            let path = NavPath::new("root");
            path.push("group");
            path.settle();
            path.pop().unwrap();

            assert_eq!(path.pages(), ["root"]);
            assert_eq!(ids(path), ["root", "group"]);

            path.settle();
            assert_eq!(ids(path), ["root"]);
        });
    }

    #[test]
    fn places_follow_the_motion() {
        mounted(|| {
            let path = NavPath::new("root");
            let core = path.core();
            let root = path.mounted()[0].id;
            path.push("group");
            let group = path.mounted()[1].id;

            assert_eq!(core.place(group), PagePlace::Entering);
            assert_eq!(core.place(root), PagePlace::Departing);

            path.settle();
            assert_eq!(core.place(group), PagePlace::Current);
            assert_eq!(core.place(root), PagePlace::Covered);

            path.pop().unwrap();
            assert_eq!(core.place(group), PagePlace::Leaving);
            assert_eq!(core.place(root), PagePlace::Returning);
        });
    }

    #[test]
    fn pop_to_root_unmounts_the_middle_pages_at_once() {
        mounted(|| {
            let path = NavPath::new("root");
            path.push("a");
            path.push("b");
            path.push("c");
            path.pop_to_root().unwrap();

            assert_eq!(path.pages(), ["root"]);
            assert_eq!(ids(path), ["root", "c"]);
        });
    }

    #[test]
    fn a_push_during_a_pop_ends_the_pop_first() {
        mounted(|| {
            let path = NavPath::new("root");
            path.push("a");
            path.pop().unwrap();
            path.push("b");

            assert_eq!(ids(path), ["root", "b"]);
        });
    }

    #[test]
    fn a_completed_swipe_pops_the_top_page_once_it_has_slid_out() {
        mounted(|| {
            let path = NavPath::new("root");
            path.push("group");
            assert!(path.begin_swipe(), "a swipe ends the push first");
            let core = path.core();
            let [root, group] = [path.mounted()[0].id, path.mounted()[1].id];
            assert_eq!(core.place(group), PagePlace::Swiped);
            assert_eq!(core.place(root), PagePlace::Uncovered);

            path.end_swipe(SwipeEnd::Complete, None);
            assert_eq!(path.pages(), ["root"]);
            assert_eq!(ids(path), ["root", "group"]);
            assert_eq!(core.place(group), PagePlace::SwipedOut);

            path.settle();
            assert_eq!(ids(path), ["root"]);
            assert_eq!(core.place(root), PagePlace::Current);
        });
    }

    #[test]
    fn a_cancelled_swipe_keeps_the_stack() {
        mounted(|| {
            let path = NavPath::new("root");
            path.push("group");
            assert!(path.begin_swipe());
            path.end_swipe(SwipeEnd::Cancel, None);
            assert_eq!(path.pages(), ["root", "group"]);
            assert_eq!(
                path.core().place(path.mounted()[1].id),
                PagePlace::SwipedBack
            );

            path.settle();
            assert_eq!(path.core().place(path.mounted()[1].id), PagePlace::Current);
        });
    }

    #[test]
    fn the_root_page_cannot_be_swiped_away() {
        mounted(|| {
            let path = NavPath::new("root");
            assert!(!path.begin_swipe());
            assert_eq!(path.core().motion.get_untracked(), Motion::Still);
        });
    }

    #[test]
    fn the_root_page_cannot_be_popped() {
        mounted(|| {
            let path = NavPath::new("root");
            assert_eq!(path.pop(), Err(PopError::AtRoot));
            assert_eq!(path.pop_to_root(), Err(PopError::AtRoot));
            assert!(path.is_at_root());
        });
    }
}
