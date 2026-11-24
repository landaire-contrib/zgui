//! What a sequence of raw touches means.
//!
//! A finger produces presses, moves and releases and nothing else. Everything a touch interface is
//! made of — a tap, a long press, a drag, a flick that keeps going, a pinch — is a *reading* of that
//! stream, and it has to be produced somewhere because no platform in this backend's reach produces
//! it. The reading is here rather than in a component for the reason every other interpretation of
//! input is here: two components that each recognised a tap would disagree about what one is, and
//! the disagreement would show up as a control that needs to be pressed twice.
//!
//! # The rules the readings obey
//!
//! * **A tap is a press and a release that did not travel.** The slop is what makes a tap on a
//!   moving finger still a tap; without it a touch screen has no tap at all, because no finger is
//!   ever perfectly still.
//! * **A press that travels past the slop becomes a pan, and stops being a candidate for anything
//!   else.** That is what stops a list from activating the row a scroll started on.
//! * **A long press is a press that has neither travelled nor ended.** It is reported by the clock
//!   rather than by an event, so whoever owns the clock asks for it.
//! * **A pan reports the speed it ended with**, because a flick is a pan whose end had speed in it
//!   and there is nothing else in the stream that can say how fast a finger was moving.

pub mod longpress;
pub mod pan;
pub mod pinch;
pub mod tap;

use core::time::Duration;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;
use zgui_geom::{Css, CssPx, Point, Size};
use zgui_vocab::{
    GesturePhase, PointerAction, PointerEvent, PointerId, PointerKind, PointerSample, Timestamp,
    Velocity,
};

pub use crate::gesture::longpress::LONG_PRESS;
pub use crate::gesture::pan::Track;
pub use crate::gesture::tap::SLOP;

/// A reading of the touch stream.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum Gesture {
    /// A finger touched and lifted without travelling.
    Tap {
        /// Which pointer.
        pointer: PointerId,
        /// Where it was.
        at: Point<CssPx, Css>,
    },
    /// A finger has been held still for long enough to mean something else.
    LongPress {
        /// Which pointer.
        pointer: PointerId,
        /// Where it is.
        at: Point<CssPx, Css>,
    },
    /// A finger has travelled past the slop and is now dragging.
    PanStart {
        /// Which pointer.
        pointer: PointerId,
        /// Where it started.
        from: Point<CssPx, Css>,
        /// Where it is now.
        at: Point<CssPx, Css>,
    },
    /// A dragging finger moved.
    PanMove {
        /// Which pointer.
        pointer: PointerId,
        /// How far it moved since the last reading.
        by: Size<CssPx, Css>,
        /// Where it is now.
        at: Point<CssPx, Css>,
        /// How fast it is travelling, in CSS pixels per second.
        velocity: Velocity,
    },
    /// A dragging finger lifted, with the speed it lifted at.
    PanEnd {
        /// Which pointer.
        pointer: PointerId,
        /// Where it lifted.
        at: Point<CssPx, Css>,
        /// How fast it was travelling, in CSS pixels per second.
        velocity: Velocity,
    },
    /// Two fingers moved towards or away from each other.
    ///
    /// A pinch starts when a second contact goes down, and ends when fewer than two remain. It is
    /// read between the two contacts that went down first.
    Pinch {
        /// Where in the gesture this reading is.
        phase: GesturePhase,
        /// The distance between the two contacts divided by their distance at the reading before.
        step: f32,
        /// The point midway between them.
        at: Point<CssPx, Css>,
    },
}

/// Reads gestures out of the raw pointer stream.
///
/// Only touch contacts are read. A mouse produces the same presses and moves, and reading a pan out
/// of a mouse drag would mean every text selection and every window-furniture drag also arrived as
/// a gesture — so the recogniser answers nothing for a mouse and a component that wants mouse drags
/// uses [`drag`](crate::drag), which is a different thing with different rules.
///
/// ```
/// use zgui_geom::{CssPx, Point};
/// use zgui_input::gesture::{Gesture, Gestures};
/// use core::time::Duration;
/// use zgui_vocab::{PointerAction, PointerEvent, PointerId, PointerKind, Timestamp};
///
/// let stamp = |millis: u64| Timestamp::from_origin(Duration::from_millis(millis));
///
/// let mut gestures = Gestures::default();
/// let finger = |x: f32, y: f32| PointerEvent {
///     id: PointerId::new(1),
///     kind: PointerKind::Touch,
///     primary: true,
///     position: Point::new(CssPx(x), CssPx(y)),
///     button: None,
///     pressure: None,
/// };
///
/// gestures.pointer(PointerAction::Pressed, &finger(10.0, 10.0), stamp(0));
/// let read = gestures.pointer(
///     PointerAction::Released,
///     &finger(11.0, 10.0),
///     stamp(80),
/// );
/// assert!(matches!(read.as_slice(), [Gesture::Tap { .. }]));
/// ```
#[derive(Clone, Debug, Default)]
pub struct Gestures {
    /// The contacts that are down, by pointer.
    contacts: FxHashMap<PointerId, Track>,
    /// The contacts that are down, in the order they went down.
    pressed: SmallVec<[PointerId; 4]>,
    /// The pinch of the first two contacts, while two are down.
    pinch: Option<Pinch>,
}

/// A pinch in progress.
#[derive(Clone, Copy, Debug)]
struct Pinch {
    /// The distance between the two contacts at the last reading.
    spread: f32,
    /// The point midway between them at the last reading.
    at: Point<CssPx, Css>,
    /// Whether an element took the pinch, so that no contact pans or taps until it ends.
    held: bool,
}

impl Gestures {
    /// A recogniser with nothing in progress.
    pub fn new() -> Self {
        Self::default()
    }

    /// How many contacts are down.
    pub fn contacts(&self) -> usize {
        self.contacts.len()
    }

    /// Whether any contact could still become something the clock has to report.
    ///
    /// This is what tells the loop whether to arm a deadline for a long press. With nothing held it
    /// is false, which is the state of every idle application.
    pub fn awaits_deadline(&self) -> bool {
        self.contacts
            .values()
            .any(|track| track.may_become_long_press())
    }

    /// Records the positions a contact passed through before its next move, oldest first.
    ///
    /// A queue that folds several moves of one contact into one keeps the folded positions as
    /// samples. They count towards the speed of the contact and produce no reading.
    pub fn passed(&mut self, pointer: PointerId, samples: &[PointerSample]) {
        let Some(track) = self.contacts.get_mut(&pointer) else {
            return;
        };
        for sample in samples {
            track.passed(sample.position, sample.timestamp);
        }
    }

    /// Reads one pointer event, returning whatever it completed.
    pub fn pointer(
        &mut self,
        action: PointerAction,
        event: &PointerEvent,
        at: Timestamp,
    ) -> SmallVec<[Gesture; 2]> {
        let mut read = SmallVec::new();
        if event.kind != PointerKind::Touch {
            return read;
        }
        match action {
            PointerAction::Pressed => {
                let mut track = Track::started(event.position, at);
                if self.pinch.is_some_and(|pinch| pinch.held) {
                    track.hold();
                }
                self.contacts.insert(event.id, track);
                self.pressed.retain(|id| *id != event.id);
                self.pressed.push(event.id);
                match self.pinch {
                    Some(_) => self.rebase_pinch(),
                    None => read.extend(self.start_pinch()),
                }
            }
            PointerAction::Moved => {
                let Some(track) = self.contacts.get_mut(&event.id) else {
                    return read;
                };
                let moved = track.moved(event.position, at);
                if moved.began_panning {
                    read.push(Gesture::PanStart {
                        pointer: event.id,
                        from: track.origin(),
                        at: event.position,
                    });
                }
                if track.is_panning() {
                    read.push(Gesture::PanMove {
                        pointer: event.id,
                        by: moved.by,
                        at: event.position,
                        velocity: moved.velocity,
                    });
                }
                read.extend(self.move_pinch());
            }
            PointerAction::Released => {
                let Some(track) = self.contacts.remove(&event.id) else {
                    return read;
                };
                self.pressed.retain(|id| *id != event.id);
                if track.is_panning() {
                    read.push(Gesture::PanEnd {
                        pointer: event.id,
                        at: event.position,
                        velocity: track.velocity(at),
                    });
                } else if !track.is_spent() {
                    read.push(Gesture::Tap {
                        pointer: event.id,
                        at: event.position,
                    });
                }
                read.extend(self.end_pinch(GesturePhase::Ended));
            }
            PointerAction::Cancelled => {
                // A cancelled contact produces no reading at all, which is the whole point of the
                // action existing: something else took the interaction over, and reporting a tap
                // for it would activate the control the gesture was taken away from.
                self.contacts.remove(&event.id);
                self.pressed.retain(|id| *id != event.id);
                read.extend(self.end_pinch(GesturePhase::Cancelled));
            }
            // Nothing else in the stream contributes to a reading: an enter and a leave are about
            // where a pointer is, and a kind this build has never heard of is not guessed at.
            _ => {}
        }
        read
    }

    /// Reports every contact that has now been held long enough to be a long press.
    ///
    /// Driven by whoever owns the clock, because a long press is the one reading that is produced
    /// by time passing rather than by anything arriving.
    pub fn elapsed(&mut self, now: Timestamp) -> SmallVec<[Gesture; 2]> {
        let mut read = SmallVec::new();
        for (pointer, track) in &mut self.contacts {
            if track.becomes_long_press(now) {
                read.push(Gesture::LongPress {
                    pointer: *pointer,
                    at: track.at(),
                });
            }
        }
        read
    }

    /// Records that an element took the long press of `pointer`, such as a context menu that
    /// opened. The contact then does not pan, so that it can move onto the menu.
    pub fn claim(&mut self, pointer: PointerId) {
        if let Some(track) = self.contacts.get_mut(&pointer) {
            track.claim();
        }
    }

    /// When the earliest pending long press comes due.
    pub fn next_deadline(&self, now: Timestamp) -> Option<Duration> {
        self.contacts
            .values()
            .filter_map(|track| track.long_press_in(now))
            .min()
    }

    /// Records that an element took the pinch in progress. No contact then pans or taps until the
    /// pinch ends, including a contact that goes down during it.
    pub fn hold_pinch(&mut self) {
        let Some(pinch) = self.pinch.as_mut() else {
            return;
        };
        pinch.held = true;
        for track in self.contacts.values_mut() {
            track.hold();
        }
    }

    /// The distance between the two contacts that went down first and the point midway between
    /// them, if two are down.
    fn measure(&self) -> Option<(f32, Point<CssPx, Css>)> {
        let mut down = self.pressed.iter().filter_map(|id| self.contacts.get(id));
        let first = down.next()?.at();
        let second = down.next()?.at();
        Some((
            pinch::distance(first, second),
            pinch::midpoint(first, second),
        ))
    }

    /// Starts a pinch when a second contact went down far enough from the first.
    fn start_pinch(&mut self) -> Option<Gesture> {
        if self.contacts.len() != 2 {
            return None;
        }
        let (spread, at) = self.measure()?;
        pinch::scale(spread, spread)?;
        self.pinch = Some(Pinch {
            spread,
            at,
            held: false,
        });

        Some(Gesture::Pinch {
            phase: GesturePhase::Started,
            step: 1.0,
            at,
        })
    }

    /// Reads the pinch in progress after a contact moved.
    fn move_pinch(&mut self) -> Option<Gesture> {
        let (spread, at) = self.measure()?;
        let pinch = self.pinch.as_mut()?;
        let step = pinch::scale(pinch.spread, spread)?;
        pinch.spread = spread;
        pinch.at = at;

        Some(Gesture::Pinch {
            phase: GesturePhase::Moved,
            step,
            at,
        })
    }

    /// Measures the pinch again from the contacts now down, after one went down or lifted while
    /// two others stay, so that the change of pair is not read as a step.
    fn rebase_pinch(&mut self) {
        let measured = self.measure();
        let Some(pinch) = self.pinch.as_mut() else {
            return;
        };
        if let Some((spread, at)) = measured {
            pinch.spread = spread;
            pinch.at = at;
        }
    }

    /// Ends the pinch in progress as `phase` when fewer than two contacts are left.
    fn end_pinch(&mut self, phase: GesturePhase) -> Option<Gesture> {
        if self.contacts.len() >= 2 {
            self.rebase_pinch();
            return None;
        }
        let pinch = self.pinch.take()?;

        Some(Gesture::Pinch {
            phase,
            step: 1.0,
            at: pinch.at,
        })
    }
}

#[cfg(test)]
mod tests {
    use zgui_geom::{Css, CssPx, Point};
    use zgui_vocab::{
        GesturePhase, PointerAction, PointerEvent, PointerId, PointerKind, PointerSample, Timestamp,
    };

    use super::{Gesture, Gestures};

    /// The instant `millis` after the application started.
    fn stamp(millis: u64) -> Timestamp {
        Timestamp::from_origin(core::time::Duration::from_millis(millis))
    }

    fn finger(id: u64, x: f32, y: f32) -> PointerEvent {
        PointerEvent {
            id: PointerId::new(id),
            kind: PointerKind::Touch,
            primary: id == 1,
            position: Point::<CssPx, Css>::new(CssPx(x), CssPx(y)),
            button: None,
            pressure: None,
        }
    }

    fn mouse(x: f32, y: f32) -> PointerEvent {
        PointerEvent::mouse(Point::new(CssPx(x), CssPx(y)))
    }

    #[test]
    fn a_press_and_a_release_that_travelled_is_a_pan_and_not_a_tap() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let moved = gestures.pointer(PointerAction::Moved, &finger(1, 0.0, 40.0), stamp(20));
        assert!(matches!(moved.first(), Some(Gesture::PanStart { .. })));

        let ended = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 40.0), stamp(30));
        assert!(
            matches!(ended.as_slice(), [Gesture::PanEnd { .. }]),
            "a scroll that ends over a row must not also activate it: {ended:?}"
        );
    }

    #[test]
    fn the_positions_a_folded_move_stands_for_count_towards_the_speed_of_the_lift() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        // Three moves of 20 pixels each 16 ms, folded into the fourth.
        let folded: Vec<PointerSample> = (1..=3_u16)
            .map(|step| PointerSample {
                position: Point::new(CssPx(0.0), CssPx(f32::from(step) * 20.0)),
                pressure: None,
                timestamp: stamp(u64::from(step) * 16),
            })
            .collect();
        gestures.passed(PointerId::new(1), &folded);
        gestures.pointer(PointerAction::Moved, &finger(1, 0.0, 80.0), stamp(64));

        let ended = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 80.0), stamp(64));
        let [Gesture::PanEnd { velocity, .. }] = ended.as_slice() else {
            panic!("the lift ended no pan: {ended:?}");
        };
        assert!(
            (velocity.y - 1250.0).abs() < 0.01,
            "a finger at 1250 px/s lifted at {} px/s",
            velocity.y
        );
    }

    #[test]
    fn a_mouse_produces_no_gesture_at_all() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &mouse(0.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Released, &mouse(0.0, 0.0), stamp(10));
        assert!(read.is_empty());
        assert_eq!(gestures.contacts(), 0);
    }

    #[test]
    fn a_cancelled_contact_produces_nothing() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Cancelled, &finger(1, 0.0, 0.0), stamp(10));
        assert!(read.is_empty(), "{read:?}");
        assert_eq!(gestures.contacts(), 0);
        assert!(!gestures.awaits_deadline());
    }

    /// The pinch readings in `read`.
    fn pinches(read: &[Gesture]) -> Vec<(GesturePhase, f32, Point<CssPx, Css>)> {
        read.iter()
            .filter_map(|gesture| match gesture {
                Gesture::Pinch { phase, step, at } => Some((*phase, *step, *at)),
                _ => None,
            })
            .collect()
    }

    fn point(x: f32, y: f32) -> Point<CssPx, Css> {
        Point::new(CssPx(x), CssPx(y))
    }

    #[test]
    fn two_fingers_moving_apart_are_a_pinch_between_them() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let started = gestures.pointer(PointerAction::Pressed, &finger(2, 100.0, 0.0), stamp(0));
        assert_eq!(
            pinches(&started),
            [(GesturePhase::Started, 1.0, point(50.0, 0.0))]
        );

        let moved = gestures.pointer(PointerAction::Moved, &finger(2, 200.0, 0.0), stamp(20));
        assert_eq!(
            pinches(&moved),
            [(GesturePhase::Moved, 2.0, point(100.0, 0.0))]
        );
        let again = gestures.pointer(PointerAction::Moved, &finger(2, 300.0, 0.0), stamp(40));
        assert_eq!(
            pinches(&again),
            [(GesturePhase::Moved, 1.5, point(150.0, 0.0))],
            "each step is measured from the reading before"
        );

        let ended = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 0.0), stamp(60));
        assert_eq!(
            pinches(&ended),
            [(GesturePhase::Ended, 1.0, point(150.0, 0.0))]
        );
    }

    #[test]
    fn a_held_pinch_stops_the_pan_of_its_contacts() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let panned = gestures.pointer(PointerAction::Moved, &finger(1, 0.0, 40.0), stamp(20));
        assert!(matches!(panned.first(), Some(Gesture::PanStart { .. })));
        gestures.pointer(PointerAction::Pressed, &finger(2, 100.0, 40.0), stamp(30));
        gestures.hold_pinch();

        let moved = gestures.pointer(PointerAction::Moved, &finger(1, 0.0, 80.0), stamp(40));
        assert!(
            moved
                .iter()
                .all(|gesture| matches!(gesture, Gesture::Pinch { .. })),
            "{moved:?}"
        );
        gestures.pointer(PointerAction::Released, &finger(2, 100.0, 40.0), stamp(50));
        let left = gestures.pointer(PointerAction::Moved, &finger(1, 0.0, 120.0), stamp(60));
        let lifted = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 120.0), stamp(70));
        assert!(left.is_empty() && lifted.is_empty(), "{left:?} {lifted:?}");
    }

    #[test]
    fn a_third_finger_is_not_read_as_a_step() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(2, 0.0, 0.0), stamp(0));
        gestures.pointer(PointerAction::Pressed, &finger(3, 100.0, 0.0), stamp(0));
        // A lower id than the first two, as iOS can give a later touch.
        let third = gestures.pointer(PointerAction::Pressed, &finger(1, 10.0, 0.0), stamp(10));
        assert!(pinches(&third).is_empty());
        let moved = gestures.pointer(PointerAction::Moved, &finger(1, 20.0, 0.0), stamp(20));
        assert!(
            pinches(&moved).iter().all(|(_, step, _)| *step == 1.0),
            "the third finger is not one of the pair: {moved:?}"
        );
        let moved = gestures.pointer(PointerAction::Moved, &finger(3, 200.0, 0.0), stamp(30));
        assert_eq!(
            pinches(&moved),
            [(GesturePhase::Moved, 2.0, point(100.0, 0.0))]
        );
    }

    #[test]
    fn one_finger_alone_is_never_a_pinch() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        let read = gestures.pointer(PointerAction::Moved, &finger(1, 60.0, 0.0), stamp(20));
        assert!(!read.iter().any(|g| matches!(g, Gesture::Pinch { .. })));
    }

    #[test]
    fn a_held_finger_becomes_a_long_press_once_and_not_again() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        assert!(gestures.awaits_deadline());
        assert!(gestures.elapsed(stamp(300)).is_empty());
        assert_eq!(gestures.elapsed(stamp(600)).len(), 1);
        assert!(
            gestures.elapsed(stamp(900)).is_empty(),
            "a held finger must fire one long press, not one per frame for as long as it is held"
        );
        assert!(!gestures.awaits_deadline());
    }

    #[test]
    fn a_long_press_that_has_already_fired_is_not_also_a_tap() {
        let mut gestures = Gestures::new();
        gestures.pointer(PointerAction::Pressed, &finger(1, 0.0, 0.0), stamp(0));
        gestures.elapsed(stamp(600));
        let read = gestures.pointer(PointerAction::Released, &finger(1, 0.0, 0.0), stamp(700));
        assert!(
            read.is_empty(),
            "lifting after a context menu has opened must not also activate the row: {read:?}"
        );
    }
}
