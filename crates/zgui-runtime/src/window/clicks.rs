//! Counting clicks into double clicks.
//!
//! A pointer activation that lands on the element the last one landed on, within the desktop's
//! double-click time and distance, is a double click. The window dispatches `double_click` after
//! the second `click`, down the same path. A third press starts a new count, so a triple press
//! gives one double click.

use zgui_dom::NodeKey;
use zgui_geom::{Css, CssPx, Point};
use zgui_vocab::{ClickSettings, Timestamp};

/// The last pointer activation that may start a double click.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ClickCount {
    /// The element, when, and where.
    last: Option<(NodeKey, Timestamp, Point<CssPx, Css>)>,
}

impl ClickCount {
    /// Notes a pointer activation of `node` at `at` and `when`, and answers whether it completes
    /// a double click under `settings`.
    pub(crate) fn note(
        &mut self,
        settings: ClickSettings,
        node: NodeKey,
        when: Timestamp,
        at: Point<CssPx, Css>,
    ) -> bool {
        let double = self.last.is_some_and(|(held, then, there)| {
            follows(settings, (held, then, there), (node, when, at))
        });
        self.last = if double { None } else { Some((node, when, at)) };
        double
    }

    /// Forgets the last activation, as a keyboard activation does.
    pub(crate) fn forget(&mut self) {
        self.last = None;
    }
}

impl crate::window::Window {
    /// Declares the desktop's double-click time and distance. Set when the window is opened, from
    /// the backend's own answer.
    pub(crate) fn set_click_settings(&mut self, settings: ClickSettings) {
        self.click_settings = settings;
    }

    /// The desktop's double-click time and distance.
    pub fn click_settings(&self) -> ClickSettings {
        self.click_settings
    }
}

/// Whether a press of `node` at `at` and `when` follows the earlier one on the same element
/// within `settings`.
pub(crate) fn follows(
    settings: ClickSettings,
    (held, then, there): (NodeKey, Timestamp, Point<CssPx, Css>),
    (node, when, at): (NodeKey, Timestamp, Point<CssPx, Css>),
) -> bool {
    held == node && settings.follows((then, there), (when, at))
}
