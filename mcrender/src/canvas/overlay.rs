use std::cmp::min;

use crate::canvas::{Image, ImageMut, Rgb, Rgba, Subpixel, avx2, scalar, sse4};

const DISABLE_AVX2: bool = false;
const DISABLE_SSE4: bool = false;

pub trait Overlay<P: ?Sized> {
    /// Blend `src` onto `self`, in the most correct way that makes sense for the operands.
    fn overlay(&mut self, src: &P);

    /// Blend `src` onto `self`, assuming that `self` is a final opaque layer and not an
    /// intermediate transparent layer. This allows for some optimizations by ignoring the
    /// destination alpha in pixel formats that otherwise would have it.
    #[inline(always)]
    fn overlay_final(&mut self, src: &P) {
        self.overlay(src);
    }
}

impl<T: Subpixel> Overlay<[Rgb<T>]> for [Rgb<T>] {
    /// Overlay RGB onto RGB (no alpha): just copy pixels.
    #[inline(always)]
    fn overlay(&mut self, src: &[Rgb<T>]) {
        self.copy_from_slice(src);
    }
}

impl<T: Subpixel> Overlay<[Rgb<T>]> for [Rgba<T>] {
    /// Overlay RGB onto RGBA: copy as if opaque.
    #[inline(always)]
    fn overlay(&mut self, src: &[Rgb<T>]) {
        assert_eq!(self.len(), src.len());
        // Accelerate this if it becomes something we commonly do...
        for (i, fg) in src.iter().enumerate() {
            self[i].overlay(fg);
        }
    }
}

impl Overlay<[Rgba<u8>]> for [Rgb<u8>] {
    /// Overlay RGBA onto RGB: always fast integer blending with opaque background.
    fn overlay(&mut self, src: &[Rgba<u8>]) {
        assert_eq!(self.len(), src.len());
        let n = if !DISABLE_AVX2 && is_x86_feature_detected!("avx2") {
            unsafe { avx2::rgba8_onto_rgb8_overlay(self, src) }
        } else if !DISABLE_SSE4 && is_x86_feature_detected!("sse4.2") {
            unsafe { sse4::rgba8_onto_rgb8_overlay(self, src) }
        } else {
            0
        };
        // Process any remainder that couldn't be vectorized
        if n < self.len() {
            scalar::rgba8_onto_rgb8_overlay(&mut self[n..], &src[n..]);
        }
    }
}

impl Overlay<[Rgba<u8>]> for [Rgba<u8>] {
    /// Overlay RGBA onto RGBA: full blending with blended alpha.
    fn overlay(&mut self, src: &[Rgba<u8>]) {
        assert_eq!(self.len(), src.len());
        let n = if !DISABLE_SSE4 && is_x86_feature_detected!("sse4.2") {
            unsafe { sse4::rgba8_as_rgba32f_overlay(self, src) }
        } else {
            0
        };
        // Process any remainder that couldn't be vectorized
        if n < self.len() {
            scalar::rgba8_as_rgba32f_overlay(&mut self[n..], &src[n..]);
        }
    }

    /// Overlay RGBA onto RGBA, ignoring destination alpha: use fast integer blending
    fn overlay_final(&mut self, src: &[Rgba<u8>]) {
        assert_eq!(self.len(), src.len());
        let n = if !DISABLE_AVX2 && is_x86_feature_detected!("avx2") {
            unsafe { avx2::rgba8_overlay_final(self, src) }
        } else if !DISABLE_SSE4 && is_x86_feature_detected!("sse4.2") {
            unsafe { sse4::rgba8_overlay_final(self, src) }
        } else {
            0
        };
        // Process any remainder that couldn't be vectorized
        if n < self.len() {
            scalar::rgba8_overlay_final(&mut self[n..], &src[n..]);
        }
    }
}

impl<T: Subpixel> Overlay<Rgb<T>> for Rgb<T> {
    /// Overlay RGB onto RGB (no alpha): just copy pixels.
    #[inline(always)]
    fn overlay(&mut self, src: &Rgb<T>) {
        *self = *src;
    }
}

impl<T: Subpixel> Overlay<Rgb<T>> for Rgba<T> {
    /// Overlay RGB onto RGBA: copy as if opaque.
    #[inline(always)]
    fn overlay(&mut self, src: &Rgb<T>) {
        *self = src.to_rgba();
    }
}

impl Overlay<Rgba<u8>> for Rgb<u8> {
    /// Overlay RGBA onto RGB: use fast integer blending with opaque background.
    #[inline]
    fn overlay(&mut self, src: &Rgba<u8>) {
        (self[0], self[1], self[2]) = blend_final_pixel_u8(
            (self[0], self[1], self[2]),
            (src[0], src[1], src[2]),
            src[3],
        );
    }
}

impl Overlay<Rgba<u8>> for Rgba<u8> {
    /// Overlay RGBA onto RGBA: full blending with blended alpha.
    fn overlay(&mut self, src: &Rgba<u8>) {
        // Zero alpha = keep original pixel
        if src[3] == 0 {
            return;
        }
        // Max alpha = overwrite with new pixel
        if src[3] == 255 {
            *self = *src;
            return;
        }

        // Otherwise, actually blend, as f32
        let mut dst_f32 = self.to_f32();
        let src_f32 = src.to_f32();
        dst_f32.overlay(&src_f32);
        *self = dst_f32.to_u8();
    }

    /// Overlay RGBA onto RGBA, ignoring destination alpha: use fast integer blending
    #[inline]
    fn overlay_final(&mut self, src: &Rgba<u8>) {
        (self[0], self[1], self[2]) = blend_final_pixel_u8(
            (self[0], self[1], self[2]),
            (src[0], src[1], src[2]),
            src[3],
        );
    }
}

impl Overlay<Rgba<f32>> for Rgb<f32> {
    /// Overlay RGBA onto RGB: always use simpler method without `dst_a`.
    fn overlay(&mut self, src: &Rgba<f32>) {
        // Zero alpha = keep original pixel
        if src[3] == 0.0 {
            return;
        }
        // Max alpha = overwrite with new pixel
        if src[3] == 1.0 {
            *self = src.to_rgb();
            return;
        }

        // When dst_a = 1.0, and therefore out_a = 1.0, then
        //      out_rgb * out_a = src_rgb * src_a + dst_rgb * dst_a * (1.0 - src_a)
        //  ->  out_rgb * 1.0 = src_rgb * src_a + dst_rgb * 1.0 * (1.0 - src_a)
        //  ->  out_rgb = src_rgb * src_a + dst_rgb * (1.0 - src_a)

        // dst_rgb * (1.0 - src_a)
        self[0] *= 1.0 - src[3];
        self[1] *= 1.0 - src[3];
        self[2] *= 1.0 - src[3];
        // out_rgb = src_rgb * src_a + dst_rgb * (1.0 - src_a)
        self[0] += src[0] * src[3];
        self[1] += src[1] * src[3];
        self[2] += src[2] * src[3];
    }
}

impl Overlay<Rgba<f32>> for Rgba<f32> {
    /// Overlay RGBA onto RGBA: full blending with blended alpha.
    fn overlay(&mut self, src: &Rgba<f32>) {
        // Zero alpha = keep original pixel
        if src[3] == 0.0 {
            return;
        }
        // Max alpha = overwrite with new pixel
        if src[3] == 1.0 {
            *self = *src;
            return;
        }

        // Calculate resulting alpha
        let a = self[3] + src[3] - self[3] * src[3];
        if a == 0.0 {
            // Resulting alpha would be 0, do nothing to avoid divide-by-0 at the end
            return;
        }

        // dst_rgb * dst_a
        self[0] *= self[3];
        self[1] *= self[3];
        self[2] *= self[3];
        // dst_rgb * dst_a * (1.0 - src_a)
        self[0] *= 1.0 - src[3];
        self[1] *= 1.0 - src[3];
        self[2] *= 1.0 - src[3];
        // out_rgb * out_a = src_rgb * src_a + dst_rgb * dst_a * (1.0 - src_a)
        self[0] += src[0] * src[3];
        self[1] += src[1] * src[3];
        self[2] += src[2] * src[3];
        // out_rgb
        self[0] /= a;
        self[1] /= a;
        self[2] /= a;
        // out_a
        self[3] = a;
    }
}

/// Overlay `src` in top-left corner of `dst`, according to the [`Overlay::overlay()`] implementation
/// between the two `Pixel` types. It's allowable for the images to have different sizes, only
/// the overlap will be processed.
pub fn overlay<D, S>(dst: &mut D, src: &S)
where
    D: ImageMut,
    S: Image,
    [D::Pixel]: Overlay<[S::Pixel]>,
{
    let rows = min(dst.height(), src.height());
    let cols = min(dst.width(), src.width());
    let mut dst_offset = dst.raw_pixel_offset();
    let dst_stride = dst.raw_pixel_row_stride();
    let dst_pixels = &mut dst.raw_pixels_mut();
    let mut src_offset = src.raw_pixel_offset();
    let src_stride = src.raw_pixel_row_stride();
    let src_pixels = &src.raw_pixels();

    for _ in 0..rows {
        dst_pixels[dst_offset..dst_offset + cols]
            .overlay(&src_pixels[src_offset..src_offset + cols]);
        dst_offset += dst_stride;
        src_offset += src_stride;
    }
}

/// Like [`overlay()`], but with the given offset. Negative offsets are allowed, only the
/// overlapping pixels will be affected.
pub fn overlay_at<D, S>(dst: &mut D, src: &S, left: isize, top: isize)
where
    D: ImageMut,
    S: Image,
    [D::Pixel]: Overlay<[S::Pixel]>,
{
    // Calculate `dst` and `src` views to achieve the desired offset:
    //   - A positive offset means an offset from the left/top of `dst`
    //   - A negative offset means an offset from the left/top of `src`
    let (dst_left, src_left) = if left < 0 {
        (0, (-left) as usize)
    } else {
        (left as usize, 0)
    };
    let (dst_top, src_top) = if top < 0 {
        (0, (-top) as usize)
    } else {
        (top as usize, 0)
    };
    let mut dst_view = dst.view_mut(dst_left, dst_top, usize::MAX, usize::MAX);
    let src_view = src.view(src_left, src_top, usize::MAX, usize::MAX);
    overlay(&mut dst_view, &src_view);
}

/// Overlay `src` in top-left corner of `dst`, according to the [`Overlay::overlay_final()`]
/// implementation between the two `Pixel` types. It's allowable for the images to have different
/// sizes, only the overlap will be processed.
pub fn overlay_final<D, S>(dst: &mut D, src: &S)
where
    D: ImageMut,
    S: Image,
    [D::Pixel]: Overlay<[S::Pixel]>,
{
    let rows = min(dst.height(), src.height());
    let cols = min(dst.width(), src.width());
    let mut dst_offset = dst.raw_pixel_offset();
    let dst_stride = dst.raw_pixel_row_stride();
    let dst_pixels = &mut dst.raw_pixels_mut();
    let mut src_offset = src.raw_pixel_offset();
    let src_stride = src.raw_pixel_row_stride();
    let src_pixels = &src.raw_pixels();

    for _ in 0..rows {
        dst_pixels[dst_offset..dst_offset + cols]
            .overlay_final(&src_pixels[src_offset..src_offset + cols]);
        dst_offset += dst_stride;
        src_offset += src_stride;
    }
}

/// Like [`overlay_final()`], but with the given offset. Negative offsets are allowed, only the
/// overlapping pixels will be affected.
pub fn overlay_final_at<D, S>(dst: &mut D, src: &S, left: isize, top: isize)
where
    D: ImageMut,
    S: Image,
    [D::Pixel]: Overlay<[S::Pixel]>,
{
    // Calculate `dst` and `src` views to achieve the desired offset:
    //   - A positive offset means an offset from the left/top of `dst`
    //   - A negative offset means an offset from the left/top of `src`
    let (dst_left, src_left) = if left < 0 {
        (0, (-left) as usize)
    } else {
        (left as usize, 0)
    };
    let (dst_top, src_top) = if top < 0 {
        (0, (-top) as usize)
    } else {
        (top as usize, 0)
    };
    let mut dst_view = dst.view_mut(dst_left, dst_top, usize::MAX, usize::MAX);
    let src_view = src.view(src_left, src_top, usize::MAX, usize::MAX);
    overlay_final(&mut dst_view, &src_view);
}

#[inline]
fn blend_final_pixel_u8(
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
