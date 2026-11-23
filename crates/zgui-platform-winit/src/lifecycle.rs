//! The background and foreground moves of the application, from the visibility of its windows.
//!
//! UIKit hides every window of a scene as the scene goes to the background, and shows them again
//! as it comes back. The application is in the background while every window is hidden. iOS stops
//! an application that uses the GPU in the background, so no window draws there.

use std::collections::HashSet;
use std::hash::Hash;

use zgui_platform::Lifecycle;

/// Which windows are hidden, whether that puts the application in the background, and which
/// redraws wait for the foreground.
#[derive(Debug)]
pub(crate) struct Visibility<W> {
    /// The windows the system hid.
    hidden: HashSet<W>,
    /// The windows asked to redraw while the application was in the background.
    withheld: HashSet<W>,
    /// Whether the application is in the background.
    background: bool,
}

impl<W: Copy + Eq + Hash> Visibility<W> {
    /// An application in the foreground with no hidden window.
    pub(crate) fn new() -> Self {
        Self {
            hidden: HashSet::new(),
            withheld: HashSet::new(),
            background: false,
        }
    }

    /// Records that the system hid or showed `window`, one of `windows`, and gives the move of
    /// the application that follows.
    ///
    /// The application goes to the background when the last of `windows` is hidden, and comes
    /// back when the first one shows.
    pub(crate) fn occluded(
        &mut self,
        window: W,
        occluded: bool,
        windows: impl IntoIterator<Item = W>,
    ) -> Option<Lifecycle> {
        if occluded {
            self.hidden.insert(window);
            if self.background
                || !windows
                    .into_iter()
                    .all(|other| self.hidden.contains(&other))
            {
                return None;
            }
            self.background = true;
            Some(Lifecycle::DidEnterBackground)
        } else {
            self.hidden.remove(&window);
            if !self.background {
                return None;
            }
            self.background = false;
            Some(Lifecycle::WillEnterForeground)
        }
    }

    /// Whether a redraw of `window` runs now.
    ///
    /// A redraw in the background is withheld until [`Visibility::take_withheld`] gives it back.
    pub(crate) fn admits_redraw(&mut self, window: W) -> bool {
        if self.background {
            self.withheld.insert(window);
        }
        !self.background
    }

    /// Whether a redraw of `window` was withheld in the background, and is owed now.
    pub(crate) fn take_withheld(&mut self, window: W) -> bool {
        !self.background && self.withheld.remove(&window)
    }

    /// Forgets `window`, which was destroyed.
    #[cfg(target_os = "ios")]
    pub(crate) fn forget(&mut self, window: W) {
        self.hidden.remove(&window);
        self.withheld.remove(&window);
    }
}

#[cfg(test)]
mod tests {
    use zgui_platform::Lifecycle;

    use super::Visibility;

    #[test]
    fn the_application_goes_to_the_background_when_its_last_window_is_hidden() {
        let mut visibility = Visibility::new();
        assert_eq!(visibility.occluded(1, true, [1, 2]), None);
        assert_eq!(
            visibility.occluded(2, true, [1, 2]),
            Some(Lifecycle::DidEnterBackground)
        );
        assert_eq!(visibility.occluded(2, true, [1, 2]), None);
    }

    #[test]
    fn the_application_comes_back_when_its_first_window_shows() {
        let mut visibility = Visibility::new();
        visibility.occluded(1, true, [1, 2]);
        visibility.occluded(2, true, [1, 2]);
        assert_eq!(
            visibility.occluded(1, false, [1, 2]),
            Some(Lifecycle::WillEnterForeground)
        );
        assert_eq!(visibility.occluded(2, false, [1, 2]), None);
    }

    #[test]
    fn a_window_shown_in_the_foreground_moves_nothing() {
        let mut visibility = Visibility::new();
        assert_eq!(visibility.occluded(1, false, [1]), None);
    }

    #[test]
    fn a_redraw_in_the_background_waits_for_the_window_to_show() {
        let mut visibility = Visibility::new();
        assert!(visibility.admits_redraw(1));
        visibility.occluded(1, true, [1]);
        assert!(!visibility.admits_redraw(1));
        assert!(!visibility.take_withheld(1), "owed only in the foreground");

        visibility.occluded(1, false, [1]);
        assert!(visibility.admits_redraw(1));
        assert!(visibility.take_withheld(1));
        assert!(!visibility.take_withheld(1), "owed once");
    }
}
