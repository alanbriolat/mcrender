use std::cmp::min;

use crate::canvas::{Image, ImageMut, Rgb, Rgba, Subpixel};

pub trait Overlay<P: ?Sized> {
    fn overlay(&mut self, src: &P);
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
        for (i, fg) in src.iter().enumerate() {
            self[i].overlay(fg);
        }
    }
}

impl Overlay<[Rgba<u8>]> for [Rgb<u8>] {
    /// Overlay RGBA onto RGB: use fast integer blending with opaque background.
    fn overlay(&mut self, src: &[Rgba<u8>]) {
        assert_eq!(self.len(), src.len());
        // TODO: SSE/AVX implementation
        for (i, fg) in src.iter().enumerate() {
            self[i].overlay(fg);
        }
    }
}

impl Overlay<[Rgba<u8>]> for [Rgba<u8>] {
    /// Overlay RGBA onto RGBA: full blending with blended alpha.
    fn overlay(&mut self, src: &[Rgba<u8>]) {
        assert_eq!(self.len(), src.len());
        // TODO: SSE/AVX implementation?
        for (i, fg) in src.iter().enumerate() {
            self[i].overlay(fg);
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
        *self = [src[0], src[1], src[2], T::MAX].into();
    }
}

impl Overlay<Rgba<u8>> for Rgb<u8> {
    /// Overlay RGBA onto RGB: use fast integer blending with opaque background.
    fn overlay(&mut self, src: &Rgba<u8>) {
        // Zero alpha = keep original pixel
        if src[3] == 0 {
            return;
        }
        // Max alpha = overwrite with new pixel
        if src[3] == 255 {
            *self = [src[0], src[1], src[2]].into();
            return;
        }
        // Otherwise, actually blend, using only integers

        // Upcast to u16
        let (bg_r, bg_g, bg_b) = (self[0] as u16, self[1] as u16, self[2] as u16);
        let (fg_r, fg_g, fg_b, fg_a) = (src[0] as u16, src[1] as u16, src[2] as u16, src[3] as u16);
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
        *self = [r as u8, g as u8, b as u8].into();
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
        // Otherwise, actually blend

        // Convert to f32 and normalize to 0.0-1.0
        let (bg_r, bg_g, bg_b, bg_a) = (
            f32::from(self[0]) / 255.0,
            f32::from(self[1]) / 255.0,
            f32::from(self[2]) / 255.0,
            f32::from(self[3]) / 255.0,
        );
        let (fg_r, fg_g, fg_b, fg_a) = (
            f32::from(src[0]) / 255.0,
            f32::from(src[1]) / 255.0,
            f32::from(src[2]) / 255.0,
            f32::from(src[3]) / 255.0,
        );

        // Calculate resulting alpha
        let a = bg_a + fg_a - bg_a * fg_a;
        if a == 0.0 {
            // Resulting alpha would be 0, do nothing to avoid divide by 0 at the end
            return;
        }

        // src_rgb * src_a
        let (fg_r, fg_g, fg_b) = (fg_r * fg_a, fg_g * fg_a, fg_b * fg_a);
        // dst_rgb * dst_a
        let (bg_r, bg_g, bg_b) = (bg_r * bg_a, bg_g * bg_a, bg_b * bg_a);
        // dst_rgb * dst_a * (1.0 - src_a)
        let fg_a_inv = 1.0 - fg_a;
        let (bg_r, bg_g, bg_b) = (bg_r * fg_a_inv, bg_g * fg_a_inv, bg_b * fg_a_inv);
        // out_rgb * out_a = src_rgb * src_a + dst_rgb * (1.0 - src_a)
        let (r, g, b) = (fg_r + bg_r, fg_g + bg_g, fg_b + bg_b);
        // out_rgb, by dividing by out_a
        let (r, g, b) = (r / a, g / a, b / a);
        // Convert back to 0-255 range and back into u8
        *self = [
            (r * 255.0) as u8,
            (g * 255.0) as u8,
            (b * 255.0) as u8,
            (a * 255.0) as u8,
        ]
        .into();
    }
}

/// Overlay `src` in top-left corner of `dst`, according to the `Overlay` implementation
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
    for y in 0..rows {
        let own_row = &mut dst.get_pixel_row_mut(y).unwrap()[..cols];
        let other_row = &src.get_pixel_row(y).unwrap()[..cols];
        own_row.overlay(other_row);
    }
}

/// Overlay `src` onto `dst`, with the given offset. Negative offsets are allowed, only the
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
