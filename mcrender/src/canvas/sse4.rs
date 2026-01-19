#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::canvas::private::PrivateToken;
use crate::canvas::{Rgb, Rgba, TransmutablePixel};

/// Overlay RGBA onto RGBA, ignoring destination alpha channel.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. SSE4-accelerated implementation
/// processes a multiple of 4 pixels, returning the number of pixels processed. Caller should
/// process remaining pixels using [`crate::canvas::scalar::rgba8_overlay_final()`].
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse4.2")]
#[inline]
pub fn rgba8_overlay_final(dst_pixels: &mut [Rgba<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    #[rustfmt::skip]
    let alpha_shuffle = _mm_set_epi8(
        15, 15, 15, 15,
        11, 11, 11, 11,
        7, 7, 7, 7,
        3, 3, 3, 3,
    );
    let alpha_mask = _mm_set1_epi32(0xFF000000u32 as i32);
    let zero = _mm_setzero_si128();

    let mut count = 0;
    // Process in chunks of 4 pixels (4 pixels * 4 channels of u8 = 16 bytes = 128 bits)
    for i in (0..dst_pixels.len()).step_by(4) {
        // Read chunk of both buffers
        let dst = unsafe { _mm_loadu_si128(dst_pixels[i..].as_ptr().cast()) };
        let src = unsafe { _mm_loadu_si128(src_pixels[i..].as_ptr().cast()) };
        // Duplicate src_a to all channels
        let src_a = _mm_shuffle_epi8(src, alpha_shuffle);
        // Process low and high halves upcast from u8 to u16
        let out_lo = u16x16_rgba_overlay_final(
            _mm_unpacklo_epi8(dst, zero),
            _mm_unpacklo_epi8(src, zero),
            _mm_unpacklo_epi8(src_a, zero),
        );
        let out_hi = u16x16_rgba_overlay_final(
            _mm_unpackhi_epi8(dst, zero),
            _mm_unpackhi_epi8(src, zero),
            _mm_unpackhi_epi8(src_a, zero),
        );
        // Recombine and results into a single vector
        let out = _mm_packus_epi16(out_lo, out_hi);
        // Restore dst_a value
        let out = _mm_or_si128(
            _mm_and_si128(alpha_mask, dst),
            _mm_andnot_si128(alpha_mask, out),
        );
        unsafe {
            _mm_storeu_si128(dst_pixels[i..].as_mut_ptr().cast(), out);
        }
        count += 4;
    }

    count
}

/// Overlay RGBA onto RGB.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. SSE4-accelerated implementation
/// processes a multiple of 4 pixels, returning the number of pixels processed. Caller should
/// process remaining pixels using [`crate::canvas::scalar::rgba8_onto_rgb8_overlay()`].
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse4.2")]
#[inline]
pub fn rgba8_onto_rgb8_overlay(dst_pixels: &mut [Rgb<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    #[rustfmt::skip]
    let pixel_shuffle = _mm_set_epi8(
        -1, 11, 10, 9,
        -1, 8, 7, 6,
        -1, 5, 4, 3,
        -1, 2, 1, 0,
    );
    #[rustfmt::skip]
    let pixel_unshuffle = _mm_set_epi8(
        -1, -1, -1, -1,
        14, 13, 12,
        10, 9, 8,
        6, 5, 4,
        2, 1, 0,
    );
    #[rustfmt::skip]
    let alpha_shuffle = _mm_set_epi8(
        15, 15, 15, 15,
        11, 11, 11, 11,
        7, 7, 7, 7,
        3, 3, 3, 3,
    );
    // let alpha_mask = _mm_set1_epi32(0xFF000000u32 as i32);
    let zero = _mm_setzero_si128();

    let mut count = 0;
    let mut dst_buf = [0u8; 16];
    // Process in chunks of 4 pixels (4 pixels * 4 channels of u8 = 16 bytes = 128 bits)
    for i in (0..dst_pixels.len()).step_by(4) {
        // Load dst chunk into a register-sized buffer
        dst_buf[..12].copy_from_slice(Rgb::<u8>::channels_from_slice(
            PrivateToken,
            &dst_pixels[i..i + 4],
        ));
        let dst = unsafe { _mm_loadu_si128(dst_buf.as_ptr().cast()) };
        // Shuffle RGB to RGBA format
        let dst = _mm_shuffle_epi8(dst, pixel_shuffle);
        // Load src chunk directly
        let src = unsafe { _mm_loadu_si128(src_pixels[i..].as_ptr().cast()) };
        // Duplicate src_a to all channels
        let src_a = _mm_shuffle_epi8(src, alpha_shuffle);
        // Process low and high halves upcast from u8 to u16
        let out_lo = u16x16_rgba_overlay_final(
            _mm_unpacklo_epi8(dst, zero),
            _mm_unpacklo_epi8(src, zero),
            _mm_unpacklo_epi8(src_a, zero),
        );
        let out_hi = u16x16_rgba_overlay_final(
            _mm_unpackhi_epi8(dst, zero),
            _mm_unpackhi_epi8(src, zero),
            _mm_unpackhi_epi8(src_a, zero),
        );
        // Recombine and results into a single vector
        let out = _mm_packus_epi16(out_lo, out_hi);
        // Unshuffle pixels
        let out = _mm_shuffle_epi8(out, pixel_unshuffle);
        unsafe {
            _mm_storeu_si128(dst_buf.as_mut_ptr().cast(), out);
        }
        Rgb::<u8>::channels_from_slice_mut(PrivateToken, &mut dst_pixels[i..i + 4])
            .copy_from_slice(&dst_buf[..12]);
        count += 4;
    }

    count
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse4.2")]
#[inline]
fn u16x16_rgba_overlay_final(dst: __m128i, src: __m128i, alpha: __m128i) -> __m128i {
    // src_rgb * src_a
    let src = _mm_mullo_epi16(src, alpha);
    // dst_rgb * (255 - src_a)
    let dst = _mm_mullo_epi16(dst, _mm_subs_epu16(_mm_set1_epi16(255), alpha));
    // (out * 255) = (src_rgb * src_a) + (dst_rgb * (255 - src_a))
    let out = _mm_adds_epu16(src, dst);
    // "Un-premultiply" the color channels by dividing by 255
    u16x16_div_by_255(out)
}

#[rustfmt::skip]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "sse4.2")]
#[inline]
fn u16x16_div_by_255(a: __m128i) -> __m128i {
    _mm_srli_epi16(
        _mm_adds_epu16(
            a,
            _mm_srli_epi16(
                _mm_adds_epu16(
                    a,
                    _mm_set1_epi16(0x0101),
                ),
                8,
            ),
        ),
        8,
    )
}
