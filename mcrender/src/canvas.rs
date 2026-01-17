use std::cmp::min;
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut, Range};

use num_traits::Num;

pub trait Subpixel: Copy + Clone + Num + PartialOrd<Self> {
    const MAX: Self;
}

impl Subpixel for u8 {
    const MAX: u8 = u8::MAX;
}
impl Subpixel for f32 {
    const MAX: f32 = 1.0;
}

pub trait Pixel:
    Copy + Clone + Deref<Target = [Self::Subpixel]> + private::TransmutablePixel
{
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, derive_more::From, derive_more::Into)]
#[repr(transparent)]
pub struct Rgb<T: Subpixel>([T; 3]);

impl<T: Subpixel> Deref for Rgb<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Rgb<u8> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Pixel for Rgb<u8> {}
pub type Rgb8 = Rgb<u8>;
impl Pixel for Rgb<f32> {}
pub type Rgb32f = Rgb<f32>;

#[derive(Copy, Clone, Debug, Eq, PartialEq, derive_more::From, derive_more::Into)]
#[repr(transparent)]
pub struct Rgba<T: Subpixel>([T; 4]);

impl<T: Subpixel> Deref for Rgba<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Rgba<u8> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Pixel for Rgba<u8> {}
pub type Rgba8 = Rgba<u8>;
impl Pixel for Rgba<f32> {}
pub type Rgba32f = Rgba<f32>;

pub struct Buffer<P: Pixel, Container = Vec<<P as private::TransmutablePixel>::Subpixel>> {
    width: usize,
    height: usize,
    len: usize,
    data: Container,
    _phantom: PhantomData<P>,
}

impl<P: Pixel, Container> Buffer<P, Container> {
    pub fn into_inner(self) -> Container {
        self.data
    }
}

impl<P, Container> Buffer<P, Container>
where
    P: Pixel,
    Container: AsRef<[P::Subpixel]>,
{
    pub fn from_raw(width: usize, height: usize, buf: Container) -> Option<Self> {
        let len = width * height * P::CHANNELS;
        if len < buf.as_ref().len() {
            None
        } else {
            Some(Self {
                width,
                height,
                len,
                data: buf,
                _phantom: PhantomData,
            })
        }
    }

    #[inline]
    pub fn channels(&self) -> &[P::Subpixel] {
        &self.data.as_ref()[..self.len]
    }

    #[inline]
    pub fn channel_index(&self, x: usize, y: usize) -> Option<usize> {
        match self.pixel_index(x, y) {
            Some(i) => Some(i * P::CHANNELS),
            None => None,
        }
    }

    #[inline]
    pub fn pixels(&self) -> &[P] {
        P::slice_from_channels(private::PrivateToken, self.channels())
    }

    #[inline]
    pub fn pixel_index(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }
}

impl<P, Container> Buffer<P, Container>
where
    P: Pixel,
    Container: AsMut<[P::Subpixel]>,
{
    #[inline]
    pub fn channels_mut(&mut self) -> &mut [P::Subpixel] {
        &mut self.data.as_mut()[..self.len]
    }

    #[inline]
    pub fn pixels_mut(&mut self) -> &mut [P] {
        P::slice_from_channels_mut(private::PrivateToken, self.channels_mut())
    }
}

impl<P: Pixel> Buffer<P, Vec<P::Subpixel>> {
    pub fn from_pixel(width: usize, height: usize, pixel: P) -> Self {
        let count = width * height;
        let len = count * P::CHANNELS;
        let mut data = Vec::with_capacity(len);
        for _ in 0..count {
            data.extend_from_slice(&pixel);
        }
        Self {
            width,
            height,
            len,
            data,
            _phantom: PhantomData,
        }
    }
}

pub trait Image {
    type Pixel: Pixel;

    fn width(&self) -> usize;

    fn height(&self) -> usize;

    fn in_bounds(&self, x: usize, y: usize) -> bool {
        x < self.width() && y < self.height()
    }

    fn get_pixel(&self, x: usize, y: usize) -> Option<&Self::Pixel>;

    fn get_pixel_row(&self, y: usize) -> Option<&[Self::Pixel]>;

    fn get_channels_row(
        &self,
        y: usize,
    ) -> Option<&[<Self::Pixel as private::TransmutablePixel>::Subpixel]> {
        self.get_pixel_row(y).map(|pixels| {
            <Self::Pixel as private::TransmutablePixel>::channels_from_slice(
                private::PrivateToken,
                pixels,
            )
        })
    }

    fn pixel_rows(&self) -> impl Iterator<Item = &[Self::Pixel]> + '_ {
        (0..self.height()).map(|y| self.get_pixel_row(y).unwrap())
    }

    fn view(&self, left: usize, top: usize, width: usize, height: usize) -> View<&Self> {
        assert!(left + width <= self.width());
        assert!(top + height <= self.height());
        View::new(self, left, top, width, height)
    }
}

pub trait ImageMut: Image {
    fn get_pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut Self::Pixel>;

    fn get_pixel_row_mut(&mut self, y: usize) -> Option<&mut [Self::Pixel]>;

    fn get_channels_row_mut(
        &mut self,
        y: usize,
    ) -> Option<&mut [<Self::Pixel as private::TransmutablePixel>::Subpixel]> {
        self.get_pixel_row_mut(y).map(|pixels| {
            <Self::Pixel as private::TransmutablePixel>::channels_from_slice_mut(
                private::PrivateToken,
                pixels,
            )
        })
    }

    fn view_mut(
        &mut self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> View<&mut Self> {
        assert!(left + width <= self.width());
        assert!(top + height <= self.height());
        View::new(self, left, top, width, height)
    }

    /// Overlay `other` in top-left corner of this `Image`, according to the `Overlay` implementation
    /// between the two `Pixel` types. It's allowable for the images to have different sizes, only
    /// the overlap will be processed.
    fn overlay<I>(&mut self, other: &I)
    where
        I: Image,
        [Self::Pixel]: Overlay<[I::Pixel]>,
    {
        let rows = min(self.height(), other.height());
        let cols = min(self.width(), other.width());
        for y in 0..rows {
            let own_row = &mut self.get_pixel_row_mut(y).unwrap()[..cols];
            let other_row = &other.get_pixel_row(y).unwrap()[..cols];
            own_row.overlay(other_row);
        }
    }
}

impl<P, Container> Image for Buffer<P, Container>
where
    P: Pixel,
    Container: AsRef<[P::Subpixel]>,
{
    type Pixel = P;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn get_pixel(&self, x: usize, y: usize) -> Option<&Self::Pixel> {
        self.pixel_index(x, y).map(|i| &self.pixels()[i])
    }

    fn get_pixel_row(&self, y: usize) -> Option<&[Self::Pixel]> {
        self.pixel_index(0, y)
            .map(|i| &self.pixels()[i..i + self.width])
    }

    fn pixel_rows(&self) -> impl Iterator<Item = &[Self::Pixel]> + '_ {
        self.pixels().chunks(self.width)
    }
}

impl<P, Container> ImageMut for Buffer<P, Container>
where
    P: Pixel,
    Container: AsRef<[P::Subpixel]> + AsMut<[P::Subpixel]>,
{
    fn get_pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut Self::Pixel> {
        self.pixel_index(x, y).map(|i| &mut self.pixels_mut()[i])
    }

    fn get_pixel_row_mut(&mut self, y: usize) -> Option<&mut [Self::Pixel]> {
        self.pixel_index(0, y).map(|i| {
            let end = i + self.width;
            &mut self.pixels_mut()[i..end]
        })
    }
}

pub struct View<I> {
    image: I,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

impl<I> View<I> {
    fn new(image: I, left: usize, top: usize, width: usize, height: usize) -> Self {
        Self {
            image,
            left,
            top,
            width,
            height,
        }
    }
}

impl<I> View<I>
where
    I: Deref,
    I::Target: Image,
{
    fn view(&self, left: usize, top: usize, width: usize, height: usize) -> View<&I::Target> {
        assert!(left + width <= self.width);
        assert!(top + height <= self.height);
        View::new(
            &*self.image,
            left + self.left,
            top + self.top,
            width,
            height,
        )
    }
}

impl<I> View<I>
where
    I: Deref + DerefMut,
    I::Target: Image,
{
    fn view_mut(
        &mut self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> View<&mut I::Target> {
        assert!(left + width <= self.width);
        assert!(top + height <= self.height);
        View::new(
            &mut *self.image,
            left + self.left,
            top + self.top,
            width,
            height,
        )
    }
}

impl<I> Image for View<I>
where
    I: Deref,
    I::Target: Image,
{
    type Pixel = <I::Target as Image>::Pixel;

    fn width(&self) -> usize {
        self.width
    }

    fn height(&self) -> usize {
        self.height
    }

    fn get_pixel(&self, x: usize, y: usize) -> Option<&Self::Pixel> {
        self.image
            .get_pixel(x.saturating_add(self.left), y.saturating_add(self.top))
    }

    fn get_pixel_row(&self, y: usize) -> Option<&[Self::Pixel]> {
        let range = self.left..self.left + self.width;
        self.image
            .get_pixel_row(y.saturating_add(self.top))
            .map(|row| &row[range])
    }
}

impl<I> ImageMut for View<I>
where
    I: DerefMut,
    I::Target: ImageMut,
{
    fn get_pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut Self::Pixel> {
        self.image
            .get_pixel_mut(x.saturating_add(self.left), y.saturating_add(self.top))
    }

    fn get_pixel_row_mut(&mut self, y: usize) -> Option<&mut [Self::Pixel]> {
        let range = self.left..self.left + self.width;
        self.image
            .get_pixel_row_mut(y.saturating_add(self.top))
            .map(|row| &mut row[range])
    }
}

pub struct PixelRows<'i, I: Image + ?Sized> {
    image: &'i I,
    iter: Range<usize>,
}

impl<'i, I: Image> Iterator for PixelRows<'i, I> {
    type Item = &'i [I::Pixel];

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(i) => Some(self.image.get_pixel_row(i).unwrap()),
            None => None,
        }
    }
}

pub struct ChannelRows<'i, I: Image> {
    image: &'i I,
    iter: Range<usize>,
}

impl<'i, I: Image> Iterator for ChannelRows<'i, I> {
    type Item = &'i [<I::Pixel as private::TransmutablePixel>::Subpixel];

    fn next(&mut self) -> Option<Self::Item> {
        match self.iter.next() {
            Some(i) => Some(self.image.get_channels_row(i).unwrap()),
            None => None,
        }
    }
}

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

pub(crate) mod private {
    #[derive(Clone, Copy)]
    pub struct PrivateToken;

    pub trait TransmutablePixel: Sized {
        type Subpixel: Clone + Copy;
        const CHANNELS: usize;

        #[inline(always)]
        fn channels_from_slice(_: PrivateToken, pixels: &[Self]) -> &[Self::Subpixel] {
            unsafe {
                std::slice::from_raw_parts(
                    pixels.as_ptr() as *const Self::Subpixel,
                    pixels.len() * Self::CHANNELS,
                )
            }
        }

        #[inline(always)]
        fn channels_from_slice_mut(_: PrivateToken, pixels: &mut [Self]) -> &mut [Self::Subpixel] {
            unsafe {
                std::slice::from_raw_parts_mut(
                    pixels.as_mut_ptr() as *mut Self::Subpixel,
                    pixels.len() * Self::CHANNELS,
                )
            }
        }

        #[inline(always)]
        fn slice_from_channels(_: PrivateToken, channels: &[Self::Subpixel]) -> &[Self] {
            assert_eq!(channels.len() % Self::CHANNELS, 0);
            unsafe {
                std::slice::from_raw_parts(
                    channels.as_ptr() as *const Self,
                    channels.len() / Self::CHANNELS,
                )
            }
        }

        #[inline(always)]
        fn slice_from_channels_mut(
            _: PrivateToken,
            channels: &mut [Self::Subpixel],
        ) -> &mut [Self] {
            assert_eq!(channels.len() % Self::CHANNELS, 0);
            unsafe {
                std::slice::from_raw_parts_mut(
                    channels.as_mut_ptr() as *mut Self,
                    channels.len() / Self::CHANNELS,
                )
            }
        }
    }

    impl TransmutablePixel for super::Rgb<u8> {
        type Subpixel = u8;
        const CHANNELS: usize = 3;
    }

    impl TransmutablePixel for super::Rgba<u8> {
        type Subpixel = u8;
        const CHANNELS: usize = 4;
    }

    impl TransmutablePixel for super::Rgb<f32> {
        type Subpixel = f32;
        const CHANNELS: usize = 3;
    }

    impl TransmutablePixel for super::Rgba<f32> {
        type Subpixel = f32;
        const CHANNELS: usize = 4;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer() {
        let mut buf = Buffer::<Rgba<u8>, _>::from_pixel(2, 3, [10, 20, 30, 40].into());
        assert_eq!(buf.channels()[1], 20);
        assert_eq!(buf.pixels()[1][1], 20);
        assert!(buf.get_pixel_mut(2, 2).is_none());
        assert!(buf.get_pixel_mut(1, 3).is_none());
    }

    #[test]
    fn test_view() {
        let raw_data = vec![
            1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4, 5, 5, 5, 6, 6, 6, 7, 7, 7, 8, 8, 8, 9, 9, 9, 10,
            10, 10, 11, 11, 11, 12, 12, 12,
        ];
        let buf: Buffer<Rgb<u8>, _> = Buffer::from_raw(3, 4, raw_data).unwrap();
        let view = buf.view(1, 1, 2, 3);
        assert_eq!(view.get_pixel(0, 0), Some(&Rgb([5, 5, 5])));
        let view = view.view(0, 1, 2, 2);
        assert_eq!(view.get_pixel(0, 0), Some(&Rgb([8, 8, 8])));
    }

    #[test]
    fn test_overlay_buffer_rgb_rgb() {
        let mut buf = Buffer::from_pixel(8, 6, Rgb::<u8>([127, 127, 127]));
        let overlay = Buffer::from_pixel(2, 3, Rgb::<u8>([1, 1, 1]));
        buf.overlay(&overlay);
        for y in 0..buf.height() {
            for x in 0..buf.width() {
                if overlay.in_bounds(x, y) {
                    assert_eq!(buf.get_pixel(x, y), Some(&Rgb([1, 1, 1])));
                } else {
                    assert_eq!(buf.get_pixel(x, y), Some(&Rgb([127, 127, 127])));
                }
            }
        }
    }

    #[test]
    fn test_overlay_view_rgb_rgb() {
        let bg = Rgb::<u8>([127, 127, 127]);
        let fg = Rgb::<u8>([1, 1, 1]);
        assert_ne!(bg, fg);
        // Create a buffer
        let mut buf = Buffer::from_pixel(8, 6, bg);
        // Create a mutable view into that buffer
        let mut view = buf.view_mut(1, 2, 7, 4);
        // Create an overlay image, and apply it to the view
        let overlay = Buffer::from_pixel(2, 3, fg);
        view.overlay(&overlay);
        // Rows above the view-adjusted overlay are unchanged
        for y in 0..2 {
            assert_eq!(
                buf.get_pixel_row(y),
                Some([bg, bg, bg, bg, bg, bg, bg, bg].as_slice())
            );
        }
        // Rows covered by the view-adjusted overlay are updated
        for y in 2..5 {
            assert_eq!(
                buf.get_pixel_row(y),
                Some([bg, fg, fg, bg, bg, bg, bg, bg].as_slice())
            );
        }
        // Rows below the view-adjusted overlay are unchanged
        for y in 5..buf.height() {
            assert_eq!(
                buf.get_pixel_row(y),
                Some([bg, bg, bg, bg, bg, bg, bg, bg].as_slice())
            );
        }
    }
}
