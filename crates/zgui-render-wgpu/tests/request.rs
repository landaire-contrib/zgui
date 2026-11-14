//! Asking each device for the features and limits that an application draws with, and choosing the
//! adapter that the application prefers.
//!
//! The sort and the resolution of a request are tested without a device in the crate. These tests
//! assert that the values reach real devices, also the device that replaces a lost one.

mod support;

use std::sync::Arc;

use zgui_bits::DamageSet;
use zgui_geom::{Scale, Size};
use zgui_render::{FrameOutcome, RenderTarget, Renderer};
use zgui_render_wgpu::gpu::adapter;
use zgui_render_wgpu::{
    AdapterPreference, DeviceRequest, Gpu, GraphicsOptions, OpenError, SharedGraphics, wgpu,
};
use zgui_scene::Scene;

use support::{SIDE, device_lock};

/// Features that an application can ask for and that the renderer does not ask for.
const FEATURES: [wgpu::Features; 9] = [
    wgpu::Features::DEPTH_CLIP_CONTROL,
    wgpu::Features::TEXTURE_COMPRESSION_BC,
    wgpu::Features::INDIRECT_FIRST_INSTANCE,
    wgpu::Features::SHADER_F16,
    wgpu::Features::RG11B10UFLOAT_RENDERABLE,
    wgpu::Features::BGRA8UNORM_STORAGE,
    wgpu::Features::FLOAT32_FILTERABLE,
    wgpu::Features::TEXTURE_COMPRESSION_ETC2,
    wgpu::Features::TEXTURE_COMPRESSION_ASTC,
];

/// A target the size of every other test's.
fn target() -> RenderTarget {
    RenderTarget::new(Size::new(SIDE, SIDE), Scale::new(1.0))
}

/// The union of [`FEATURES`].
fn every_feature() -> wgpu::Features {
    FEATURES
        .iter()
        .fold(wgpu::Features::empty(), |all, feature| all | *feature)
}

/// The device that graphics with no request open, or `None` where this machine has no device.
fn plain_device(test: &str) -> Option<Arc<Gpu>> {
    match SharedGraphics::new().open_gpu() {
        Ok(gpu) => Some(gpu),
        Err(failure) => {
            eprintln!("{test}: skipped, no usable graphics device ({failure})");
            None
        }
    }
}

/// A frame with nothing in it.
fn empty_scene() -> Scene {
    let mut scene = Scene::new();
    scene.begin_frame(Size::new(SIDE, SIDE));
    scene.finish(&DamageSet::full());
    scene
}

#[test]
fn a_device_that_replaces_a_lost_one_has_the_required_features() {
    let test = "a_device_that_replaces_a_lost_one_has_the_required_features";
    let _device = device_lock();
    let Some(plain) = plain_device(test) else {
        return;
    };
    let Some(extra) = FEATURES.into_iter().find(|feature| {
        plain.adapter().features().contains(*feature)
            && !plain.device().features().contains(*feature)
    }) else {
        eprintln!(
            "{test}: skipped, {} has none of the features this test asks for",
            plain.describe()
        );
        return;
    };
    drop(plain);

    let graphics = SharedGraphics::with_options(GraphicsOptions {
        request: DeviceRequest::new().with_required_features(extra),
        ..GraphicsOptions::default()
    });
    let mut renderer = graphics
        .renderer_offscreen(target(), wgpu::TextureFormat::Bgra8Unorm, false)
        .expect("the adapter that has the feature opens a device with it");
    assert!(renderer.gpu().device().features().contains(extra));

    let dead = Arc::clone(renderer.gpu());
    dead.loss()
        .report(wgpu::DeviceLostReason::Unknown, "reported by the test");
    assert!(matches!(
        renderer.draw(&empty_scene(), &DamageSet::full()),
        FrameOutcome::Recovered
    ));

    assert!(
        !Arc::ptr_eq(renderer.gpu(), &dead),
        "the renderer is still on the device that was lost"
    );
    assert!(
        renderer.gpu().device().features().contains(extra),
        "the replacement device was opened without the required feature {extra:?}"
    );
}

#[test]
fn optional_features_are_the_ones_the_adapter_has() {
    let test = "optional_features_are_the_ones_the_adapter_has";
    let _device = device_lock();
    let graphics = SharedGraphics::with_options(GraphicsOptions {
        request: DeviceRequest::new().with_optional_features(every_feature()),
        ..GraphicsOptions::default()
    });
    let gpu = match graphics.open_gpu() {
        Ok(gpu) => gpu,
        Err(failure) => {
            eprintln!("{test}: skipped, no usable graphics device ({failure})");
            return;
        }
    };

    assert_eq!(
        gpu.device().features() & every_feature(),
        gpu.adapter().features() & every_feature()
    );
}

#[test]
fn a_required_feature_the_adapter_lacks_is_named_in_the_error() {
    let test = "a_required_feature_the_adapter_lacks_is_named_in_the_error";
    let _device = device_lock();
    let backends = adapter::requested_backends();
    let Some(tier) = adapter::tiers(backends).into_iter().next() else {
        eprintln!("{test}: skipped, no backend is enumerated");
        return;
    };
    let instance = wgpu::Instance::new(adapter::instance_descriptor(tier));
    let Some(candidate) = adapter::candidates(&instance, tier, &AdapterPreference::Automatic)
        .into_iter()
        .next()
    else {
        eprintln!("{test}: skipped, no adapter in {tier:?}");
        return;
    };
    let Some(lacking) = FEATURES
        .into_iter()
        .find(|feature| !candidate.features().contains(*feature))
    else {
        eprintln!("{test}: skipped, the adapter has every feature this test asks for");
        return;
    };

    let request = DeviceRequest::new().with_required_features(lacking);
    let error = Gpu::open(instance, candidate, &[], &request)
        .expect_err("an adapter without a required feature opens no device");

    let text = error.to_string();
    let OpenError::MissingFeatures { missing } = error else {
        panic!("the wrong error: {text}");
    };
    assert_eq!(missing, lacking);
}

#[test]
fn a_name_preference_lists_the_named_adapter_first() {
    let test = "a_name_preference_lists_the_named_adapter_first";
    let _device = device_lock();
    // The native tier only, because the GL tier always follows it.
    let backends = adapter::requested_backends() - wgpu::Backends::GL;
    let automatic = adapter::enumerate(backends, &AdapterPreference::Automatic);
    let Some(last) = automatic.last().filter(|_| automatic.len() > 1) else {
        eprintln!("{test}: skipped, fewer than two native adapters");
        return;
    };

    let named = adapter::enumerate(
        backends,
        &AdapterPreference::NameContains(last.name.clone()),
    );

    assert_eq!(named.len(), automatic.len());
    assert!(
        named[0]
            .name
            .to_lowercase()
            .contains(&last.name.to_lowercase()),
        "{} is first, and {} was named",
        named[0].name,
        last.name
    );
}
