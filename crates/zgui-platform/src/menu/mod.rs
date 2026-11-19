//! The menus a desktop shows outside the windows.
//!
//! macOS shows one menu bar for the whole application. Other desktops have no such place, and a
//! backend without one ignores the menu. [`PlatformCapabilities::app_menu`] says which applies.
//!
//! A [`StatusItem`] is an item in the desktop's status area, with a menu of its own.
//! [`PlatformCapabilities::status_item`] says whether the desktop shows one.
//!
//! [`PlatformCapabilities::app_menu`]: crate::PlatformCapabilities::app_menu
//! [`PlatformCapabilities::status_item`]: crate::PlatformCapabilities::status_item

mod model;
mod role;
mod shortcut;
mod status;

pub use crate::menu::model::{AppMenu, MenuAction, MenuEntry, MenuId, Submenu};
pub use crate::menu::role::MenuRole;
pub use crate::menu::shortcut::Shortcut;
pub use crate::menu::status::{StatusIcon, StatusItem};

#[cfg(test)]
mod tests;
