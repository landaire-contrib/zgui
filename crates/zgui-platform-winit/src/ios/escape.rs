//! The VoiceOver gesture that leaves a surface.
//!
//! VoiceOver asks the focused element to perform an escape, and then each container above it,
//! until one answers that it did. The outermost container of the accessibility tree is the view
//! winit draws into, so the view answers for its window: with an Escape key while an Escape
//! dismisses something, and with no otherwise.

use core::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CString;
use std::sync::{Arc, Weak};

use objc2::encode::{Encode, Encoding};
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{ffi, sel};
use zgui_platform::{Surface, SurfaceId};

use crate::surface::WinitSurface;

thread_local! {
    /// The surface of each view, by the address of the view, removed with the surface.
    static SURFACES: RefCell<HashMap<usize, (SurfaceId, Weak<WinitSurface>)>> =
        RefCell::new(HashMap::new());
}

/// Answers `accessibilityPerformEscape` for the view at `view`.
extern "C-unwind" fn perform_escape(view: &AnyObject, _selector: Sel) -> Bool {
    let address = core::ptr::from_ref(view).addr();
    let surface = SURFACES.with_borrow(|surfaces| {
        surfaces
            .get(&address)
            .and_then(|(_, surface)| surface.upgrade())
    });
    Bool::new(surface.is_some_and(|surface| surface.escape()))
}

/// Makes the view `surface` draws into answer the escape gesture for it.
pub(crate) fn attach(surface: &Arc<WinitSurface>) {
    let Some(view) = super::ui_view(surface.window()) else {
        return;
    };
    let object: &AnyObject = view.as_ref();
    install(object.class());
    SURFACES.with_borrow_mut(|surfaces| {
        surfaces.insert(
            core::ptr::from_ref(object).addr(),
            (Surface::id(surface.as_ref()), Arc::downgrade(surface)),
        )
    });
}

/// Stops answering the escape gesture for `surface`.
pub(crate) fn detach(surface: SurfaceId) {
    let _ = SURFACES.try_with(|surfaces| {
        surfaces.borrow_mut().retain(|_, (id, _)| *id != surface);
    });
}

/// Adds `accessibilityPerformEscape` to `class`, the class of winit's view.
///
/// The accessibility adapter later makes the view an instance of a subclass, which inherits the
/// method. Installing twice does nothing.
fn install(class: &AnyClass) {
    let types = CString::new(format!(
        "{}{}{}",
        Bool::ENCODING,
        Encoding::Object,
        Encoding::Sel
    ))
    .expect("an encoding has no interior nul");
    let function: extern "C-unwind" fn(&AnyObject, Sel) -> Bool = perform_escape;
    // SAFETY: the function has the signature the selector's type encoding states, and adding a
    // method to a registered class is what `class_addMethod` exists for. A class that already
    // answers the selector itself keeps its own method.
    unsafe {
        let imp: Imp = core::mem::transmute(function);
        ffi::class_addMethod(
            core::ptr::from_ref(class).cast_mut(),
            sel!(accessibilityPerformEscape),
            imp,
            types.as_ptr(),
        );
    }
}
