//! The item an application shows in the desktop's status area.

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

/// An item in the desktop's status area, with the menu it opens.
///
/// On macOS this is a menu bar extra at the right of the menu bar. A click opens the menu, and
/// what the user picks arrives as [`AppEvent::Menu`](crate::AppEvent::Menu).
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct StatusItem {
    /// The picture the item shows.
    pub icon: Option<StatusIcon>,
    /// The text the item shows beside the picture.
    pub title: Option<SharedString>,
    /// The text the desktop shows while the pointer is over the item.
    pub tooltip: Option<SharedString>,
    /// The entries of the menu the item opens, top to bottom.
    pub menu: Vec<MenuEntry>,
}

impl StatusItem {
    /// An item with no picture, no text and an empty menu.
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
