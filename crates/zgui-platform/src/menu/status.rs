//! The item an application shows in the desktop's status area.

use zgui_geom::{Css, CssPx, Rect};
use zgui_vocab::SharedString;

use crate::menu::model::MenuEntry;
use crate::surface::WindowIcon;

/// The picture of a status item.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatusIcon {
    /// The pixels, as straight RGBA rows.
    pub image: WindowIcon,
    /// Whether the desktop draws the picture as a mask in its own colors.
    ///
    /// macOS uses the alpha channel of a template picture and ignores its colors, so the picture
    /// follows a light or dark menu bar. Other desktops ignore it.
    pub template: bool,
}

impl StatusIcon {
    /// A picture drawn in its own colors.
    pub fn new(image: WindowIcon) -> Self {
        Self {
            image,
            template: false,
        }
    }

    /// The same picture, drawn as a mask or in its own colors.
    #[must_use]
    pub fn with_template(mut self, template: bool) -> Self {
        self.template = template;
        self
    }
}

/// What a primary click on a status item does.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StatusPress {
    /// The click opens the item's menu.
    #[default]
    OpensMenu,
    /// The click arrives as [`AppEvent::StatusItemPressed`](crate::AppEvent::StatusItemPressed),
    /// with where the item is, and the item's menu opens on a secondary click instead.
    ///
    /// A secondary click is a right click, or a click with Control held on macOS.
    Reports,
}

/// An item in the desktop's status area, with the menu it opens.
///
/// On macOS this is a menu bar extra at the right of the menu bar. A click opens the menu, or
/// reports the press when [`StatusItem::press`] says so. What the user picks from the menu arrives
/// as [`AppEvent::Menu`](crate::AppEvent::Menu).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StatusItem {
    /// The picture the item shows.
    pub icon: Option<StatusIcon>,
    /// The text the item shows beside the picture.
    pub title: Option<SharedString>,
    /// The text the desktop shows while the pointer is over the item.
    pub tooltip: Option<SharedString>,
    /// What a primary click on the item does.
    pub press: StatusPress,
    /// The entries of the menu the item opens, top to bottom.
    pub menu: Vec<MenuEntry>,
}

impl StatusItem {
    /// An item with no picture, no text and an empty menu, whose primary click opens the menu.
    pub fn new() -> Self {
        Self::default()
    }

    /// The same item, showing `icon`.
    #[must_use]
    pub fn with_icon(mut self, icon: StatusIcon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// The same item, showing `title` beside its picture.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<SharedString>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// The same item, with `tooltip` over it.
    #[must_use]
    pub fn with_tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// The same item, with `press` as what a primary click does.
    #[must_use]
    pub fn with_press(mut self, press: StatusPress) -> Self {
        self.press = press;
        self
    }

    /// The same item, with `entry` added at the bottom of its menu.
    #[must_use]
    pub fn with(mut self, entry: impl Into<MenuEntry>) -> Self {
        self.menu.push(entry.into());
        self
    }

    /// The same item, with a separator added at the bottom of its menu.
    #[must_use]
    pub fn separator(mut self) -> Self {
        self.menu.push(MenuEntry::Separator);
        self
    }
}

/// Where a status item is on the screen, as reported with a press.
///
/// Both rectangles are in the space [`SurfaceAttributes::position`] is measured in: CSS pixels
/// from the desktop's origin, growing right and down. A window opened at the bottom left of
/// `item` sits right under the item.
///
/// [`SurfaceAttributes::position`]: crate::SurfaceAttributes::position
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct StatusAnchor {
    /// The item itself.
    pub item: Rect<CssPx, Css>,
    /// The part of the item's screen that windows may cover.
    ///
    /// The screen without the menu bar, the Dock or a task bar. A window kept inside it stays
    /// fully on screen.
    pub work_area: Rect<CssPx, Css>,
}

impl StatusAnchor {
    /// An item at `item`, on a screen whose windows may cover `work_area`.
    pub const fn new(item: Rect<CssPx, Css>, work_area: Rect<CssPx, Css>) -> Self {
        Self { item, work_area }
    }
}
