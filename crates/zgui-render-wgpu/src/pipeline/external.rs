//! What one rectangle showing a texture the renderer did not draw is told.

use bytemuck::{Pod, Zeroable};
use zgui_render::ExternalTexture;
use zgui_scene::ExternalQuad;

/// What a sample of an external texture returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalSampling {
    /// The stored values, which are already in the gamma space the frame blends in.
    Stored,
    /// Linear values, because the format is sRGB and the sampler decodes it.
    DecodedSrgb,
}

impl ExternalSampling {
    /// How a texture of `format` samples through a view of the same format.
    pub fn of(format: wgpu::TextureFormat) -> Self {
        if format.is_srgb() {
            Self::DecodedSrgb
        } else {
            Self::Stored
        }
    }
}

/// An attached external texture as the frame plan sees it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExternalSource {
    /// What was registered for it.
    pub texture: ExternalTexture,
    /// What its view returns when sampled.
    pub sampling: ExternalSampling,
}

/// The `control.w` bit set when the texture is already premultiplied.
pub const PREMULTIPLIED: u32 = 1;
/// The `control.w` bit set when samples are linear and must be encoded back to sRGB.
pub const DECODED_SRGB: u32 = 2;

/// The block one external quad reads.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct ExternalParams {
    /// The quad, in device pixels: origin then extent.
    pub bounds: [f32; 4],
    /// The clip chain, the transform, the opacity, and the [`PREMULTIPLIED`] and
    /// [`DECODED_SRGB`] bits.
    pub control: [f32; 4],
}

impl ExternalParams {
    /// The block for `quad` showing `source`.
    pub fn of(quad: &ExternalQuad, source: &ExternalSource) -> Self {
        let mut flags = 0;
        if source.texture.premultiplied {
            flags |= PREMULTIPLIED;
        }
        if source.sampling == ExternalSampling::DecodedSrgb {
            flags |= DECODED_SRGB;
        }
        Self {
            bounds: [
                quad.bounds.origin.x.0,
                quad.bounds.origin.y.0,
                quad.bounds.size.width.0,
                quad.bounds.size.height.0,
            ],
            control: [
                quad.clip.0 as f32,
                quad.transform.index() as f32,
                quad.opacity.clamp(0.0, 1.0),
                flags as f32,
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DECODED_SRGB, ExternalParams, ExternalSampling, ExternalSource, PREMULTIPLIED};
    use zgui_geom::{Device, DevicePx, Point, Rect, Size};
    use zgui_render::{ExternalTexture, TextureHandle};
    use zgui_scene::{ExternalQuad, ExternalTextureId};

    fn source(premultiplied: bool, sampling: ExternalSampling) -> ExternalSource {
        ExternalSource {
            texture: ExternalTexture {
                id: ExternalTextureId(1),
                handle: TextureHandle(1),
                size: Size::new(8, 8),
                premultiplied,
            },
            sampling,
        }
    }

    fn flags(source: &ExternalSource) -> u32 {
        let bounds: Rect<DevicePx, Device> = Rect::new(
            Point::new(DevicePx(0.0), DevicePx(0.0)),
            Size::new(DevicePx(8.0), DevicePx(8.0)),
        );
        let quad = ExternalQuad::new(bounds, ExternalTextureId(1));
        ExternalParams::of(&quad, source).control[3] as u32
    }

    #[test]
    fn a_texture_that_is_not_premultiplied_says_so_where_the_shader_can_see_it() {
        assert_eq!(flags(&source(false, ExternalSampling::Stored)), 0);
        assert_eq!(
            flags(&source(true, ExternalSampling::Stored)),
            PREMULTIPLIED
        );
    }

    #[test]
    fn an_srgb_texture_asks_the_shader_to_encode_its_samples() {
        assert_eq!(
            flags(&source(true, ExternalSampling::DecodedSrgb)),
            PREMULTIPLIED | DECODED_SRGB
        );
        assert_eq!(
            ExternalSampling::of(wgpu::TextureFormat::Rgba8UnormSrgb),
            ExternalSampling::DecodedSrgb
        );
        assert_eq!(
            ExternalSampling::of(wgpu::TextureFormat::Rgba8Unorm),
            ExternalSampling::Stored
        );
    }
}
