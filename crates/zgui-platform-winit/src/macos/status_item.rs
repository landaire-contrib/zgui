//! The status item, shown as a menu bar extra.

use std::sync::Arc;

use objc2::rc::Retained;
use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_app_kit::{
    NSBitmapFormat, NSBitmapImageRep, NSCellImagePosition, NSDeviceRGBColorSpace, NSImage, NSMenu,
    NSStatusBar, NSStatusItem, NSVariableStatusItemLength,
};
use objc2_foundation::{NSSize, NSString};
use zgui_platform::{MenuEntry, StatusIcon, StatusItem, StatusPress, Waker};

use crate::macos::status_press::Presses;

/// The height a picture is drawn at in the menu bar, in points.
const ICON_HEIGHT: f64 = 18.0;

/// The status item on show, kept alive for as long as it is.
#[derive(Default)]
pub(crate) struct StatusItems {
    /// The item and the muda menu that owns its AppKit menu.
    current: Option<Shown>,
}

/// An item in the menu bar and the menu it opens.
struct Shown {
    /// The AppKit item.
    item: Retained<NSStatusItem>,
    /// The entries the menu was built from.
    entries: Vec<MenuEntry>,
    /// What a primary click on the item does.
    press: StatusPress,
    /// The muda menu behind the item's menu, kept so its items keep reporting picks.
    _menu: muda::Menu,
    /// What reports the item's presses, when it reports them.
    _presses: Option<Presses>,
}

impl core::fmt::Debug for StatusItems {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("StatusItems")
            .field("installed", &self.current.is_some())
            .finish()
    }
}

/// Shows `item` in the menu bar, or removes the item shown when `item` is absent.
///
/// An item already on show is changed in place, so it keeps its position in the menu bar. Its
/// menu is built again only when the entries or what a click does change, because a new menu
/// closes the old one while the user has it open. A press the item reports goes to `waker`.
pub(crate) fn install(items: &mut StatusItems, item: Option<&StatusItem>, waker: Arc<dyn Waker>) {
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    let Some(item) = item else {
        if let Some(shown) = items.current.take() {
            NSStatusBar::systemStatusBar().removeStatusItem(&shown.item);
        }
        return;
    };

    let shown = match items.current.take() {
        Some(shown) if shown.entries == item.menu && shown.press == item.press => shown,
        Some(shown) => attach(shown.item, item, waker, main),
        None => {
            let status =
                NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
            attach(status, item, waker, main)
        }
    };

    if let Some(button) = shown.item.button(main) {
        let image = item.icon.as_ref().and_then(image);
        button.setImage(image.as_deref());
        button.setImagePosition(NSCellImagePosition::ImageLeft);
        let title = item.title.as_ref().map_or("", |title| title.as_str());
        button.setTitle(&NSString::from_str(title));
        let tooltip = item
            .tooltip
            .as_ref()
            .map(|tooltip| NSString::from_str(tooltip.as_str()));
        button.setToolTip(tooltip.as_deref());
    }

    items.current = Some(shown);
}

/// Builds a menu holding the entries of `item` and gives it to `status`, to open as the item's
/// press says.
fn attach(
    status: Retained<NSStatusItem>,
    item: &StatusItem,
    waker: Arc<dyn Waker>,
    main: MainThreadMarker,
) -> Shown {
    let menu = crate::macos::menu::build_menu(&item.menu);
    // SAFETY: muda's pointer is to the NSMenu the muda menu owns, which is alive for this call.
    // Retaining it keeps it alive for as long as the status item or the press target holds it.
    let ns_menu: Option<Retained<NSMenu>> =
        unsafe { Retained::retain(muda::ContextMenu::ns_menu(&menu).cast()) };

    let presses = match item.press {
        StatusPress::Reports => {
            let ns_menu = ns_menu.filter(|_| !item.menu.is_empty());
            Some(crate::macos::status_press::report(
                &status, ns_menu, waker, main,
            ))
        }
        // Every other press opens the menu. The target of an item that reported its presses goes
        // with the `Presses` it was kept by.
        _ => {
            status.setMenu(ns_menu.as_deref());
            None
        }
    };

    Shown {
        item: status,
        entries: item.menu.clone(),
        press: item.press,
        _menu: menu,
        _presses: presses,
    }
}

/// The AppKit picture `icon` stands for, drawn [`ICON_HEIGHT`] points tall.
fn image(icon: &StatusIcon) -> Option<Retained<NSImage>> {
    let width = isize::try_from(icon.image.width()).ok()?;
    let height = isize::try_from(icon.image.height()).ok()?;
    if width == 0 || height == 0 {
        return None;
    }

    // SAFETY: a null plane pointer asks AppKit to allocate the pixels itself, and the colour space
    // name is AppKit's own constant.
    let rep = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bitmapFormat_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            core::ptr::null_mut(),
            width,
            height,
            8,
            4,
            true,
            false,
            NSDeviceRGBColorSpace,
            NSBitmapFormat::AlphaNonpremultiplied,
            0,
            32,
        )
    }?;

    let row = usize::try_from(width).ok()? * 4;
    let stride = usize::try_from(rep.bytesPerRow()).ok()?;
    let pixels = rep.bitmapData();
    if pixels.is_null() || stride < row {
        return None;
    }
    for (index, source) in icon.image.rgba().chunks_exact(row).enumerate() {
        // SAFETY: the bitmap holds `height` rows of `stride` bytes, `index` is below `height`
        // because the source holds exactly `height` rows, and each row copies `row` bytes, which
        // is no more than `stride`.
        unsafe {
            core::ptr::copy_nonoverlapping(source.as_ptr(), pixels.add(index * stride), row);
        }
    }

    let size = NSSize::new(ICON_HEIGHT * width as f64 / height as f64, ICON_HEIGHT);
    rep.setSize(size);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    image.addRepresentation(&rep);
    image.setTemplate(icon.template);
    Some(image)
}
