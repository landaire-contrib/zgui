//! What an on-demand renderer is told about the device it draws on, through a real frame loop.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use zgui_geom::{Scale, Size};
use zgui_platform::Surface;
use zgui_platform_headless::Harness;
use zgui_render::RenderTarget;
use zgui_render_wgpu::{Builder, Gpu, WgpuRenderer, wgpu};
use zgui_runtime::{App, AppError, Runtime};
use zgui_view::{BuildCx, IntoView, View};
use zgui_wgpu::{SurfaceElementExt, SurfaceRenderCx, SurfaceRenderer, WgpuSurfaces};

/// What one render saw.
#[derive(Clone, Debug, PartialEq)]
struct Seen {
    /// The device generation.
    generation: u64,
    /// The adapter.
    adapter: wgpu::AdapterInfo,
}

/// Records what it is told and asks to be drawn again every refresh.
struct Recorder {
    /// Every render, in order.
    seen: Arc<Mutex<Vec<Seen>>>,
}

impl SurfaceRenderer for Recorder {
    fn render(&mut self, cx: &mut SurfaceRenderCx<'_>) {
        self.seen.lock().unwrap().push(Seen {
            generation: cx.generation,
            adapter: cx.adapter_info.clone(),
        });
        cx.request_animation_frame();
    }
}

/// A window drawn by a real wgpu renderer into an offscreen texture.
fn app(renderer: WgpuRenderer, seen: Arc<Mutex<Vec<Seen>>>) -> Harness<Runtime> {
    let mut renderer = Some(renderer);
    let mut recorder = Some(Recorder { seen });
    let handler = App::new()
        .with_title("generation")
        .with_size(64.0, 64.0)
        .with_stylesheet("root { display: block; width: 64px; height: 64px } surface { width: 32px; height: 32px }")
        .with_renderer(Box::new(
            move |_surface: &Arc<dyn Surface>, target| -> Result<_, AppError> {
                let mut renderer = renderer.take().expect("one window asks for one renderer");
                zgui_render::Renderer::configure(&mut renderer, target);
                Ok(Box::new(renderer) as Box<dyn zgui_render::Renderer>)
            },
        ))
        .with_text_engine(Box::new(|| {
            Box::new(zgui_layout::Paragraphs::new(
                zgui_testkit_scene::MonoShaper::new(),
            ))
        }))
        .with_glyph_raster(Box::new(|| {
            Arc::new(zgui_testkit_scene::MonoRaster::new())
        }))
        .into_handler(move |cx: &mut BuildCx<'_>| {
            let recorder = recorder.take().expect("the view is built once");
            Box::new(
                zgui_elements::r#box()
                    .class("root")
                    .child(zgui_elements::surface().renderer(recorder))
                    .into_view()
                    .build(cx),
            )
        })
        .expect("the reactive runtime installs");
    let mut harness = Harness::new(handler);
    harness.app_mut().windows_mut()[0].install_embed_host(Box::new(WgpuSurfaces::new()));
    harness
}

/// The adapter of the device the window's renderer draws on now.
fn current_adapter(app: &mut Harness<Runtime>) -> wgpu::AdapterInfo {
    app.app_mut().windows_mut()[0]
        .renderer()
        .as_any()
        .and_then(|any| any.downcast_ref::<WgpuRenderer>())
        .expect("the window draws with wgpu")
        .gpu()
        .adapter()
        .get_info()
}

#[test]
fn a_renderer_sees_the_generation_move_after_the_device_is_lost() {
    let target = RenderTarget::new(Size::new(64, 64), Scale::new(1.0));
    let renderer = match Builder::new().offscreen(target, wgpu::TextureFormat::Bgra8Unorm, false) {
        Ok(renderer) => renderer,
        Err(failure) => {
            eprintln!("skipped: no usable graphics device ({failure})");
            return;
        }
    };
    let first: Arc<Gpu> = Arc::clone(renderer.gpu());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let mut app = app(renderer, Arc::clone(&seen));

    app.run_for(Duration::from_millis(100), Duration::from_millis(16));
    let before = seen
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("the renderer was drawn");
    assert_eq!(before.adapter, current_adapter(&mut app));

    // What a driver delivers on a real loss; the next frame rebuilds the device.
    first
        .loss()
        .report(wgpu::DeviceLostReason::Unknown, "injected by a test");
    app.run_for(Duration::from_millis(100), Duration::from_millis(16));

    let after = seen
        .lock()
        .unwrap()
        .last()
        .cloned()
        .expect("the renderer was drawn");
    assert_ne!(
        after.generation, before.generation,
        "a render after the loss is told it draws on another device"
    );
    // Recovery picks an adapter again, so it is compared with the device in use now.
    assert_eq!(after.adapter, current_adapter(&mut app));
}
