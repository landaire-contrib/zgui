//! When the iOS text view takes and gives up the keyboard.
//!
//! The keyboard reads the text around the caret when its view becomes the first responder, and
//! sets its shift key and its suggestions from that text. So the view becomes the first responder
//! only after it holds the snapshot of the field that has focus. Focus that moves straight from one
//! field to another ends one session and starts the next, as a switch between two UIKit text fields
//! does, and the keyboard stays on the screen.

use zgui_platform::TextInputSession;

/// What the text view does with the next snapshot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Step {
    /// Compare the snapshot with the copy and tell UIKit what changed. With `announce`, tell
    /// UIKit the text changed even when the copy holds it already.
    Refresh {
        /// Whether UIKit's own idea of the text is out of date.
        announce: bool,
    },
    /// Take the snapshot as the start of a session, then become the first responder.
    Begin {
        /// Whether to give up the first responder first, which ends the session before.
        restart: bool,
    },
}

/// The session the text view takes part in, and what it owes the keyboard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Keyboard {
    /// The session of the field with focus, and nothing while no field has focus.
    session: Option<TextInputSession>,
    /// Whether the view waits for a snapshot to become the first responder.
    begin: Option<Begin>,
    /// Whether UIKit thinks text was inserted that the field did not take.
    stale: bool,
}

/// How the next session starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Begin {
    /// The view does not have the keyboard.
    Start,
    /// The view has the keyboard for another field.
    Restart,
}

impl Keyboard {
    /// No field has focus.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// A field has focus in `session`. `responding` is whether the view is the first responder.
    pub(crate) fn focus(&mut self, session: Option<TextInputSession>, responding: bool) {
        let switched = session.is_some() && session != self.session;
        self.session = session.or(self.session);
        self.begin = match (responding, switched) {
            (false, _) => Some(Begin::Start),
            (true, true) => Some(Begin::Restart),
            (true, false) => self.begin,
        };
    }

    /// No field has focus.
    pub(crate) fn blur(&mut self) {
        *self = Self::new();
    }

    /// The keyboard sent a line break, which the field takes as a press of Enter.
    pub(crate) fn returned(&mut self) {
        self.stale = true;
    }

    /// What to do with the next snapshot.
    pub(crate) fn snapshot(&mut self) -> Step {
        match self.begin.take() {
            Some(begin) => {
                self.stale = false;
                Step::Begin {
                    restart: begin == Begin::Restart,
                }
            }
            None => Step::Refresh {
                announce: self.stale,
            },
        }
    }

    /// The snapshot was taken, and UIKit knows the text.
    pub(crate) fn announced(&mut self) {
        self.stale = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIRST: Option<TextInputSession> = Some(TextInputSession(1));
    const SECOND: Option<TextInputSession> = Some(TextInputSession(2));

    #[test]
    fn a_focused_field_takes_the_keyboard_after_its_snapshot() {
        let mut keyboard = Keyboard::new();
        keyboard.focus(FIRST, false);
        assert_eq!(keyboard.snapshot(), Step::Begin { restart: false });
        assert_eq!(keyboard.snapshot(), Step::Refresh { announce: false });
    }

    #[test]
    fn a_caret_move_in_the_same_field_keeps_the_session() {
        let mut keyboard = Keyboard::new();
        keyboard.focus(FIRST, false);
        keyboard.snapshot();
        keyboard.focus(FIRST, true);
        assert_eq!(keyboard.snapshot(), Step::Refresh { announce: false });
    }

    #[test]
    fn a_switch_to_another_field_restarts_with_the_keyboard_up() {
        let mut keyboard = Keyboard::new();
        keyboard.focus(FIRST, false);
        keyboard.snapshot();
        keyboard.focus(SECOND, true);
        assert_eq!(keyboard.snapshot(), Step::Begin { restart: true });
    }

    #[test]
    fn focus_after_a_blur_starts_again() {
        let mut keyboard = Keyboard::new();
        keyboard.focus(FIRST, false);
        keyboard.snapshot();
        keyboard.blur();
        keyboard.focus(FIRST, false);
        assert_eq!(keyboard.snapshot(), Step::Begin { restart: false });
    }

    #[test]
    fn a_return_the_field_took_is_announced_once() {
        let mut keyboard = Keyboard::new();
        keyboard.focus(FIRST, false);
        keyboard.snapshot();
        keyboard.returned();
        assert_eq!(keyboard.snapshot(), Step::Refresh { announce: true });
        keyboard.announced();
        assert_eq!(keyboard.snapshot(), Step::Refresh { announce: false });
    }

    #[test]
    fn a_state_with_no_session_keeps_the_one_held() {
        let mut keyboard = Keyboard::new();
        keyboard.focus(FIRST, false);
        keyboard.snapshot();
        keyboard.focus(None, true);
        assert_eq!(keyboard.snapshot(), Step::Refresh { announce: false });
    }
}
