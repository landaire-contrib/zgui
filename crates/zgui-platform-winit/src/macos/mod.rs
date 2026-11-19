//! The AppKit objects winit does not expose: the application menu and its key equivalents, the
//! status item, the Dock icon, the reopen request, the title-bar buttons, the title-bar
//! double-click preference and the momentum phase of a scroll.
//!
//! Every function here runs on the main thread, inside a callback of the event loop.

mod buttons;
mod dock;
mod double_click;
mod fullscreen;
mod keys;
mod menu;
mod reopen;
mod scroll;
mod status_item;
mod status_press;
mod window;

pub(crate) use crate::macos::buttons::place as place_title_buttons;
pub(crate) use crate::macos::dock::set_visible as set_dock_icon_visible;
pub(crate) use crate::macos::double_click::perform as title_bar_double_click;
pub(crate) use crate::macos::fullscreen::{
    forget as forget_surface, observe as observe_fullscreen,
};
pub(crate) use crate::macos::keys::install as answer_key_equivalents;
pub(crate) use crate::macos::menu::{Menus, install as install_menu, listen as listen_to_menus};
pub(crate) use crate::macos::reopen::install as install_reopen;
pub(crate) use crate::macos::scroll::{install as read_scroll_phases, phase as scroll_phase};
pub(crate) use crate::macos::status_item::{StatusItems, install as install_status_item};
pub(crate) use crate::macos::window::ns_view;
