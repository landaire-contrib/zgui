//! The `:active` state of a finger on a touch screen, which waits before it shows.
//!
//! A finger that comes down may start a scroll, and a row that lights up under every scroll
//! flickers. So a touch press shows `:active` only after it is held for a moment, as UIKit shows a
//! highlight. A pan that starts before then cancels it. A quick tap that lifts before then shows
//! `:active` briefly, so the person sees what they tapped.
//!
//! The router keeps the stage. The window keeps the clock: it calls
//! [`Router::reveal_touch_press`] when the wait is over and [`Router::end_touch_flash`] when the
//! brief state is over.

use zgui_dom::{Document, StyleFilter};
use zgui_vocab::PointerId;

use crate::hit::HitChain;
use crate::router::Router;
use crate::state::within::Moved;

/// How long a finger is held before its press shows.
pub const TOUCH_PRESS_DELAY: core::time::Duration = core::time::Duration::from_millis(100);

/// How long the press of a quick tap shows after the finger lifts.
pub const TOUCH_FLASH: core::time::Duration = core::time::Duration::from_millis(100);

/// Where the `:active` state of a touch press is.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TouchPress {
    /// The finger is down and its press does not show yet.
    Waiting {
        /// The finger.
        pointer: PointerId,
        /// What it pressed.
        chain: HitChain,
    },
    /// The finger lifted before its press showed, and the press shows briefly.
    Flashing,
}

/// Which stage of a touch press waits for the clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TouchPressStage {
    /// The press shows when [`TOUCH_PRESS_DELAY`] has passed.
    Waiting,
    /// The press goes away when [`TOUCH_FLASH`] has passed.
    Flashing,
}

impl TouchPressStage {
    /// How long this stage lasts.
    pub const fn duration(self) -> core::time::Duration {
        match self {
            Self::Waiting => TOUCH_PRESS_DELAY,
            Self::Flashing => TOUCH_FLASH,
        }
    }
}

impl Router {
    /// The stage of the touch press that waits for the clock, if one does.
    pub fn touch_press(&self) -> Option<TouchPressStage> {
        match self.touch {
            Some(TouchPress::Waiting { .. }) => Some(TouchPressStage::Waiting),
            Some(TouchPress::Flashing) => Some(TouchPressStage::Flashing),
            None => None,
        }
    }

    /// Shows the press of a finger that is still down.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn reveal_touch_press(&mut self, document: &Document, filter: &dyn StyleFilter) -> Moved {
        let Some(TouchPress::Waiting { chain, .. }) = self.touch.take() else {
            return Moved::default();
        };
        self.interaction.active.press(document, filter, &chain)
    }

    /// Ends the brief press of a quick tap.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn end_touch_flash(&mut self, document: &Document, filter: &dyn StyleFilter) -> Moved {
        if self.touch != Some(TouchPress::Flashing) {
            return Moved::default();
        }
        self.touch = None;
        self.interaction.active.release(document, filter)
    }

    /// Takes the press of `pointer` away, because the finger started to pan.
    ///
    /// A press that waits never shows. A press that shows goes away.
    ///
    /// # Panics
    ///
    /// Panics if the document is poisoned by an earlier batch that unwound.
    pub fn cancel_touch_press(
        &mut self,
        pointer: PointerId,
        document: &Document,
        filter: &dyn StyleFilter,
    ) -> Moved {
        if let Some(TouchPress::Waiting { pointer: held, .. }) = &self.touch {
            if *held == pointer {
                self.touch = None;
            }
            return Moved::default();
        }
        self.interaction.active.release(document, filter)
    }

    /// Holds the press of `pointer` on `chain` until the clock shows it, and takes away what
    /// shows now.
    pub(crate) fn wait_touch_press(
        &mut self,
        pointer: PointerId,
        chain: &HitChain,
        document: &Document,
        filter: &dyn StyleFilter,
    ) -> Moved {
        self.touch = Some(TouchPress::Waiting {
            pointer,
            chain: chain.clone(),
        });
        self.interaction.active.release(document, filter)
    }

    /// Ends the touch press of `pointer`. A press that still waits shows briefly when the finger
    /// `lifted`, and never shows when the contact was cancelled.
    pub(crate) fn lift_touch_press(
        &mut self,
        pointer: PointerId,
        lifted: bool,
        document: &Document,
        filter: &dyn StyleFilter,
    ) -> Moved {
        let waiting = match &self.touch {
            Some(TouchPress::Waiting {
                pointer: held,
                chain,
            }) if *held == pointer => Some(chain.clone()),
            _ => None,
        };
        match waiting {
            Some(chain) if lifted => {
                self.touch = Some(TouchPress::Flashing);
                self.interaction.active.press(document, filter, &chain)
            }
            Some(_) => {
                self.touch = None;
                Moved::default()
            }
            None => self.interaction.active.release(document, filter),
        }
    }
}
