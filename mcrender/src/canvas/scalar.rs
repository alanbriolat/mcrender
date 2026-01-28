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

/// Multiply RGBA by RGB and overlay onto RGBA, ignoring destination alpha channel.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. Scalar version of this function always
/// processes `dst_pixels.len()` pixels, returning that number.
#[inline]
pub fn rgba8_multiply_overlay_final(dst_pixels: &mut [Rgba<u8>], multiply: &Rgb<u8>, src_pixels: &[Rgba<u8>]) -> usize {
    for (dst, src) in dst_pixels.iter_mut().zip(src_pixels.iter()) {
        let fg_r = u16_div_by_255(src[0] as u16 * multiply[0] as u16) as u8;
        let fg_g = u16_div_by_255(src[1] as u16 * multiply[1] as u16) as u8;
        let fg_b = u16_div_by_255(src[2] as u16 * multiply[2] as u16) as u8;
        let fg_a = src[3];
        (dst[0], dst[1], dst[2]) = blend_final_pixel_u8((dst[0], dst[1], dst[2]), (fg_r, fg_g, fg_b), fg_a);
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

#[inline]
pub fn blend_final_pixel_u8(
    (bg_r, bg_g, bg_b): (u8, u8, u8),
    (fg_r, fg_g, fg_b): (u8, u8, u8),
    fg_a: u8,
) -> (u8, u8, u8) {
    // Zero alpha = keep original pixel
    if fg_a == 0 {
        return (bg_r, bg_g, bg_b);
    }
    // Max alpha = overwrite with new pixel
    if fg_a == 255 {
        return (fg_r, fg_g, fg_b);
    }
    // Otherwise, actually blend, using only integers

    // Upcast to u16
    let (bg_r, bg_g, bg_b) = (bg_r as u16, bg_g as u16, bg_b as u16);
    let (fg_r, fg_g, fg_b, fg_a) = (fg_r as u16, fg_g as u16, fg_b as u16, fg_a as u16);
    // src_rgb * src_a
    let (fg_r, fg_g, fg_b) = (fg_r * fg_a, fg_g * fg_a, fg_b * fg_a);
    // dst_rgb * (255 - src_a)
    let fg_a_inv = 255 - fg_a;
    let (bg_r, bg_g, bg_b) = (bg_r * fg_a_inv, bg_g * fg_a_inv, bg_b * fg_a_inv);
    // out_rgb * 255 = src_rgb * src_a + dst_rgb * (255 - src_a)
    let (r, g, b) = (fg_r + bg_r, fg_g + bg_g, fg_b + bg_b);
    // Divide by final alpha using fast integer divide-by-255 trick
    let (r, g, b) = (
        (r + ((r + 257) >> 8)) >> 8,
        (g + ((g + 257) >> 8)) >> 8,
        (b + ((b + 257) >> 8)) >> 8,
    );
    (r as u8, g as u8, b as u8)
}
