//! Haptics through UIKit's feedback generators.

use std::cell::RefCell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_ui_kit::{
    UIImpactFeedbackGenerator, UIImpactFeedbackStyle, UINotificationFeedbackGenerator,
    UINotificationFeedbackType, UISelectionFeedbackGenerator,
};
use zgui_platform::{Haptic, ImpactWeight, Outcome};

/// The generators made so far, one of each kind and weight, kept so that a series of haptics
/// does not make a generator for each.
#[derive(Default)]
struct Generators {
    light: Option<Retained<UIImpactFeedbackGenerator>>,
    medium: Option<Retained<UIImpactFeedbackGenerator>>,
    heavy: Option<Retained<UIImpactFeedbackGenerator>>,
    selection: Option<Retained<UISelectionFeedbackGenerator>>,
    notification: Option<Retained<UINotificationFeedbackGenerator>>,
}

thread_local! {
    static GENERATORS: RefCell<Generators> = RefCell::new(Generators::default());
}

/// Plays `haptic`. Does nothing off the main thread.
pub(crate) fn play(haptic: Haptic) {
    let Some(main) = MainThreadMarker::new() else {
        return;
    };

    GENERATORS.with(|generators| {
        let generators = &mut *generators.borrow_mut();
        match haptic {
            Haptic::Impact(weight) => {
                let Some((held, style)) = impact(generators, weight) else {
                    return;
                };
                held.get_or_insert_with(|| impact_generator(main, style))
                    .impactOccurred();
            }
            Haptic::Selection => generators
                .selection
                .get_or_insert_with(|| UISelectionFeedbackGenerator::new(main))
                .selectionChanged(),
            Haptic::Notification(outcome) => {
                let Some(kind) = notification(outcome) else {
                    return;
                };
                generators
                    .notification
                    .get_or_insert_with(|| UINotificationFeedbackGenerator::new(main))
                    .notificationOccurred(kind);
            }
            _ => {}
        }
    });
}

/// The held generator and the UIKit style for `weight`, when UIKit has one.
fn impact(
    generators: &mut Generators,
    weight: ImpactWeight,
) -> Option<(
    &mut Option<Retained<UIImpactFeedbackGenerator>>,
    UIImpactFeedbackStyle,
)> {
    match weight {
        ImpactWeight::Light => Some((&mut generators.light, UIImpactFeedbackStyle::Light)),
        ImpactWeight::Medium => Some((&mut generators.medium, UIImpactFeedbackStyle::Medium)),
        ImpactWeight::Heavy => Some((&mut generators.heavy, UIImpactFeedbackStyle::Heavy)),
        _ => None,
    }
}

/// The UIKit feedback type for `outcome`, when UIKit has one.
fn notification(outcome: Outcome) -> Option<UINotificationFeedbackType> {
    match outcome {
        Outcome::Success => Some(UINotificationFeedbackType::Success),
        Outcome::Warning => Some(UINotificationFeedbackType::Warning),
        Outcome::Error => Some(UINotificationFeedbackType::Error),
        _ => None,
    }
}

#[expect(
    deprecated,
    reason = "the generator for a view replaces this from iOS 17.5 only, and older versions are supported"
)]
fn impact_generator(
    main: MainThreadMarker,
    style: UIImpactFeedbackStyle,
) -> Retained<UIImpactFeedbackGenerator> {
    UIImpactFeedbackGenerator::initWithStyle(main.alloc(), style)
}
