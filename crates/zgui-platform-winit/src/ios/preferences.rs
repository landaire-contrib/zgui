//! The Dynamic Type text size and the Reduce Motion setting.

use core::ptr::NonNull;
use std::cell::OnceCell;
use std::sync::Arc;

use block2::RcBlock;
use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::{NSNotification, NSNotificationCenter, NSObjectProtocol};
use objc2_ui_kit::{
    UIAccessibilityIsReduceMotionEnabled, UIAccessibilityReduceMotionStatusDidChangeNotification,
    UIApplication, UIContentSizeCategory, UIContentSizeCategoryAccessibilityExtraExtraExtraLarge,
    UIContentSizeCategoryAccessibilityExtraExtraLarge,
    UIContentSizeCategoryAccessibilityExtraLarge, UIContentSizeCategoryAccessibilityLarge,
    UIContentSizeCategoryAccessibilityMedium, UIContentSizeCategoryDidChangeNotification,
    UIContentSizeCategoryExtraExtraExtraLarge, UIContentSizeCategoryExtraExtraLarge,
    UIContentSizeCategoryExtraLarge, UIContentSizeCategoryExtraSmall, UIContentSizeCategoryLarge,
    UIContentSizeCategoryMedium, UIContentSizeCategorySmall,
};
use zgui_platform::{MotionPreference, TextScale, WakeReason, Waker};

use crate::uikit::ContentSizeCategory;

thread_local! {
    /// The observers of the two settings, for the life of the process.
    static OBSERVERS: OnceCell<[Retained<ProtocolObject<dyn NSObjectProtocol>>; 2]> =
        const { OnceCell::new() };
}

/// Tells `waker` when the user changes the text size or the Reduce Motion setting.
///
/// Observing twice does nothing.
pub(crate) fn observe(waker: Arc<dyn Waker>) {
    OBSERVERS.with(|observers| {
        observers.get_or_init(|| {
            let center = NSNotificationCenter::defaultCenter();
            let block = RcBlock::new(move |_: NonNull<NSNotification>| {
                waker.wake(WakeReason::PreferencesChanged);
            });
            // SAFETY: the names are UIKit's own, and the block only posts a wake, which any
            // thread may do.
            unsafe {
                [
                    UIContentSizeCategoryDidChangeNotification,
                    UIAccessibilityReduceMotionStatusDidChangeNotification,
                ]
                .map(|name| {
                    center.addObserverForName_object_queue_usingBlock(
                        Some(name),
                        None,
                        None,
                        &block,
                    )
                })
            }
        });
    });
}

/// The ratio of the body text size the user chose to the default one.
pub(crate) fn text_scale(main: MainThreadMarker) -> TextScale {
    let category = UIApplication::sharedApplication(main).preferredContentSizeCategory();
    // `UIContentSizeCategoryUnspecified` asks for the default size.
    content_size_category(&category).map_or(TextScale::DEFAULT, ContentSizeCategory::text_scale)
}

/// Whether the user turned Reduce Motion on.
pub(crate) fn motion_preference() -> MotionPreference {
    if UIAccessibilityIsReduceMotionEnabled() {
        MotionPreference::Reduce
    } else {
        MotionPreference::NoPreference
    }
}

/// The category UIKit names `category`, or `None` for the unspecified one.
fn content_size_category(category: &UIContentSizeCategory) -> Option<ContentSizeCategory> {
    // SAFETY: the constants are UIKit's own strings.
    let known = unsafe {
        [
            (
                UIContentSizeCategoryExtraSmall,
                ContentSizeCategory::ExtraSmall,
            ),
            (UIContentSizeCategorySmall, ContentSizeCategory::Small),
            (UIContentSizeCategoryMedium, ContentSizeCategory::Medium),
            (UIContentSizeCategoryLarge, ContentSizeCategory::Large),
            (
                UIContentSizeCategoryExtraLarge,
                ContentSizeCategory::ExtraLarge,
            ),
            (
                UIContentSizeCategoryExtraExtraLarge,
                ContentSizeCategory::ExtraExtraLarge,
            ),
            (
                UIContentSizeCategoryExtraExtraExtraLarge,
                ContentSizeCategory::ExtraExtraExtraLarge,
            ),
            (
                UIContentSizeCategoryAccessibilityMedium,
                ContentSizeCategory::AccessibilityMedium,
            ),
            (
                UIContentSizeCategoryAccessibilityLarge,
                ContentSizeCategory::AccessibilityLarge,
            ),
            (
                UIContentSizeCategoryAccessibilityExtraLarge,
                ContentSizeCategory::AccessibilityExtraLarge,
            ),
            (
                UIContentSizeCategoryAccessibilityExtraExtraLarge,
                ContentSizeCategory::AccessibilityExtraExtraLarge,
            ),
            (
                UIContentSizeCategoryAccessibilityExtraExtraExtraLarge,
                ContentSizeCategory::AccessibilityExtraExtraExtraLarge,
            ),
        ]
    };
    known
        .into_iter()
        .find(|(name, _)| **name == *category)
        .map(|(_, ours)| ours)
}
