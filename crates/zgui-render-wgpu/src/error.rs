//! Errors of this crate.

/// Why [`Gpu::open`](crate::Gpu::open) could not open a device on an adapter.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// The [`DeviceRequest`](crate::DeviceRequest) requires experimental features, which the
    /// request does not give.
    #[error("the request requires the experimental features {features:?}")]
    ExperimentalFeatures {
        /// The experimental features that the request requires.
        features: wgpu::Features,
    },
    /// The adapter does not have every feature that the
    /// [`DeviceRequest`](crate::DeviceRequest) requires.
    #[error("the adapter does not have the required features {missing:?}")]
    MissingFeatures {
        /// The required features that the adapter does not have.
        missing: wgpu::Features,
    },
    /// The adapter does not reach a limit that the [`DeviceRequest`](crate::DeviceRequest)
    /// requires.
    #[error("the adapter limit {name} is {allowed}, and the request requires {required}")]
    Limit {
        /// The name of the limit, as the field of [`wgpu::Limits`] is named.
        name: &'static str,
        /// The value that the request requires.
        required: u64,
        /// The value that the adapter allows.
        allowed: u64,
    },
    /// The driver refused to create the device.
    #[error("the driver refused the device: {0}")]
    Refused(#[from] wgpu::RequestDeviceError),
}

/// Why a device could not present to what the candidate loop proved it against.
#[derive(Debug, thiserror::Error)]
pub enum PresentError {
    /// No surface could be made for the window on the instance of the adapter.
    #[error("no surface could be made for the window: {0}")]
    Surface(#[from] wgpu::CreateSurfaceError),
    /// The surface has no format on this adapter.
    #[error("the surface is not compatible with this adapter")]
    Incompatible,
    /// Making or configuring the presentation raised a validation error.
    #[error("the presentation raised a validation error: {0}")]
    Validation(wgpu::Error),
}

/// What made the candidate loop pass over an adapter.
#[derive(Debug, thiserror::Error)]
pub enum RejectionReason {
    /// No device opened on the adapter.
    #[error(transparent)]
    Open(#[from] OpenError),
    /// The adapter has the Vulkan device extensions asked for, and the driver refused a device
    /// carrying them.
    #[error(
        "the adapter has the Vulkan device extensions asked for and the driver refused a device \
         carrying them"
    )]
    ExtensionsRefused,
    /// A device opened on the adapter and could not present.
    #[error(transparent)]
    Present(#[from] PresentError),
}

/// One adapter that the candidate loop passed over.
///
/// It is the reason of each [`RejectedAdapter`](zgui_render::RejectedAdapter) that the loop
/// records, where `downcast_ref` reaches it.
#[derive(Debug, thiserror::Error)]
#[error("{reason}")]
pub struct Rejection {
    /// The adapter.
    pub adapter: wgpu::AdapterInfo,
    /// Why it was passed over.
    pub reason: RejectionReason,
}
