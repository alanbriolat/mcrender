#[cfg(target_arch = "x86")]
use std::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use std::arch::x86_64::*;

use crate::canvas::private::PrivateToken;
use crate::canvas::{Rgb, Rgba, TransmutablePixel};

/// Overlay RGBA onto RGBA, ignoring destination alpha channel.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. AVX2-accelerated implementation
/// processes a multiple of 8 pixels, returning the number of pixels processed. Caller should
/// process remaining pixels using [`crate::canvas::scalar::rgba8_overlay_final()`].
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[inline]
pub fn rgba8_overlay_final(dst_pixels: &mut [Rgba<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    #[rustfmt::skip]
    let alpha_shuffle = _mm256_set_epi8(
        15, 15, 15, 15,
        11, 11, 11, 11,
        7, 7, 7, 7,
        3, 3, 3, 3,
        15, 15, 15, 15,
        11, 11, 11, 11,
        7, 7, 7, 7,
        3, 3, 3, 3,
    );
    let alpha_mask = _mm256_set1_epi32(0xFF000000u32 as i32);
    let zero = _mm256_setzero_si256();

    const CHUNK_LEN: usize = 8;
    let mut count = 0;
    // Process in chunks of 8 pixels (8 pixels * 4 channels of u8 = 32 bytes = 256 bits)
    for (dst_chunk, src_chunk) in dst_pixels
        .chunks_mut(CHUNK_LEN)
        .zip(src_pixels.chunks(CHUNK_LEN))
    {
        if dst_chunk.len() < CHUNK_LEN {
            break;
        }
        count += CHUNK_LEN;
        // Read chunk of both buffers
        let dst = unsafe { _mm256_loadu_si256(dst_chunk.as_ptr().cast()) };
        let src = unsafe { _mm256_loadu_si256(src_chunk.as_ptr().cast()) };
        // Duplicate src_a to all channels
        let src_a = _mm256_shuffle_epi8(src, alpha_shuffle);
        // Process low and high halves upcast from u8 to u16
        let out_lo = u16x16_rgba_overlay_final(
            _mm256_unpacklo_epi8(dst, zero),
            _mm256_unpacklo_epi8(src, zero),
            _mm256_unpacklo_epi8(src_a, zero),
        );
        let out_hi = u16x16_rgba_overlay_final(
            _mm256_unpackhi_epi8(dst, zero),
            _mm256_unpackhi_epi8(src, zero),
            _mm256_unpackhi_epi8(src_a, zero),
        );
        // Recombine and results into a single vector
        let out = _mm256_packus_epi16(out_lo, out_hi);
        // Restore dst_a value
        let out = _mm256_or_si256(
            _mm256_and_si256(alpha_mask, dst),
            _mm256_andnot_si256(alpha_mask, out),
        );
        unsafe {
            _mm256_storeu_si256(dst_chunk.as_mut_ptr().cast(), out);
        }
    }

    count
}

/// Overlay RGBA onto RGB.
///
/// Assumes `src_pixels` is at least as long as `dst_pixels`. AVX2-accelerated implementation
/// processes a multiple of 8 pixels, returning the number of pixels processed. Caller should
/// process remaining pixels using [`crate::canvas::scalar::rgba8_onto_rgb8_overlay()`].
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[inline]
pub fn rgba8_onto_rgb8_overlay(dst_pixels: &mut [Rgb<u8>], src_pixels: &[Rgba<u8>]) -> usize {
    #[rustfmt::skip]
    let pixel_shuffle = _mm256_set_epi8(
        // Upper lane: RGBR GBRG BRGB ____ -> RGB_ RGB_ RGB_ RGB_
        -1, 11, 10, 9,
        -1, 8, 7, 6,
        -1, 5, 4, 3,
        -1, 2, 1, 0,
        // Lower lane: ____ RGBR GBRG BRGB -> RGB_ RGB_ RGB_ RGB_
        -1, 15, 14, 13,
        -1, 12, 11, 10,
        -1, 9, 8, 7,
        -1, 6, 5, 4,
    );
    #[rustfmt::skip]
    let pixel_unshuffle = _mm256_set_epi8(
        // Upper lane RGB_ RGB_ RGB_ RGB_ -> RGBR GBRG BRGB ____
        -1, -1, -1, -1,
        14, 13, 12,
        10, 9, 8,
        6, 5, 4,
        2, 1, 0,
        // Lower lane: RGB_ RGB_ RGB_ RGB_ -> ____ RGBR GBRG BRGB
        14, 13, 12,
        10, 9, 8,
        6, 5, 4,
        2, 1, 0,
        -1, -1, -1, -1,
    );
    #[rustfmt::skip]
    let alpha_shuffle = _mm256_set_epi8(
        15, 15, 15, 15,
        11, 11, 11, 11,
        7, 7, 7, 7,
        3, 3, 3, 3,
        15, 15, 15, 15,
        11, 11, 11, 11,
        7, 7, 7, 7,
        3, 3, 3, 3,
    );
    let zero = _mm256_setzero_si256();

    const CHUNK_LEN: usize = 8;
    let mut count = 0;
    let mut dst_buf = [0u8; 32];
    // Process in chunks of 8 pixels (8 pixels * 4 channels of u8 = 32 bytes = 256 bits)
    for (dst_chunk, src_chunk) in dst_pixels
        .chunks_mut(CHUNK_LEN)
        .zip(src_pixels.chunks(CHUNK_LEN))
    {
        if dst_chunk.len() < CHUNK_LEN {
            break;
        }
        count += CHUNK_LEN;
        // Load dst chunk into middle of a register-sized buffer,
        // so we have ____ RGBR GBRG BRGB RGBR GBRG BRGB ____
        dst_buf[4..28].copy_from_slice(Rgb::<u8>::channels_from_slice(PrivateToken, dst_chunk));
        let dst = unsafe { _mm256_loadu_si256(dst_buf.as_ptr().cast()) };
        // Shuffle RGB to RGBA format
        let dst = _mm256_shuffle_epi8(dst, pixel_shuffle);
        // Load src chunk directly
        let src = unsafe { _mm256_loadu_si256(src_chunk.as_ptr().cast()) };
        // Duplicate src_a to all channels
        let src_a = _mm256_shuffle_epi8(src, alpha_shuffle);
        // Process low and high halves upcast from u8 to u16
        let out_lo = u16x16_rgba_overlay_final(
            _mm256_unpacklo_epi8(dst, zero),
            _mm256_unpacklo_epi8(src, zero),
            _mm256_unpacklo_epi8(src_a, zero),
        );
        let out_hi = u16x16_rgba_overlay_final(
            _mm256_unpackhi_epi8(dst, zero),
            _mm256_unpackhi_epi8(src, zero),
            _mm256_unpackhi_epi8(src_a, zero),
        );
        // Recombine and results into a single vector
        let out = _mm256_packus_epi16(out_lo, out_hi);
        // Unshuffle pixels and read back into buffer
        let out = _mm256_shuffle_epi8(out, pixel_unshuffle);
        unsafe {
            _mm256_storeu_si256(dst_buf.as_mut_ptr().cast(), out);
        }
        // Store middle of buffer over the pixels
        Rgb::<u8>::channels_from_slice_mut(PrivateToken, dst_chunk)
            .copy_from_slice(&dst_buf[4..28]);
    }

    count
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[inline]
fn u16x16_rgba_overlay_final(dst: __m256i, src: __m256i, alpha: __m256i) -> __m256i {
    // src_rgb * src_a
    let src = _mm256_mullo_epi16(src, alpha);
    // dst_rgb * (255 - src_a)
    let dst = _mm256_mullo_epi16(dst, _mm256_subs_epu16(_mm256_set1_epi16(255), alpha));
    // (out * 255) = (src_rgb * src_a) + (dst_rgb * (255 - src_a))
    let out = _mm256_adds_epu16(src, dst);
    // "Un-premultiply" the color channels by dividing by 255
    u16x16_div_by_255(out)
}

#[rustfmt::skip]
#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
#[target_feature(enable = "avx2")]
#[inline]
fn u16x16_div_by_255(a: __m256i) -> __m256i {
    _mm256_srli_epi16(
        _mm256_adds_epu16(
            a,
            _mm256_srli_epi16(
                _mm256_adds_epu16(
                    a,
                    _mm256_set1_epi16(0x0101),
                ),
                8,
            ),
        ),
        8,
    )
}
