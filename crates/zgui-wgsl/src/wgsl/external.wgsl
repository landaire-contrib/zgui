// A rectangle showing a texture the renderer did not draw: a video frame, a screen capture.
//
// There is one of these per source in a frame rather than thousands, so it binds its own texture
// and its own block instead of being instanced out of a buffer. What it shares with everything
// else is the clip: the same chain, evaluated by the same function, so a video inside a rounded,
// scrolled card is clipped exactly as the card's own background is.

struct ExternalParams {
    // The quad, in device pixels: origin then extent.
    bounds: vec4<f32>,
    // x: the clip chain; y: the transform; z: the multiplier on the texture's alpha;
    // w: flags. Bit 0: the texture's colours are already scaled by its alpha. Bit 1: the
    // sampler decodes sRGB, so samples are linear.
    control: vec4<f32>,
}

@group(1) @binding(0) var<uniform> foreign: ExternalParams;
@group(1) @binding(1) var external_texture: texture_2d<f32>;
@group(1) @binding(2) var external_sampler: sampler;

const EXTERNAL_PREMULTIPLIED: u32 = 1u;
const EXTERNAL_DECODED_SRGB: u32 = 2u;

fn external_encode_srgb_channel(value: f32) -> f32 {
    if value <= 0.0031308 {
        return value * 12.92;
    }
    return 1.055 * pow(value, 1.0 / 2.4) - 0.055;
}

struct ExternalVarying {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_external(@builtin(vertex_index) vertex: u32) -> ExternalVarying {
    let corner = unit_corner(vertex);
    let device = foreign.bounds.xy + corner * foreign.bounds.zw;
    var out: ExternalVarying;
    out.position = to_clip_position(device, u32(foreign.control.y));
    out.uv = corner;
    return out;
}

@fragment
fn fs_external(in: ExternalVarying) -> @location(0) vec4<f32> {
    // The clip chain is in device space, so it is evaluated at the real pixel.
    let coverage = clip_coverage(device_position(in.position.xy), u32(foreign.control.x));
    if coverage <= 0.0 {
        return vec4<f32>(0.0);
    }
    var texel = textureSampleLevel(external_texture, external_sampler, in.uv, 0.0);
    let flags = u32(foreign.control.w);
    let premultiplied = (flags & EXTERNAL_PREMULTIPLIED) != 0u;
    if (flags & EXTERNAL_DECODED_SRGB) != 0u {
        // The frame blends gamma-encoded values, and an sRGB view hands back linear ones. The
        // encoding applies to the straight colour, so a premultiplied texel is divided first.
        var color = texel.rgb;
        if premultiplied {
            color = select(vec3<f32>(0.0), color / texel.a, texel.a > 0.0);
        }
        color = vec3<f32>(
            external_encode_srgb_channel(color.r),
            external_encode_srgb_channel(color.g),
            external_encode_srgb_channel(color.b),
        );
        if premultiplied {
            color = color * texel.a;
        }
        texel = vec4<f32>(color, texel.a);
    }
    if !premultiplied {
        // Everything composites premultiplied. Blending straight bytes as though they were
        // premultiplied makes a half-covered edge contribute a full-intensity colour, which is the
        // bright fringe seen around anything with a soft edge.
        texel = vec4<f32>(texel.rgb * texel.a, texel.a);
    }
    return texel * coverage * foreign.control.z;
}
