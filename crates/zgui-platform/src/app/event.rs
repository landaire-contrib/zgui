//! What the desktop says to the application as a whole.

use crate::menu::{MenuId, MenuRole, StatusAnchor};

/// Something the desktop asked of the application rather than of one window.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum AppEvent {
    /// The user asked for the application again: a click on its dock icon, or a second launch.
    ///
    /// An application that hides its last window shows it again here.
    Reopen,
    /// The user picked an action of the application menu.
    Menu(MenuId),
    /// The user picked an editing role of the application menu.
    ///
    /// The framework replays the role's shortcut into the focused window.
    Role(MenuRole),
    /// The user clicked a status item that reports its presses, with the primary button.
    ///
    /// Only an item whose [`press`](crate::StatusItem::press) is
    /// [`StatusPress::Reports`](crate::StatusPress::Reports) reports one.
    StatusItemPressed(StatusAnchor),
    /// The application moved between the active, inactive, background and foreground states.
    Lifecycle(Lifecycle),
}

/// A move of the application between the states the system runs it in.
///
/// The names are those of UIKit's application notifications. An application hides what it shows
/// on [`Lifecycle::WillResignActive`], because the system takes the picture for its application
/// switcher after that. The windows and their content stay through every move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Lifecycle {
    /// The application is about to stop receiving input: the user opened the application
    /// switcher, a system alert shows, or the application is about to go to the background.
    WillResignActive,
    /// The application receives input again.
    DidBecomeActive,
    /// The application is in the background, and nothing of it shows.
    DidEnterBackground,
    /// The application is about to show again after it was in the background.
    WillEnterForeground,
}
