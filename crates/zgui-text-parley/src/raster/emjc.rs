//! Apple's `emjc` pictures, the records of Apple Color Emoji on iOS.
//!
//! A record is a 16-byte header and an LZFSE stream. The stream holds the alpha plane, one
//! prediction filter for each row, and the colour residuals in a reversible luma and chroma
//! transform. An appendix widens the range of some residuals. The layout follows
//! <https://github.com/PoomSmart/EmojiFonts/blob/main/EMJC.md>.

/// A decoded picture: rows of straight-alpha RGBA pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Picture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    rgba: Vec<u8>,
}

impl Picture {
    /// Mirrors the picture left to right.
    pub(crate) fn mirror(&mut self) {
        for row in self.rgba.chunks_exact_mut(self.width as usize * 4) {
            let pixels = row.len() / 4;
            for x in 0..pixels / 2 {
                let (left, right) = row.split_at_mut((pixels - 1 - x) * 4);
                left[x * 4..x * 4 + 4].swap_with_slice(&mut right[..4]);
            }
        }
    }

    /// The picture at `width` by `height` pixels. Each pixel of the result averages the area of
    /// the picture it covers, weighted by alpha, so the edges of a reduced picture keep their
    /// colour.
    pub(crate) fn resample(&self, width: u32, height: u32) -> Vec<u8> {
        if (width, height) == (self.width, self.height) {
            return self.rgba.clone();
        }
        let premultiplied: Vec<[f32; 4]> = self
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .map(|pixel| {
                let alpha = f32::from(pixel[3]) / 255.0;
                [
                    f32::from(pixel[0]) * alpha,
                    f32::from(pixel[1]) * alpha,
                    f32::from(pixel[2]) * alpha,
                    f32::from(pixel[3]),
                ]
            })
            .collect();

        let columns = spans(self.width, width);
        let rows = spans(self.height, height);
        let mut across = vec![[0.0f32; 4]; width as usize * self.height as usize];
        for y in 0..self.height as usize {
            for (x, span) in columns.iter().enumerate() {
                across[y * width as usize + x] = average(span, |source| {
                    premultiplied[y * self.width as usize + source]
                });
            }
        }

        let mut out = Vec::with_capacity(width as usize * height as usize * 4);
        for span in &rows {
            for x in 0..width as usize {
                let [red, green, blue, alpha] =
                    average(span, |source| across[source * width as usize + x]);
                let unmultiply = |channel: f32| {
                    if alpha > 0.0 {
                        (channel * 255.0 / alpha).round().clamp(0.0, 255.0) as u8
                    } else {
                        0
                    }
                };
                out.extend_from_slice(&[
                    unmultiply(red),
                    unmultiply(green),
                    unmultiply(blue),
                    alpha.round().clamp(0.0, 255.0) as u8,
                ]);
            }
        }
        out
    }
}

/// Decodes one `emjc` record. `None` for a record that is malformed or larger than any emoji.
pub(crate) fn decode(record: &[u8]) -> Option<Picture> {
    /// Four times the largest strike of Apple Color Emoji, 160 pixels per em.
    const MAX_SIDE: usize = 640;

    let header = record.first_chunk::<16>()?;
    if &header[..4] != b"emj1" {
        return None;
    }
    let width = usize::from(u16::from_le_bytes([header[8], header[9]]));
    let height = usize::from(u16::from_le_bytes([header[10], header[11]]));
    let appendix_length = usize::from(u16::from_le_bytes([header[12], header[13]]));
    if width == 0 || height == 0 || width > MAX_SIDE || height > MAX_SIDE {
        return None;
    }

    let pixels = width * height;
    let components = pixels * 3;
    let payload = lzfse::decode(
        &record[16..],
        pixels + height + components + appendix_length,
    )?;
    let (alpha, rest) = payload.split_at(pixels);
    let (filters, rest) = rest.split_at(height);
    let (residuals, appendix) = rest.split_at(components);

    // The appendix sets the offset of the residuals it names; every other offset is zero.
    let mut values = vec![0i32; components];
    let mut position = 0;
    for &entry in appendix {
        position += usize::from(entry / 4);
        if position >= components {
            break;
        }
        values[position] = 128 * i32::from(entry % 4);
        position += 1;
    }

    let mut rgba = vec![0u8; pixels * 4];
    for (y, &filter) in filters.iter().enumerate() {
        if filter > 4 {
            return None;
        }
        for x in 0..width {
            let pixel = y * width + x;
            for component in pixel * 3..pixel * 3 + 3 {
                values[component] = difference(residuals[component], values[component]);
            }

            let left = (x > 0).then(|| pixel - 1);
            let up = (y > 0).then(|| pixel - width);
            let neighbour = |source: usize| {
                [
                    values[source * 3],
                    values[source * 3 + 1],
                    values[source * 3 + 2],
                ]
            };
            let predicted = match (filter, left, up) {
                (1, Some(left), Some(up)) => {
                    let corner = values[(pixel - width - 1) * 3];
                    let from_up =
                        (values[left * 3] - corner).abs() < (values[up * 3] - corner).abs();
                    Some(neighbour(if from_up { up } else { left }))
                }
                (4, Some(left), Some(up)) => {
                    let (left, up) = (neighbour(left), neighbour(up));
                    Some([0, 1, 2].map(|channel| (left[channel] + up[channel] + 1) / 2))
                }
                (1 | 2 | 4, Some(left), _) => Some(neighbour(left)),
                (1 | 3 | 4, None, Some(up)) | (3, Some(_), Some(up)) => Some(neighbour(up)),
                _ => None,
            };
            if let Some(predicted) = predicted {
                for (channel, value) in predicted.into_iter().enumerate() {
                    values[pixel * 3 + channel] += value;
                }
            }

            let [red, green, blue] = to_rgb(
                values[pixel * 3],
                values[pixel * 3 + 1],
                values[pixel * 3 + 2],
            );
            let channel = |value: i32| value.clamp(0, 255) as u8;
            rgba[pixel * 4..pixel * 4 + 4].copy_from_slice(&[
                channel(red),
                channel(green),
                channel(blue),
                alpha[pixel],
            ]);
        }
    }

    Some(Picture {
        width: width as u32,
        height: height as u32,
        rgba,
    })
}

/// A residual from its zigzag byte and the offset the appendix gave its position.
fn difference(encoded: u8, offset: i32) -> i32 {
    let magnitude = i32::from(encoded >> 1);
    if encoded & 1 == 1 {
        -magnitude - offset
    } else {
        magnitude + offset
    }
}

/// Red, green and blue from the luma `base` and the chroma differences `p` and `q`.
fn to_rgb(base: i32, p: i32, q: i32) -> [i32; 3] {
    let half = |value: i32| value.div_euclid(2);
    match (p < 0, q < 0) {
        (true, true) => [
            base + half(p) - half(q + 1),
            base + half(q),
            base - half(p + 1) - half(q + 1),
        ],
        (true, false) => [
            base + half(p) - half(q),
            base + half(q + 1),
            base - half(p + 1) - half(q),
        ],
        (false, true) => [
            base + half(p + 1) - half(q + 1),
            base + half(q),
            base - half(p) - half(q + 1),
        ],
        (false, false) => [
            base + half(p + 1) - half(q),
            base + half(q + 1),
            base - half(p) - half(q),
        ],
    }
}

/// For each of `target` pixels, the pixels of a row or column of `source` it covers and the
/// fraction of each.
fn spans(source: u32, target: u32) -> Vec<Vec<(usize, f32)>> {
    let scale = source as f32 / target as f32;
    (0..target)
        .map(|index| {
            let start = index as f32 * scale;
            let end = (start + scale).min(source as f32);
            let first = start.floor() as usize;
            let last = (end.ceil() as usize).clamp(first + 1, source as usize);
            (first..last)
                .map(|covered| {
                    let overlap = end.min(covered as f32 + 1.0) - start.max(covered as f32);
                    (covered, overlap.max(0.0) / (end - start))
                })
                .collect()
        })
        .collect()
}

/// The weighted mean of the pixels `span` names.
fn average(span: &[(usize, f32)], pixel: impl Fn(usize) -> [f32; 4]) -> [f32; 4] {
    let mut sum = [0.0f32; 4];
    for &(source, weight) in span {
        for (total, value) in sum.iter_mut().zip(pixel(source)) {
            *total += value * weight;
        }
    }
    sum
}

/// LZFSE streams, through Apple's Compression library. Only Apple's systems ship `emjc` pictures,
/// and every one of them has the library.
#[cfg(target_vendor = "apple")]
#[allow(unsafe_code)]
mod lzfse {
    use std::ffi::c_void;

    /// `COMPRESSION_LZFSE`.
    const LZFSE: u32 = 0x801;

    #[link(name = "compression")]
    unsafe extern "C" {
        fn compression_decode_buffer(
            dst_buffer: *mut u8,
            dst_size: usize,
            src_buffer: *const u8,
            src_size: usize,
            scratch_buffer: *mut c_void,
            algorithm: u32,
        ) -> usize;

        #[cfg(test)]
        fn compression_encode_buffer(
            dst_buffer: *mut u8,
            dst_size: usize,
            src_buffer: *const u8,
            src_size: usize,
            scratch_buffer: *mut c_void,
            algorithm: u32,
        ) -> usize;
    }

    /// The bytes of a stream that holds exactly `length` of them.
    pub(super) fn decode(stream: &[u8], length: usize) -> Option<Vec<u8>> {
        // One byte more than expected, so that a longer stream is refused.
        let mut out = vec![0u8; length + 1];
        // SAFETY: both buffers are valid for the lengths given; with no scratch buffer the
        // library allocates its own.
        let written = unsafe {
            compression_decode_buffer(
                out.as_mut_ptr(),
                out.len(),
                stream.as_ptr(),
                stream.len(),
                std::ptr::null_mut(),
                LZFSE,
            )
        };
        if written != length {
            return None;
        }
        out.truncate(length);
        Some(out)
    }

    /// `bytes` as a stream.
    #[cfg(test)]
    pub(super) fn encode(bytes: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; bytes.len() + 1024];
        // SAFETY: both buffers are valid for the lengths given.
        let written = unsafe {
            compression_encode_buffer(
                out.as_mut_ptr(),
                out.len(),
                bytes.as_ptr(),
                bytes.len(),
                std::ptr::null_mut(),
                LZFSE,
            )
        };
        assert_ne!(written, 0, "the bytes compress");
        out.truncate(written);
        out
    }
}

/// Other systems ship no `emjc` pictures.
#[cfg(not(target_vendor = "apple"))]
mod lzfse {
    /// No stream decodes here.
    pub(super) fn decode(_stream: &[u8], _length: usize) -> Option<Vec<u8>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `emjc` record of `width` by `height` pixels holding `payload` before compression.
    #[cfg(target_vendor = "apple")]
    fn record(width: u16, height: u16, appendix_length: u16, payload: &[u8]) -> Vec<u8> {
        let mut out = b"emj1".to_vec();
        for field in [0, 0xa101, width, height, appendix_length, 0] {
            out.extend_from_slice(&field.to_le_bytes());
        }
        out.extend_from_slice(&lzfse::encode(payload));
        out
    }

    /// Grey 100 and (120, 100, 100) in the first row, unfiltered; the second row repeats the
    /// first through the up filter, so its residuals are zero.
    #[test]
    #[cfg(target_vendor = "apple")]
    fn decodes_the_transform_the_zigzag_residuals_and_the_up_filter() {
        let alpha = [255, 128, 255, 0];
        let filters = [0, 3];
        // Grey 100 is base 100, p 0, q 0. (120, 100, 100) is base 105, p 20, q -10.
        let residuals = [200, 0, 0, 210, 40, 21, 0, 0, 0, 0, 0, 0];
        let payload = [&alpha[..], &filters, &residuals].concat();

        let picture = decode(&record(2, 2, 0, &payload)).expect("decoded");

        assert_eq!((picture.width, picture.height), (2, 2));
        assert_eq!(
            picture.rgba,
            [
                100, 100, 100, 255, 120, 100, 100, 128, 100, 100, 100, 255, 120, 100, 100, 0
            ]
        );
    }

    /// Pure red needs p 255, past the 127 a residual byte holds; the appendix gives that position
    /// an offset of 128.
    #[test]
    #[cfg(target_vendor = "apple")]
    fn the_appendix_widens_a_residual() {
        // Red is base 64, p 255, q -127: zigzag 128, 2 * (255 - 128) and 2 * 127 + 1.
        let payload = [255, 0, 128, 254, 255, 0b0000_0101];
        let picture = decode(&record(1, 1, 1, &payload)).expect("decoded");

        assert_eq!(picture.rgba, [255, 0, 0, 255]);
    }

    #[test]
    #[cfg(target_vendor = "apple")]
    fn refuses_a_record_whose_stream_does_not_match_its_header() {
        let payload = [255, 0, 200, 0, 0];
        let mut wrong = record(1, 1, 0, &payload);
        wrong[8] = 2;

        assert_eq!(decode(&wrong), None);
        assert_eq!(decode(b"png \0\0\0\0\0\0\0\0\0\0\0\0"), None);
    }

    #[test]
    fn refuses_a_picture_larger_than_any_emoji() {
        let mut header = *b"emj1\0\0\x01\xa1\0\0\x01\0\0\0\0\0";
        header[8..10].copy_from_slice(&641u16.to_le_bytes());

        assert_eq!(decode(&header), None);
    }

    #[test]
    fn mirrors_and_reduces_with_alpha_weighting() {
        let mut picture = Picture {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 0],
        };
        picture.mirror();
        assert_eq!(picture.rgba, [0, 0, 255, 0, 255, 0, 0, 255]);

        // The transparent blue pixel adds coverage and no colour.
        assert_eq!(picture.resample(1, 1), [255, 0, 0, 128]);
    }
}
