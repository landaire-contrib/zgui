//! The desktop's double-click time and distance, as far as a windowing backend can read them.

use zgui_platform::ClickSettings;

/// The double-click time and distance of this desktop.
///
/// Windows and macOS state the time in a system setting, and Windows states the distance. Other
/// desktops have no setting a windowing backend can read, so they get the 400 ms and 5 pixels that
/// GPUI uses on Linux.
///
/// ```
/// let settings = zgui_platform_winit::desktop_click_settings();
/// assert!(!settings.interval.is_zero());
/// assert!(settings.slop.0 > 0.0);
/// ```
pub fn desktop_click_settings() -> ClickSettings {
    platform::settings()
}

#[cfg(windows)]
mod platform {
    use std::time::Duration;

    use windows_sys::Win32::UI::HiDpi::GetSystemMetricsForDpi;
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime;
    use windows_sys::Win32::UI::WindowsAndMessaging::{SM_CXDOUBLECLK, SM_CYDOUBLECLK};
    use zgui_geom::CssPx;
    use zgui_platform::ClickSettings;

    /// The DPI at which one device pixel is one CSS pixel.
    const CSS_DPI: u32 = 96;

    pub(super) fn settings() -> ClickSettings {
        // SAFETY: both calls only read system settings and take no pointers.
        let (millis, width, height) = unsafe {
            (
                GetDoubleClickTime(),
                GetSystemMetricsForDpi(SM_CXDOUBLECLK, CSS_DPI),
                GetSystemMetricsForDpi(SM_CYDOUBLECLK, CSS_DPI),
            )
        };
        let fallback = ClickSettings::desktop();
        let interval = if millis == 0 {
            fallback.interval
        } else {
            Duration::from_millis(u64::from(millis))
        };
        // The metrics are the full width and height of the rectangle centred on the first press.
        let slop = match width.max(height) {
            side if side > 0 => CssPx(side as f32 / 2.0),
            _ => fallback.slop,
        };

        fallback.with_interval(interval).with_slop(slop)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::time::Duration;

    use objc2_app_kit::NSEvent;
    use zgui_platform::ClickSettings;

    pub(super) fn settings() -> ClickSettings {
        let fallback = ClickSettings::desktop();
        let seconds = NSEvent::doubleClickInterval();
        match Duration::try_from_secs_f64(seconds) {
            Ok(interval) if !interval.is_zero() => fallback.with_interval(interval),
            _ => fallback,
        }
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::time::Duration;

    use zgui_geom::CssPx;
    use zgui_platform::ClickSettings;

    pub(super) fn settings() -> ClickSettings {
        ClickSettings::desktop()
            .with_interval(Duration::from_millis(400))
            .with_slop(CssPx(5.0))
    }
}
