use std::cmp::min;

use crate::canvas::{Rgb, Rgba, scalar, ImageMut, Image, sse4, avx2};

const DISABLE_AVX2: bool = false;
const DISABLE_SSE4: bool = false;

pub trait Multiply<P: ?Sized = Self> {
    fn multiply(&mut self, src: &P);
}

impl Multiply<Rgb<u8>> for Rgba<u8> {
    fn multiply(&mut self, src: &Rgb<u8>) {
        self[0] = scalar::u16_div_by_255(self[0] as u16 * src[0] as u16) as u8;
        self[1] = scalar::u16_div_by_255(self[1] as u16 * src[1] as u16) as u8;
        self[2] = scalar::u16_div_by_255(self[2] as u16 * src[2] as u16) as u8;
    }
}

impl Multiply<Rgb<u8>> for [Rgba<u8>] {
    fn multiply(&mut self, src: &Rgb<u8>) {
        self.iter_mut().for_each(|p| p.multiply(src));
    }
}

pub trait MultiplyOverlay<M: ?Sized, O: ?Sized> {
    /// Multiply `overlay` by `multiply` and blend it onto `self`.
    fn multiply_overlay_final(&mut self, multiply: &M, overlay: &O);
}

impl MultiplyOverlay<Rgb<u8>, [Rgba<u8>]> for [Rgba<u8>] {
    /// Multiply RGBA by RGB and overlay onto RGBA, ignoring destination alpha channel.
    fn multiply_overlay_final(&mut self, multiply: &Rgb<u8>, overlay: &[Rgba<u8>]) {
        assert_eq!(self.len(), overlay.len());
        let n = if !DISABLE_AVX2 && is_x86_feature_detected!("avx2") {
            unsafe { avx2::rgba8_multiply_overlay_final(self, multiply, overlay) }
        } else if !DISABLE_SSE4 && is_x86_feature_detected!("sse4.2") {
            unsafe { sse4::rgba8_multiply_overlay_final(self, multiply, overlay) }
        } else {
            0
        };
        // Process any remainder that couldn't be vectorized
        if n < self.len() {
            scalar::rgba8_multiply_overlay_final(&mut self[n..], multiply, &overlay[n..]);
        }
    }
}

pub fn multiply_overlay_final<D, S, M>(dst: &mut D, src: &S, multiply: &M)
where
    D: ImageMut,
    S: Image,
    [D::Pixel]: MultiplyOverlay<M, [S::Pixel]>,
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
        dst_pixels[dst_offset..dst_offset+cols].multiply_overlay_final(multiply, &src_pixels[src_offset..src_offset+cols]);
        dst_offset += dst_stride;
        src_offset += src_stride;
    }
}

pub fn multiply_overlay_final_at<D, S, M>(dst: &mut D, src: &S, multiply: &M, left: isize, top: isize)
where
    D: ImageMut,
    S: Image,
    [D::Pixel]: MultiplyOverlay<M, [S::Pixel]>,
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
    multiply_overlay_final(&mut dst_view, &src_view, multiply);
}
