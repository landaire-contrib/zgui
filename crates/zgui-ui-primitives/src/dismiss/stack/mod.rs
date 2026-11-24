//! Which open surface a press or an escape belongs to.

mod entry;

use std::cell::RefCell;
use std::rc::Rc;

use zgui::prelude::*;
use zgui::reactive::{ArcTrigger, Owner, RenderEffect};
use zgui::runtime::windows::ReaderModal;
use zgui::vocab::PointerId;

use crate::dismiss::stack::entry::Entry;

/// One registered layer's name.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[repr(transparent)]
pub struct LayerId(u64);

impl LayerId {
    /// Wraps a stack's own number.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// The number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The open dismissable surfaces of one window, innermost last.
///
/// A press past an open popover inside an open dialog has to dismiss **the popover**, and only the
/// popover. Getting that wrong in either direction is a bug users notice immediately: dismissing
/// both closes the dialog they were working in, and dismissing the dialog leaves an orphaned menu
/// floating over nothing.
///
/// So exactly one layer answers, and which one is decided here — by the overlay band first and by
/// the order they opened second. The band comes first because a toast raised before a dialog is
/// still above it, and mount order alone would say otherwise.
///
/// # A layer that is on its way out
///
/// Closing is not instant: a surface that has been dismissed stays mounted until its exit
/// animation has finished, which is a tenth of a second in which it is still registered here. It
/// keeps answering **presses**, because it is still on the screen and a press has a place — one
/// that lands on a surface which is still visible must not reach the surface behind it. It stops
/// answering **Escape**, because Escape has no place: it belongs to whatever is open, and a
/// surface that has already been told to close is not. Without that distinction a dialog opened
/// from a dialog cannot be closed with two presses of Escape, because the inner one eats the
/// second while it fades. A press past it is not consumed: the layer is already closing, and the
/// press goes on to what it lands on.
///
/// # A layer that is no longer there at all
///
/// A layer whose element has left the document answers nothing. Its entry should have been taken
/// off by the component's own cleanup, and if that never ran — a surface that failed to unmount —
/// what is left is an entry that would otherwise swallow every press and every Escape in the
/// window for the rest of the session.
///
/// ```
/// use zgui::prelude::*;
/// use zgui::reactive::{Mounted, install};
/// use zgui_ui_primitives::dismiss::LayerStack;
///
/// install().ok();
/// let scope = Mounted::new();
/// scope.with(|| {
///     let stack = LayerStack::current();
///     let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
///     let popover = stack.push(OverlayLayer::Popover, NodeRef::new());
///
///     // The popover went up second but sits on a lower band, and the band decides.
///     assert_eq!(stack.topmost(), Some(dialog));
///     assert!(!stack.is_topmost(popover));
///
///     // Once the dialog has been asked to close, Escape belongs to the popover instead — while
///     // a press still belongs to the dialog, which is still on the screen.
///     stack.set_leaving(dialog, true);
///     assert_eq!(stack.answering_escape(), Some(popover));
///     assert_eq!(stack.topmost(), Some(dialog));
/// });
/// scope.unmount();
/// ```
///
/// # A press past the topmost layer
///
/// A press past a topmost layer that covers closes it and activates nothing, as a tap outside a
/// menu does on macOS and iOS. The layer stops the press as it goes down and records its pointer
/// here, and the stack consumes the release of that pointer, so no click follows. The record is on
/// the stack because a layer with no exit animation is gone before the release arrives. A cancelled
/// pointer and the next press of the same pointer clear the record.
///
/// # What a reader reaches
///
/// A layer that covers what is under it is modal to a screen reader. While one is open, the reader
/// reaches the innermost such layer, the layers open above it and the elements kept readable with
/// [`LayerStack::keep_readable`], and nothing else in the window; see
/// [`LayerStack::reader_modal`].
#[derive(Clone)]
pub struct LayerStack {
    /// The open layers, in registration order.
    entries: Rc<RefCell<Vec<Entry>>>,
    /// The next number to hand out, for both names and tie-breaks.
    next: Rc<std::cell::Cell<u64>>,
    /// Notified when a layer goes up, comes down, starts to leave or changes what it covers.
    changed: ArcTrigger,

    /// The pointers whose press past a layer closed it, until that press ends.
    swallowed: Rc<RefCell<Vec<PointerId>>>,
    /// The window root's listeners that consume the release of those presses.
    releases: Rc<RefCell<Vec<ListenerGuard>>>,
    /// The elements a reader reaches while a layer confines it, by registration number.
    readable: Rc<RefCell<Vec<(u64, NodeRef)>>>,
}

impl LayerStack {
    /// An empty stack.
    pub fn new() -> Self {
        Self {
            entries: Rc::new(RefCell::new(Vec::new())),
            next: Rc::new(std::cell::Cell::new(1)),
            changed: ArcTrigger::new(),
            swallowed: Rc::new(RefCell::new(Vec::new())),
            releases: Rc::new(RefCell::new(Vec::new())),
            readable: Rc::new(RefCell::new(Vec::new())),
        }
    }

    /// This window's stack, created and published on first use.
    ///
    /// Behind a context rather than a global, for the reason everything per-window is: two windows
    /// in one process each have their own open surfaces, and a press in one must not dismiss
    /// anything in the other.
    ///
    /// The first one created is published in the window's **root** scope rather than in whichever
    /// surface happened to ask first. That is what makes a popover and a dialog written side by
    /// side share one stack: published where the asker stands, the second one would find nothing,
    /// mint a stack of its own, and believe itself topmost — and one Escape would close both.
    ///
    /// The stack keeps its window told whether an Escape closes a layer, and which layer a reader
    /// is confined to; see [`WindowHandle::set_escape_dismisses`] and
    /// [`WindowHandle::set_modal_root`].
    pub fn current() -> Self {
        match use_local_context::<Self>() {
            Some(stack) => stack,
            None => {
                let stack = Self::new();
                let window = try_use_window();
                match root_scope() {
                    Some(root) => root.with(|| {
                        provide_local_context(stack.clone());
                        stack.tell(window);
                    }),
                    None => {
                        provide_local_context(stack.clone());
                        stack.tell(window);
                    }
                }
                stack
            }
        }
    }

    /// Keeps `window` told whether an Escape closes a layer and which layer a reader is confined
    /// to, for as long as the calling scope.
    fn tell(&self, window: Option<WindowHandle>) {
        let Some(window) = window else {
            return;
        };

        let stack = self.clone();
        let escapes_window = window.clone();
        let told = RenderEffect::new(move |was: Option<bool>| {
            let dismisses = stack.escape_dismisses();
            if was != Some(dismisses) {
                escapes_window.set_escape_dismisses(dismisses);
            }
            dismisses
        });

        let stack = self.clone();
        let confined = RenderEffect::new(move |was: Option<Option<ReaderModal>>| {
            let modal = stack.reader_modal();
            if was.as_ref() != Some(&modal) {
                window.set_reader_modal(modal.clone());
            }
            modal
        });

        on_cleanup_local(move || {
            drop(told);
            drop(confined);
        });
    }

    /// Registers a layer on `band` over `surface`, and reports what it is called.
    ///
    /// The handle is the layer's own element. It is what the stack asks whether the layer is still
    /// on the screen, so a layer registered over a handle that is never bound is one nothing can
    /// ever retire.
    pub fn push(&self, band: OverlayLayer, surface: NodeRef) -> LayerId {
        let ordinal = self.next.get();
        self.next.set(ordinal + 1);
        let id = LayerId::new(ordinal);
        self.entries
            .borrow_mut()
            .push(Entry::new(id, band, ordinal, surface));
        self.changed.notify();
        id
    }

    /// Takes a layer out. Removing one that is not there does nothing.
    pub fn pop(&self, id: LayerId) {
        let removed = {
            let mut entries = self.entries.borrow_mut();
            let before = entries.len();
            entries.retain(|entry| entry.id() != id);
            entries.len() != before
        };
        if removed {
            self.changed.notify();
        }
    }

    /// Records whether a layer has been asked to close and is playing its exit.
    ///
    /// Setting one that is not there does nothing, which is what makes this safe to call from an
    /// effect that outlives the entry it is about.
    pub fn set_leaving(&self, id: LayerId, leaving: bool) {
        self.change(id, |entry| {
            let changed = entry.leaving() != leaving;
            entry.set_leaving(leaving);
            changed
        });
    }

    /// Records whether what is under a layer counts as covered while it is open, as for a menu or
    /// a dialog that a press past it only closes. Setting one that is not there does nothing.
    pub fn set_covers(&self, id: LayerId, covers: bool) {
        self.change(id, |entry| {
            let changed = entry.covers() != covers;
            entry.set_covers(covers);
            changed
        });
    }

    /// Records whether an Escape that a layer answers closes it. Setting one that is not there
    /// does nothing.
    pub fn set_escapes(&self, id: LayerId, escapes: bool) {
        self.change(id, |entry| {
            let changed = entry.escapes() != escapes;
            entry.set_escapes(escapes);
            changed
        });
    }

    /// Applies `edit` to the entry `id` names, and notifies when `edit` reports a change.
    fn change(&self, id: LayerId, edit: impl FnOnce(&mut Entry) -> bool) {
        let changed = self
            .entries
            .borrow_mut()
            .iter_mut()
            .find(|entry| entry.id() == id)
            .is_some_and(edit);
        if changed {
            self.changed.notify();
        }
    }

    /// Records that the press of `pointer` closed a layer, so that its release reaches nothing.
    pub(crate) fn swallow_press(&self, pointer: PointerId) {
        let mut swallowed = self.swallowed.borrow_mut();
        if !swallowed.contains(&pointer) {
            swallowed.push(pointer);
        }
    }

    /// Consumes on `window`, the window's root, the release of each press a layer closed.
    ///
    /// The listeners live as long as the stack, which is as long as the window. Calls after the
    /// first do nothing.
    pub(crate) fn consume_releases(&self, window: &NodeRef) {
        if !self.releases.borrow().is_empty() {
            return;
        }

        let mut held = Vec::new();
        // Registered before any layer's own press listener, so a press clears the record of an
        // earlier press whose release never reached the window, as a mouse let go outside it.
        let swallowed = Rc::clone(&self.swallowed);
        held.extend(window.listen(
            events::POINTER_DOWN,
            ListenerOptions::CAPTURE,
            move |ev: &mut EventCx<'_, events::PointerDown>| {
                forget(&swallowed, ev.id);
            },
        ));
        let swallowed = Rc::clone(&self.swallowed);
        held.extend(window.listen(
            events::POINTER_UP,
            ListenerOptions::CAPTURE,
            move |ev: &mut EventCx<'_, events::PointerUp>| {
                if !forget(&swallowed, ev.id) {
                    return;
                }
                ev.stop_immediate_propagation();
                ev.prevent_default();
            },
        ));
        let swallowed = Rc::clone(&self.swallowed);
        held.extend(window.listen(
            events::POINTER_CANCEL,
            ListenerOptions::CAPTURE,
            move |ev: &mut EventCx<'_, events::PointerCancel>| {
                forget(&swallowed, ev.id);
            },
        ));
        *self.releases.borrow_mut() = held;
    }

    /// Whether an open layer that covers what is under it is on the screen, subscribing the
    /// caller.
    ///
    /// A screen reader reaches nothing outside the innermost such layer, other than what is
    /// beside it; see [`LayerStack::reader_modal`].
    pub fn covering(&self) -> bool {
        self.changed.track();
        self.entries
            .borrow_mut()
            .iter_mut()
            .any(|entry| entry.covers() && !entry.leaving() && entry.live())
    }

    /// Keeps `node` readable while a layer confines a reader, for as long as the calling scope.
    ///
    /// For a region that announces notices, such as the toasts', which a reader has to hear over
    /// an open dialog.
    pub fn keep_readable(&self, node: NodeRef) {
        let number = self.next.get();
        self.next.set(number + 1);
        self.readable.borrow_mut().push((number, node));
        self.changed.notify();

        let stack = self.clone();
        on_cleanup_local(move || {
            stack
                .readable
                .borrow_mut()
                .retain(|(held, _)| *held != number);
            stack.changed.notify();
        });
    }

    /// What a reader is confined to, subscribing the caller.
    ///
    /// The surface is the innermost open layer that covers what is under it. Beside it are the
    /// live layers ranked above it and the elements kept readable with
    /// [`LayerStack::keep_readable`]. The window confines a screen reader to these, as UIKit does
    /// for a view with `accessibilityViewIsModal`.
    ///
    /// A layer that covers is modal to a reader: a menu, a select, a dialog or a sheet. A layer
    /// that does not cover, such as a tooltip, leaves the rest of the window readable. A layer
    /// that plays its exit gives the window back.
    pub fn reader_modal(&self) -> Option<ReaderModal> {
        self.changed.track();
        let id = self.answering(|entry| entry.covers() && !entry.leaving())?;

        let mut entries = self.entries.borrow_mut();
        let modal = entries.iter().find(|entry| entry.id() == id)?;
        let rank = modal.rank();
        let surface = modal.surface().get()?;

        let mut beside = Vec::new();
        for entry in entries.iter_mut() {
            if entry.rank() <= rank || !entry.live() {
                continue;
            }
            beside.extend(entry.surface().get());
        }
        drop(entries);
        for (_, node) in self.readable.borrow().iter() {
            beside.extend(node.get());
        }

        Some(ReaderModal { surface, beside })
    }

    /// Whether an Escape closes an open layer now, subscribing the caller.
    pub fn escape_dismisses(&self) -> bool {
        self.changed.track();
        let Some(id) = self.answering_escape() else {
            return false;
        };
        self.entries
            .borrow()
            .iter()
            .any(|entry| entry.id() == id && entry.escapes())
    }

    /// The layer a press belongs to, when anything is on the screen.
    pub fn topmost(&self) -> Option<LayerId> {
        self.answering(|_| true)
    }

    /// Whether `id` is the layer a press belongs to.
    pub fn is_topmost(&self, id: LayerId) -> bool {
        self.topmost() == Some(id)
    }

    /// Whether `id` has been asked to close and is playing its exit.
    pub fn is_leaving(&self, id: LayerId) -> bool {
        self.entries
            .borrow()
            .iter()
            .any(|entry| entry.id() == id && entry.leaving())
    }

    /// The layer an Escape belongs to, which is the innermost one that is still open.
    pub fn answering_escape(&self) -> Option<LayerId> {
        self.answering(|entry| !entry.leaving())
    }

    /// Whether `id` is the layer an Escape belongs to.
    pub fn answers_escape(&self, id: LayerId) -> bool {
        self.answering_escape() == Some(id)
    }

    /// The innermost live layer `want` accepts.
    ///
    /// Taken through a mutable borrow because asking is also what records that a layer has been
    /// seen on the screen: a handle that is bound now and unbound later is the only evidence there
    /// is that a surface has gone.
    fn answering(&self, want: impl Fn(&Entry) -> bool) -> Option<LayerId> {
        let mut entries = self.entries.borrow_mut();
        let mut best: Option<((OverlayLayer, u64), LayerId)> = None;
        for entry in entries.iter_mut() {
            if !entry.live() || !want(entry) {
                continue;
            }
            if best.is_none_or(|(rank, _)| entry.rank() > rank) {
                best = Some((entry.rank(), entry.id()));
            }
        }
        best.map(|(_, id)| id)
    }

    /// How many layers are open.
    pub fn len(&self) -> usize {
        self.entries.borrow().len()
    }

    /// Whether none are.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Every entry as one string, innermost last, for a trace line.
    pub(crate) fn describe(&self) -> String {
        let mut entries = self.entries.borrow_mut();
        let described: Vec<String> = entries.iter_mut().map(Entry::describe).collect();
        format!("[{}]", described.join(","))
    }
}

impl Default for LayerStack {
    fn default() -> Self {
        Self::new()
    }
}

/// The outermost scope above the calling one, which for anything inside a window is the window's.
///
/// A window's own scope has no parent — it is what the runtime creates for that window and
/// nothing else — so walking up from wherever a surface was written reaches it and stops there.
fn root_scope() -> Option<Owner> {
    let mut scope = Owner::current()?;
    while let Some(parent) = scope.parent() {
        scope = parent;
    }
    Some(scope)
}

/// Removes `pointer` from `swallowed`, and answers whether it was there.
fn forget(swallowed: &RefCell<Vec<PointerId>>, pointer: PointerId) -> bool {
    let mut swallowed = swallowed.borrow_mut();
    let before = swallowed.len();
    swallowed.retain(|held| *held != pointer);

    swallowed.len() != before
}

#[cfg(test)]
mod tests {
    use zgui::prelude::{NodeRef, OverlayLayer};

    use super::LayerStack;

    #[test]
    fn within_one_band_the_last_one_open_is_the_one_that_answers() {
        let stack = LayerStack::new();
        let first = stack.push(OverlayLayer::Popover, NodeRef::new());
        let second = stack.push(OverlayLayer::Popover, NodeRef::new());
        assert_eq!(stack.topmost(), Some(second));

        // Closing the inner one hands the answer back to the outer one rather than to nothing.
        stack.pop(second);
        assert_eq!(stack.topmost(), Some(first));
    }

    #[test]
    fn the_band_beats_the_order_they_opened_in() {
        // A toast raised before a dialog paints above it, so a press belongs to the toast. Mount
        // order alone would say the dialog, which is the bug this ordering exists to prevent.
        let stack = LayerStack::new();
        let toast = stack.push(OverlayLayer::Toast, NodeRef::new());
        stack.push(OverlayLayer::Modal, NodeRef::new());
        assert_eq!(stack.topmost(), Some(toast));
    }

    #[test]
    fn a_popover_inside_a_dialog_is_the_one_that_answers() {
        // The case the whole stack exists for, written the way a user meets it: a dialog is open,
        // a menu inside it is open, and one press past the menu closes the menu.
        let stack = LayerStack::new();
        let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
        let menu = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert_eq!(stack.topmost(), Some(menu));
        stack.pop(menu);
        assert_eq!(
            stack.topmost(),
            Some(dialog),
            "and the dialog is still open"
        );
    }

    #[test]
    fn an_empty_stack_answers_nothing_rather_than_guessing() {
        let stack = LayerStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.topmost(), None);
        let id = stack.push(OverlayLayer::Popover, NodeRef::new());
        stack.pop(id);
        assert_eq!(stack.topmost(), None);
        assert!(!stack.is_topmost(id));
    }

    #[test]
    fn escape_goes_to_the_innermost_surface_that_has_not_been_closed_yet() {
        // The nested case, at the moment it goes wrong: the inner surface has been dismissed and
        // is playing its exit, and the second Escape has to reach the dialog behind it rather than
        // be eaten again by the one that is already leaving.
        let stack = LayerStack::new();
        let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
        let inner = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert!(stack.answers_escape(inner));

        stack.set_leaving(inner, true);
        assert!(
            stack.answers_escape(dialog),
            "the Escape belongs to the dialog the moment the inner surface has been told to close"
        );
        assert!(
            !stack.answers_escape(inner),
            "and the surface on its way out claims no more of them"
        );
        assert!(
            stack.is_topmost(inner),
            "while a press still belongs to it, because it is still on the screen"
        );
    }

    #[test]
    fn a_layer_that_was_never_built_still_answers() {
        // Registered in a component's body, one flush before the view binds its element. A stack
        // that took an unbound handle for a departed surface would drop the first key or press
        // after every open.
        let stack = LayerStack::new();
        let id = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert_eq!(stack.topmost(), Some(id));
        assert_eq!(stack.answering_escape(), Some(id));
    }

    #[test]
    fn only_an_open_layer_that_covers_counts_as_covering() {
        let stack = LayerStack::new();
        let tooltip = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert!(
            !stack.covering(),
            "a layer covers nothing unless it says so"
        );

        let menu = stack.push(OverlayLayer::Modal, NodeRef::new());
        stack.set_covers(menu, true);
        assert!(stack.covering());

        stack.set_leaving(menu, true);
        assert!(
            !stack.covering(),
            "a menu playing its exit leaves what is under it to the next press"
        );
        stack.pop(menu);
        stack.pop(tooltip);
        assert!(!stack.covering());
    }

    #[test]
    fn escape_dismisses_only_when_the_layer_it_belongs_to_takes_it() {
        let stack = LayerStack::new();
        assert!(!stack.escape_dismisses(), "nothing is open");

        let dialog = stack.push(OverlayLayer::Modal, NodeRef::new());
        assert!(stack.escape_dismisses());

        let alert = stack.push(OverlayLayer::Modal, NodeRef::new());
        stack.set_escapes(alert, false);
        assert!(
            !stack.escape_dismisses(),
            "the alert refuses Escape, and the dialog behind it does not get the key"
        );

        stack.pop(alert);
        assert!(stack.escape_dismisses());

        stack.set_leaving(dialog, true);
        assert!(
            !stack.escape_dismisses(),
            "a dialog playing its exit is already closing"
        );
    }
}
