//! Why a navigation stack refused to go back.

use core::fmt;

/// Why [`NavPath::pop`](crate::nav_stack::NavPath::pop) or
/// [`NavPath::pop_to_root`](crate::nav_stack::NavPath::pop_to_root) left the stack as it was.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum PopError {
    /// The root page is the only page.
    AtRoot,
    /// The top page blocks back, for example while it holds unsaved edits.
    Blocked,
}

impl fmt::Display for PopError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AtRoot => f.write_str("the stack holds only its root page"),
            Self::Blocked => f.write_str("the top page blocks back"),
        }
    }
}

impl core::error::Error for PopError {}
