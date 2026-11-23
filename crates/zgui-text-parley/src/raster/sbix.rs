//! Colour bitmaps that Apple stores in `sbix` records swash does not read.
//!
//! Apple Color Emoji on iOS stores each picture as an `emjc` record, and refers to other records
//! with `dupe` and `flip`. Swash reads only `png ` and `dupe`. For a glyph whose record is one of
//! Apple's, this module finds the strike, decodes the picture and scales it to the requested size.
//! It answers nothing for every other glyph, and swash then rasterises that glyph.

use skrifa::raw::TableProvider;
use skrifa::raw::tables::sbix::{GlyphData, Sbix, Strike};
use skrifa::{FontRef, GlyphId, Tag};
use zgui_geom::{DevicePx, Point, Size};
use zgui_text::{GlyphFormat, GlyphImage};

use crate::raster::emjc;

/// The colour picture of `glyph` at `size` pixels per em, when its `sbix` record is one of Apple's.
pub(crate) fn apple_bitmap(data: &[u8], index: u32, glyph: u16, size: f32) -> Option<GlyphImage> {
    /// A record that holds an `emjc` picture.
    const EMJC: Tag = Tag::new(b"emjc");
    /// The widest and tallest image this answers. Swash draws the outline of a larger glyph.
    const MAX_SIDE: u32 = 4096;

    if !size.is_finite() || size <= 0.0 {
        return None;
    }
    let font = FontRef::from_index(data, index).ok()?;
    let sbix = font.sbix().ok()?;
    let glyph = GlyphId::new(u32::from(glyph));
    let strike = nearest_strike(&sbix, glyph, size)?;
    let (record, mirrored) = resolve(&strike, glyph)?;
    if record.graphic_type() != EMJC {
        return None;
    }
    let mut picture = emjc::decode(record.data())?;
    if mirrored {
        picture.mirror();
    }

    let ppem = f32::from(strike.ppem());
    if ppem <= 0.0 {
        return None;
    }

    let upem = f32::from(font.head().ok()?.units_per_em());
    let left = i32::from(record.origin_offset_x());
    let mut top = i32::from(record.origin_offset_y());
    // Apple's records sit on the baseline with no offset; the picture is drawn 100 units lower,
    // as swash does for the `png ` records of the same font.
    if top == 0 && upem > 0.0 {
        top = (-100.0 * ppem / upem).round() as i32;
    }
    top += picture.height as i32;

    let scale = size / ppem;
    let width = (picture.width as f32 * scale) as u32;
    let height = (picture.height as f32 * scale) as u32;
    if width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }
    let bytes = picture.resample(width, height);

    Some(GlyphImage {
        size: Size::new(width, height),
        placement: Point::new(
            DevicePx((left as f32 * scale) as i32 as f32),
            DevicePx((top as f32 * scale) as i32 as f32),
        ),
        format: GlyphFormat::Color,
        bytes,
    })
}

/// The first strike, in the table's order, at least `size` pixels per em that holds `glyph`, or
/// the largest one that holds it. Swash chooses a `png ` strike the same way.
fn nearest_strike<'a>(sbix: &Sbix<'a>, glyph: GlyphId, size: f32) -> Option<Strike<'a>> {
    let mut best: Option<Strike<'a>> = None;
    for strike in sbix.strikes().iter() {
        let Ok(strike) = strike else {
            continue;
        };
        if !matches!(strike.glyph_data(glyph), Ok(Some(_))) {
            continue;
        }
        if f32::from(strike.ppem()) >= size {
            return Some(strike);
        }
        if best.as_ref().is_none_or(|best| strike.ppem() > best.ppem()) {
            best = Some(strike);
        }
    }
    best
}

/// The record that holds the picture of `glyph`, after its references, and whether the picture is
/// mirrored.
fn resolve<'a>(strike: &Strike<'a>, glyph: GlyphId) -> Option<(GlyphData<'a>, bool)> {
    /// A record that refers to the picture of another glyph.
    const DUPE: Tag = Tag::new(b"dupe");
    /// A record that refers to the picture of another glyph, mirrored left to right.
    const FLIP: Tag = Tag::new(b"flip");
    /// The most references followed from one record.
    const MAX_REFERENCES: usize = 2;

    let mut record = strike.glyph_data(glyph).ok()??;
    let mut mirrored = false;
    for _ in 0..MAX_REFERENCES {
        let kind = record.graphic_type();
        if kind != DUPE && kind != FLIP {
            return Some((record, mirrored));
        }
        let target = record.data().first_chunk::<2>()?;
        mirrored ^= kind == FLIP;
        record = strike
            .glyph_data(GlyphId::new(u32::from(u16::from_be_bytes(*target))))
            .ok()??;
    }
    let kind = record.graphic_type();
    (kind != DUPE && kind != FLIP).then_some((record, mirrored))
}
