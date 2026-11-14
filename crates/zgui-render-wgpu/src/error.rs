//! Errors of this crate.

/// Why [`Gpu::open`](crate::Gpu::open) could not open a device on an adapter.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
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
