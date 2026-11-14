//! The features and limits an application asks for on top of what the renderer asks for.

use crate::error::OpenError;

/// The features and limits that an application asks for on each device that this crate opens.
///
/// Use this when the application draws with wgpu itself on the device of the interface, for
/// example a 3D view in a `surface` element. A [`SharedGraphics`](crate::SharedGraphics) keeps the
/// request and applies it to each device that it opens, also to the device that it opens after a
/// device loss.
///
/// The renderer asks for its own optional features in addition to the request. Each device
/// gets the limits of its adapter, or the downlevel limits on GL, raised to the required limits.
///
/// The request does not give the features whose names start with `EXPERIMENTAL`, because wgpu
/// enables them only with an unsafe acknowledgement. A device does not get one as an optional
/// feature, and a device that requires one does not open.
///
/// An adapter the request rules out is listed in the [`GpuUnavailable`](zgui_render::GpuUnavailable)
/// that the open fails with, if no other adapter opens. The reason of its entry downcasts to a
/// [`Rejection`](crate::Rejection) whose reason is [`RejectionReason::Open`](crate::RejectionReason::Open)
/// with one of the [`OpenError`] values named below.
///
/// ```
/// use zgui_render_wgpu::DeviceRequest;
/// use zgui_render_wgpu::wgpu;
///
/// let request = DeviceRequest::new()
///     .with_required_features(wgpu::Features::DEPTH_CLIP_CONTROL)
///     .with_optional_features(wgpu::Features::TEXTURE_COMPRESSION_BC);
///
/// assert_eq!(request.required_features(), wgpu::Features::DEPTH_CLIP_CONTROL);
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DeviceRequest {
    /// The features that a device must have. An adapter without one of them is rejected.
    required_features: wgpu::Features,
    /// The features that a device gets where its adapter has them.
    optional_features: wgpu::Features,
    /// The limits that a device must reach. An adapter below one of them is rejected.
    required_limits: Option<wgpu::Limits>,
}

impl DeviceRequest {
    /// A request for nothing beyond what the renderer asks for.
    pub fn new() -> Self {
        Self {
            required_features: wgpu::Features::empty(),
            optional_features: wgpu::Features::empty(),
            required_limits: None,
        }
    }

    /// Adds `features` to the features that a device must have.
    ///
    /// The candidate loop rejects an adapter that does not have all of them, with
    /// [`OpenError::MissingFeatures`], and tries the next adapter. An experimental feature here
    /// rejects each adapter, with [`OpenError::ExperimentalFeatures`].
    pub fn with_required_features(mut self, features: wgpu::Features) -> Self {
        self.required_features |= features;
        self
    }

    /// Adds `features` to the features that a device gets where its adapter has them.
    pub fn with_optional_features(mut self, features: wgpu::Features) -> Self {
        self.optional_features |= features;
        self
    }

    /// Sets the limits that a device must reach.
    ///
    /// The candidate loop rejects an adapter below one of them, with [`OpenError::Limit`], and tries
    /// the next adapter.
    pub fn with_required_limits(mut self, limits: wgpu::Limits) -> Self {
        self.required_limits = Some(limits);
        self
    }

    /// The features that a device must have.
    pub fn required_features(&self) -> wgpu::Features {
        self.required_features
    }

    /// The features that a device gets where its adapter has them.
    pub fn optional_features(&self) -> wgpu::Features {
        self.optional_features
    }

    /// The limits that a device must reach, if the request sets them.
    pub fn required_limits(&self) -> Option<&wgpu::Limits> {
        self.required_limits.as_ref()
    }
}

/// The features and limits to create a device with.
#[derive(Debug, PartialEq)]
pub(crate) struct Resolved {
    /// The features of the device.
    pub(crate) features: wgpu::Features,
    /// The limits of the device.
    pub(crate) limits: wgpu::Limits,
}

/// What an adapter offers, and what the renderer asks of it without a request.
pub(crate) struct Offer<'a> {
    /// The features of the adapter.
    pub(crate) features: wgpu::Features,
    /// The limits of the adapter.
    pub(crate) limits: &'a wgpu::Limits,
    /// The optional features of the renderer.
    pub(crate) renderer_features: wgpu::Features,
    /// The limits the renderer gives a device on this adapter.
    pub(crate) renderer_limits: wgpu::Limits,
}

/// Applies `request` to what an adapter offers.
///
/// # Errors
///
/// Returns [`OpenError::ExperimentalFeatures`] where the request requires an experimental
/// feature, [`OpenError::MissingFeatures`] where the adapter lacks a required feature, and
/// [`OpenError::Limit`] where it is below a required limit.
pub(crate) fn resolve(request: &DeviceRequest, offer: &Offer<'_>) -> Result<Resolved, OpenError> {
    let experimental = wgpu::Features::all_experimental_mask();
    let refused = request.required_features & experimental;
    if !refused.is_empty() {
        return Err(OpenError::ExperimentalFeatures { features: refused });
    }

    let missing = request.required_features - offer.features;
    if !missing.is_empty() {
        return Err(OpenError::MissingFeatures { missing });
    }

    let limits = match &request.required_limits {
        None => offer.renderer_limits.clone(),
        Some(required) => {
            let mut failure = None;
            required.check_limits_with_fail_fn(offer.limits, true, |name, required, allowed| {
                failure = Some(OpenError::Limit {
                    name,
                    required,
                    allowed,
                });
            });
            if let Some(failure) = failure {
                return Err(failure);
            }
            offer
                .renderer_limits
                .clone()
                .or_better_values_from(required)
        }
    };

    // wgpu refuses a device with an experimental feature unless the descriptor carries the unsafe
    // acknowledgement, so the request leaves them out.
    let optional =
        ((offer.renderer_features | request.optional_features) & offer.features) - experimental;
    Ok(Resolved {
        features: optional | request.required_features,
        limits,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An adapter with `features` and the default limits, under a renderer that asks for
    /// dual-source blending.
    fn offer(features: wgpu::Features, limits: &wgpu::Limits) -> Offer<'_> {
        Offer {
            features,
            limits,
            renderer_features: wgpu::Features::DUAL_SOURCE_BLENDING,
            renderer_limits: wgpu::Limits::downlevel_defaults(),
        }
    }

    #[test]
    fn a_missing_required_feature_is_named_in_the_error() {
        let limits = wgpu::Limits::default();
        let request = DeviceRequest::new().with_required_features(
            wgpu::Features::DEPTH_CLIP_CONTROL | wgpu::Features::SHADER_F16,
        );

        let error = resolve(&request, &offer(wgpu::Features::SHADER_F16, &limits))
            .expect_err("an adapter without a required feature is refused");

        let text = error.to_string();
        let OpenError::MissingFeatures { missing } = error else {
            panic!("the wrong error: {text}");
        };
        assert_eq!(missing, wgpu::Features::DEPTH_CLIP_CONTROL);
        assert!(text.contains("DEPTH_CLIP_CONTROL"), "{text}");
    }

    #[test]
    fn a_required_experimental_feature_is_refused_on_each_adapter() {
        let limits = wgpu::Limits::default();
        let request =
            DeviceRequest::new().with_required_features(wgpu::Features::EXPERIMENTAL_RAY_QUERY);

        let error = resolve(
            &request,
            &offer(wgpu::Features::EXPERIMENTAL_RAY_QUERY, &limits),
        )
        .expect_err("an experimental feature is refused where the adapter has it");

        let text = error.to_string();
        let OpenError::ExperimentalFeatures { features } = error else {
            panic!("the wrong error: {text}");
        };
        assert_eq!(features, wgpu::Features::EXPERIMENTAL_RAY_QUERY);
    }

    #[test]
    fn an_optional_experimental_feature_is_left_out() {
        let limits = wgpu::Limits::default();
        let request = DeviceRequest::new().with_optional_features(
            wgpu::Features::EXPERIMENTAL_MESH_SHADER | wgpu::Features::SHADER_F16,
        );
        let adapter = wgpu::Features::EXPERIMENTAL_MESH_SHADER | wgpu::Features::SHADER_F16;

        let resolved = resolve(&request, &offer(adapter, &limits)).expect("nothing is required");

        assert_eq!(resolved.features, wgpu::Features::SHADER_F16);
    }

    #[test]
    fn optional_features_are_intersected_with_the_adapter() {
        let limits = wgpu::Limits::default();
        let request = DeviceRequest::new()
            .with_required_features(wgpu::Features::SHADER_F16)
            .with_optional_features(
                wgpu::Features::TEXTURE_COMPRESSION_BC | wgpu::Features::DEPTH_CLIP_CONTROL,
            );
        let adapter = wgpu::Features::SHADER_F16
            | wgpu::Features::TEXTURE_COMPRESSION_BC
            | wgpu::Features::DUAL_SOURCE_BLENDING;

        let resolved = resolve(&request, &offer(adapter, &limits)).expect("nothing is missing");

        assert_eq!(resolved.features, adapter);
    }

    #[test]
    fn a_limit_the_adapter_does_not_reach_is_named_in_the_error() {
        let adapter = wgpu::Limits::downlevel_defaults();
        let required = wgpu::Limits {
            max_texture_dimension_2d: adapter.max_texture_dimension_2d * 2,
            ..wgpu::Limits::downlevel_defaults()
        };
        let request = DeviceRequest::new().with_required_limits(required.clone());

        let error = resolve(&request, &offer(wgpu::Features::empty(), &adapter))
            .expect_err("an adapter below a required limit is refused");

        let text = error.to_string();
        let OpenError::Limit {
            name,
            required: value,
            allowed,
        } = error
        else {
            panic!("the wrong error: {text}");
        };
        assert_eq!(name, "max_texture_dimension_2d");
        assert_eq!(value, u64::from(required.max_texture_dimension_2d));
        assert_eq!(allowed, u64::from(adapter.max_texture_dimension_2d));
    }

    #[test]
    fn required_limits_raise_the_renderer_limits() {
        let adapter = wgpu::Limits::default();
        let required = wgpu::Limits {
            max_texture_dimension_2d: adapter.max_texture_dimension_2d,
            ..wgpu::Limits::downlevel_webgl2_defaults()
        };
        let request = DeviceRequest::new().with_required_limits(required);
        let offered = offer(wgpu::Features::empty(), &adapter);
        assert!(
            offered.renderer_limits.max_texture_dimension_2d < adapter.max_texture_dimension_2d
        );

        let resolved = resolve(&request, &offered).expect("the adapter reaches every limit");

        assert_eq!(
            resolved.limits.max_texture_dimension_2d,
            adapter.max_texture_dimension_2d
        );
        assert_eq!(
            resolved.limits.max_storage_buffers_per_shader_stage,
            offered.renderer_limits.max_storage_buffers_per_shader_stage,
            "a required limit below the renderer limit lowered it"
        );
    }

    #[test]
    fn an_empty_request_changes_nothing() {
        let limits = wgpu::Limits::default();
        let offered = offer(wgpu::Features::all_webgpu_mask(), &limits);

        let resolved = resolve(&DeviceRequest::new(), &offered).expect("nothing is required");

        assert_eq!(
            resolved.features,
            wgpu::Features::DUAL_SOURCE_BLENDING & offered.features
        );
        assert_eq!(resolved.limits, offered.renderer_limits);
    }
}
