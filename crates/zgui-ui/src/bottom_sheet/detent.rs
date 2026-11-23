//! The heights a bottom sheet rests at, and where a released drag goes.

/// A height a [`BottomSheet`](crate::bottom_sheet::BottomSheet) rests at.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Detent {
    /// About half the height of the window.
    Medium,
    /// The full height of the window, less a gap at the top.
    Large,
}

impl Detent {
    /// The value of the `data-detent` attribute, which the style sheet selects on.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }
}

/// The sizes a drag is measured against, in CSS pixels.
#[derive(Copy, Clone, PartialEq, Debug)]
pub(crate) struct Geometry {
    /// The height of the sheet at its large detent.
    pub(crate) panel: f32,
    /// The height of the window.
    pub(crate) window: f32,
    /// Whether the sheet is a form sheet, which has one resting place.
    pub(crate) form: bool,
}

impl Geometry {
    /// How far below its large detent the sheet rests at `detent`.
    pub(crate) fn offset(self, detent: Detent) -> f32 {
        if self.form {
            return 0.0;
        }
        match detent {
            Detent::Large => 0.0,
            Detent::Medium => (self.panel - self.window * 0.5).max(0.0),
        }
    }

    /// How far below its large detent the sheet is when it is off the window.
    pub(crate) fn dismissed(self) -> f32 {
        self.panel
    }

    /// Where the sheet is drawn for a drag at `offset`.
    ///
    /// Above the large detent, and below `floor` when there is one, the sheet resists the finger.
    pub(crate) fn resisted(self, offset: f32, floor: Option<f32>) -> f32 {
        if offset < 0.0 {
            return -resist(-offset, self.window);
        }
        match floor {
            Some(floor) if offset > floor => floor + resist(offset - floor, self.window),
            _ => offset,
        }
    }
}

/// Where a released drag sends the sheet.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub(crate) enum Release {
    /// The sheet rests at a detent.
    Rest(Detent),
    /// The sheet closes.
    Dismiss,
    /// The drag asked to close the sheet while dismissal is blocked. The sheet rests at a detent.
    Refused(Detent),
}

/// Where a drag released at `offset` with `velocity` sends the sheet.
///
/// `velocity` is in CSS pixels per second, downwards positive. The release projects the motion
/// forward as a scroll view does, and picks the nearest place to the projected end: one of
/// `detents`, or off the window.
pub(crate) fn release(
    offset: f32,
    velocity: f32,
    detents: &[Detent],
    geometry: Geometry,
    dismissable: bool,
) -> Release {
    // UIKit's normal deceleration rate. A flick travels `v * r / (1 - r) / 1000` points.
    const DECELERATION: f32 = 0.998;

    let projected = offset + velocity / 1000.0 * DECELERATION / (1.0 - DECELERATION);
    let Some(nearest) = detents.iter().copied().min_by(|a, b| {
        let a = (geometry.offset(*a) - projected).abs();
        let b = (geometry.offset(*b) - projected).abs();
        a.total_cmp(&b)
    }) else {
        return Release::Dismiss;
    };

    let to_detent = (geometry.offset(nearest) - projected).abs();
    let to_dismissed = (geometry.dismissed() - projected).abs();
    if to_dismissed >= to_detent {
        return Release::Rest(nearest);
    }

    if dismissable {
        Release::Dismiss
    } else {
        Release::Refused(nearest)
    }
}

/// How far a finger that travelled `distance` past a limit moves the sheet, as a scroll view
/// stretches past an end.
fn resist(distance: f32, dimension: f32) -> f32 {
    const COEFFICIENT: f32 = 0.55;

    if dimension <= 0.0 {
        return 0.0;
    }

    (1.0 - 1.0 / (distance * COEFFICIENT / dimension + 1.0)) * dimension
}

#[cfg(test)]
mod tests {
    use super::*;

    const PHONE: Geometry = Geometry {
        panel: 790.0,
        window: 874.0,
        form: false,
    };

    const BOTH: &[Detent] = &[Detent::Medium, Detent::Large];

    #[test]
    fn a_slow_release_rests_at_the_nearest_detent() {
        let medium = PHONE.offset(Detent::Medium);
        assert_eq!(medium, 353.0);

        assert_eq!(
            release(100.0, 0.0, BOTH, PHONE, true),
            Release::Rest(Detent::Large)
        );
        assert_eq!(
            release(300.0, 0.0, BOTH, PHONE, true),
            Release::Rest(Detent::Medium)
        );
        assert_eq!(
            release(560.0, 0.0, BOTH, PHONE, true),
            Release::Rest(Detent::Medium)
        );
        assert_eq!(release(600.0, 0.0, BOTH, PHONE, true), Release::Dismiss);
    }

    #[test]
    fn a_flick_carries_the_sheet_past_the_nearest_detent() {
        // Released near the large detent, but moving down fast.
        assert_eq!(
            release(50.0, 900.0, BOTH, PHONE, true),
            Release::Rest(Detent::Medium)
        );
        assert_eq!(release(50.0, 2000.0, BOTH, PHONE, true), Release::Dismiss);
        // Released near medium, moving up.
        assert_eq!(
            release(340.0, -800.0, BOTH, PHONE, true),
            Release::Rest(Detent::Large)
        );
    }

    #[test]
    fn a_blocked_dismissal_rests_at_a_detent_and_says_so() {
        assert_eq!(
            release(700.0, 500.0, BOTH, PHONE, false),
            Release::Refused(Detent::Medium)
        );
        assert_eq!(
            release(600.0, 0.0, &[Detent::Large], PHONE, false),
            Release::Refused(Detent::Large)
        );
    }

    #[test]
    fn one_detent_closes_past_half_of_the_sheet() {
        assert_eq!(
            release(390.0, 0.0, &[Detent::Large], PHONE, true),
            Release::Rest(Detent::Large)
        );
        assert_eq!(
            release(400.0, 0.0, &[Detent::Large], PHONE, true),
            Release::Dismiss
        );
    }

    #[test]
    fn a_form_sheet_has_one_resting_place() {
        let form = Geometry {
            panel: 620.0,
            window: 834.0,
            form: true,
        };
        assert_eq!(form.offset(Detent::Medium), 0.0);
    }

    #[test]
    fn the_sheet_resists_past_its_limits() {
        assert_eq!(PHONE.resisted(200.0, None), 200.0);

        let above = PHONE.resisted(-200.0, None);
        assert!(above < 0.0 && above > -200.0, "{above}");

        let floor = PHONE.offset(Detent::Medium);
        let below = PHONE.resisted(floor + 200.0, Some(floor));
        assert!(below > floor && below < floor + 200.0, "{below}");
    }
}
