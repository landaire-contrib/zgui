//! Why a navigation stack refused to go back.

/// Why [`NavPath::pop`](crate::nav_stack::NavPath::pop) or
/// [`NavPath::pop_to_root`](crate::nav_stack::NavPath::pop_to_root) left the stack as it was.
#[derive(Copy, Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum PopError {
    /// The root page is the only page.
    #[error("the stack holds only its root page")]
    AtRoot,
    /// The top page blocks back, for example while it holds unsaved edits.
    #[error("the top page blocks back")]
    Blocked,
}
