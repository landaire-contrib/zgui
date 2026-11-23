//! The iPadOS pointer over the elements of a surface.
//!
//! A pointer interaction on the view asks for the area under the pointer as the pointer moves. The
//! runtime gives the surface the element under the hovering pointer after each frame, and the
//! interaction answers from that element. When the element changes, the interaction is told to ask
//! again.
//!
//! # Why a shape and an empty preview
//!
//! zgui draws the full surface into one layer, so no view holds the pixels of an element. The
//! system effects change a view: the highlight effect puts the pointer, as a rounded rectangle,
//! behind the view; the hover effect scales and tints the view; the lift effect scales the view
//! and puts a shadow under it. Each takes its view from a targeted preview.
//!
//! The highlight is the effect a system button uses, and it is the only one of the three that
//! shows correctly with an empty view. Its preview is a transparent view at the area of the
//! element, and its shape is a rounded rectangle with the corner radius of the element. The pointer
//! then changes into that rectangle, with snapping, and the transparent view in front of it shows
//! nothing. A hover or a lift of an empty view scales nothing, and a lift puts a shadow under
//! nothing.

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSNumber, NSObject};
use objc2_ui_kit::{
    UIAxis, UIBezierPath, UIColor, UIInteraction, UIPointerHighlightEffect, UIPointerInteraction,
    UIPointerInteractionDelegate, UIPointerRegion, UIPointerRegionRequest, UIPointerShape,
    UIPointerStyle, UIPreviewParameters, UIPreviewTarget, UITargetedPreview, UIView,
};
use zgui_geom::{Device, DevicePx, Point, Rect};
use zgui_platform::{PointerShape, PointerTarget, Surface, SurfaceId};

use crate::surface::WinitSurface;

thread_local! {
    /// The pointer interaction of each surface, with its delegate, removed with the surface.
    ///
    /// An interaction holds its delegate weakly, so the delegate is kept here.
    static INTERACTIONS: RefCell<HashMap<SurfaceId, Attached>> = RefCell::new(HashMap::new());
}

/// A pointer interaction on the view of a surface.
struct Attached {
    /// The interaction.
    interaction: Retained<UIPointerInteraction>,
    /// The delegate of the interaction.
    _delegate: Retained<Delegate>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and `Delegate` does not implement `Drop`.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ZguiPointerDelegate"]
    #[ivars = Weak<WinitSurface>]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl UIPointerInteractionDelegate for Delegate {
        #[unsafe(method_id(pointerInteraction:regionForRequest:defaultRegion:))]
        fn region_for_request(
            &self,
            _interaction: &UIPointerInteraction,
            request: &UIPointerRegionRequest,
            _default_region: &UIPointerRegion,
        ) -> Option<Retained<UIPointerRegion>> {
            self.region(request.location())
        }

        #[unsafe(method_id(pointerInteraction:styleForRegion:))]
        fn style_for_region(
            &self,
            interaction: &UIPointerInteraction,
            region: &UIPointerRegion,
        ) -> Option<Retained<UIPointerStyle>> {
            self.style(interaction, region)
        }
    }
);

impl Delegate {
    /// A delegate that answers from the pointer target of `surface`.
    fn new(main: MainThreadMarker, surface: Weak<WinitSurface>) -> Retained<Self> {
        let this = Self::alloc(main).set_ivars(surface);
        // SAFETY: `init` is NSObject's designated initializer, and the ivars are set.
        unsafe { msg_send![super(this), init] }
    }

    /// The region of the element under `location`, in the points of the view.
    ///
    /// `None` when no element under `location` changes the pointer.
    fn region(&self, location: CGPoint) -> Option<Retained<UIPointerRegion>> {
        let surface = self.ivars().upgrade()?;
        let target = surface.pointer_target()?;
        let scale = surface.scale();
        let point = Point::new(
            DevicePx((location.x * scale) as f32),
            DevicePx((location.y * scale) as f32),
        );
        if !target.contains(point) {
            return None;
        }

        let identifier = NSNumber::new_u64(target.id.get());
        // SAFETY: a region takes any object as its identifier.
        Some(unsafe {
            UIPointerRegion::regionWithRect_identifier(
                points(target.area, scale),
                Some(ProtocolObject::from_ref(&*identifier)),
                self.mtm(),
            )
        })
    }

    /// The pointer style over `region`.
    ///
    /// `None` when `region` names an element that is no longer under the pointer.
    fn style(
        &self,
        interaction: &UIPointerInteraction,
        region: &UIPointerRegion,
    ) -> Option<Retained<UIPointerStyle>> {
        let surface = self.ivars().upgrade()?;
        let target = surface.pointer_target()?;

        // The element under the pointer can change between the two requests.
        let identifier = NSNumber::new_u64(target.id.get());
        let current = region
            .identifier()
            .is_some_and(|named| named.isEqual(Some(AsRef::<AnyObject>::as_ref(&*identifier))));
        if !current {
            return None;
        }

        let view = interaction.view()?;
        style(&target, &view, surface.scale(), self.mtm())
    }
}

/// Adds a pointer interaction to the view of `surface`.
///
/// Does nothing off the main thread, before iOS 13.4, or when the window has no UIKit view.
pub(crate) fn attach(surface: &Arc<WinitSurface>) {
    let Some(main) = MainThreadMarker::new() else {
        return;
    };
    // The class first appears in iOS 13.4, and a message to a missing class panics.
    if AnyClass::get(c"UIPointerInteraction").is_none() {
        return;
    }
    let Some(view) = super::ui_view(surface.window()) else {
        return;
    };

    let delegate = Delegate::new(main, Arc::downgrade(surface));
    let interaction = UIPointerInteraction::initWithDelegate(
        UIPointerInteraction::alloc(main),
        Some(ProtocolObject::from_ref(&*delegate)),
    );
    view.addInteraction(ProtocolObject::from_ref(&*interaction));
    INTERACTIONS.with(|held| {
        held.borrow_mut().insert(
            Surface::id(surface.as_ref()),
            Attached {
                interaction,
                _delegate: delegate,
            },
        )
    });
}

/// Removes the pointer interaction of `surface` from its view.
pub(crate) fn detach(surface: SurfaceId) {
    let Some(attached) = INTERACTIONS.with(|held| held.borrow_mut().remove(&surface)) else {
        return;
    };

    if let Some(view) = attached.interaction.view() {
        view.removeInteraction(ProtocolObject::from_ref(&*attached.interaction));
    }
}

/// Tells the pointer interaction of `surface` to ask again for the area under the pointer.
///
/// Does nothing off the main thread. The interaction then asks again when the pointer moves.
pub(crate) fn invalidate(surface: SurfaceId) {
    if MainThreadMarker::new().is_none() {
        return;
    }

    INTERACTIONS.with(|held| {
        if let Some(attached) = held.borrow().get(&surface) {
            attached.interaction.invalidate();
        }
    });
}

/// The pointer style over `target`, in a view at `scale` physical pixels to the point.
///
/// `None` for a shape this platform has no pointer for, which keeps the system pointer.
fn style(
    target: &PointerTarget,
    view: &UIView,
    scale: f64,
    main: MainThreadMarker,
) -> Option<Retained<UIPointerStyle>> {
    let area = points(target.area, scale);

    match target.shape {
        PointerShape::Highlight { corner_radius } => {
            let radius = f64::from(corner_radius.0) / scale;
            let bounds = CGRect::new(CGPoint::new(0.0, 0.0), area.size);
            let empty = UIView::initWithFrame(UIView::alloc(main), bounds);
            let parameters = UIPreviewParameters::new(main);
            parameters.setBackgroundColor(Some(&UIColor::clearColor()));
            parameters.setVisiblePath(Some(&UIBezierPath::bezierPathWithRoundedRect_cornerRadius(
                bounds, radius,
            )));
            let center = CGPoint::new(
                area.origin.x + area.size.width / 2.0,
                area.origin.y + area.size.height / 2.0,
            );
            let placed = UIPreviewTarget::initWithContainer_center(
                UIPreviewTarget::alloc(main),
                view,
                center,
            );
            let preview = UITargetedPreview::initWithView_parameters_target(
                UITargetedPreview::alloc(main),
                &empty,
                &parameters,
                &placed,
            );
            let effect = UIPointerHighlightEffect::effectWithPreview(&preview);
            let shape = UIPointerShape::shapeWithRoundedRect_cornerRadius(area, radius, main);
            // SAFETY: the shape is in the coordinate space of the container of the preview, which
            // is the view of the interaction.
            Some(unsafe { UIPointerStyle::styleWithEffect_shape(&effect, Some(&shape)) })
        }
        PointerShape::Beam { line_height } => {
            let length = f64::from(line_height.0) / scale;
            let shape =
                UIPointerShape::beamWithPreferredLength_axis(length, UIAxis::Vertical, main);
            Some(UIPointerStyle::styleWithShape_constrainedAxes(
                &shape,
                UIAxis::empty(),
            ))
        }
        _ => None,
    }
}

/// `area`, in the points of a view at `scale` physical pixels to the point.
fn points(area: Rect<DevicePx, Device>, scale: f64) -> CGRect {
    let point = |pixels: DevicePx| f64::from(pixels.0) / scale;

    CGRect::new(
        CGPoint::new(point(area.origin.x), point(area.origin.y)),
        CGSize::new(point(area.size.width), point(area.size.height)),
    )
}
