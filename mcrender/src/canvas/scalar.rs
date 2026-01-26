use crate::canvas::overlay::Overlay;
use crate::canvas::{Rgb, Rgba};

/// Overlay RGBA onto RGBA, fully blended including blended alpha.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. Returns the number of pixels
/// processed, which for this implementation should be equal to `dst_pixels.len()`, since it
/// processes one pixel at a time.
#[inline]
pub fn rgba8_as_rgba32f_overlay(dst_pixels: &mut [Rgba<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    for (dst, src) in dst_pixels.iter_mut().zip(src_pixels.iter()) {
        dst.overlay(src);
    }
    dst_pixels.len()
}

/// Overlay RGBA onto RGBA, ignoring destination alpha channel.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. Scalar version of this function always
/// processes `dst_pixels.len()` pixels, returning that number.
#[inline]
pub fn rgba8_overlay_final(dst_pixels: &mut [Rgba<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    for (dst, src) in dst_pixels.iter_mut().zip(src_pixels.iter()) {
        dst.overlay_final(src);
    }
    dst_pixels.len()
}

/// Overlay RGBA onto RGB.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. Scalar version of this function always
/// processes `dst_pixels.len()` pixels, returning that number.
#[inline]
pub fn rgba8_onto_rgb8_overlay(dst_pixels: &mut [Rgb<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    for (dst, src) in dst_pixels.iter_mut().zip(src_pixels.iter()) {
        dst.overlay(src);
    }
    dst_pixels.len()
}

#[inline(always)]
pub fn u16_div_by_255(a: u16) -> u16 {
    (a + ((a + 257) >> 8)) >> 8
}
