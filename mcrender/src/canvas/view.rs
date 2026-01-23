use std::cmp::min;
use std::ops::{Deref, DerefMut};

use crate::canvas::{Image, ImageMut};

pub struct ImageView<I> {
    image: I,
    left: usize,
    top: usize,
    width: usize,
    height: usize,
}

impl<I> ImageView<I>
where
    I: Deref,
    I::Target: Image,
{
    /// Create a new `ImageView` wrapping around `image`, with the position and extent clamped to
    /// remain within the area of `image`.
    pub fn new(image: I, left: usize, top: usize, width: usize, height: usize) -> Self {
        let left = min(left, image.width());
        let top = min(top, image.height());
        let width = min(width, image.width().saturating_sub(left));
        let height = min(height, image.height().saturating_sub(top));
        Self {
            image,
            left,
            top,
            width,
            height,
        }
    }

    /// Alternative to `Image::view()` that will avoid an extra level of indirection.
    pub fn view(
        &self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> ImageView<&I::Target> {
        ImageView::new(
            &*self.image,
            left.saturating_add(self.left),
            top.saturating_add(self.top),
            width,
            height,
        )
    }
}

impl<I> ImageView<I>
where
    I: Deref + DerefMut,
    I::Target: Image,
{
    /// Alternative to `ImageMut::view_mut()` that will avoid an extra level of indirection.
    pub fn view_mut(
        &mut self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> ImageView<&mut I::Target> {
        ImageView::new(
            &mut *self.image,
            left.saturating_add(self.left),
            top.saturating_add(self.top),
            width,
            height,
        )
    }
}

impl<I> Image for ImageView<I>
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

    fn raw_pixels(&self) -> &[Self::Pixel] {
        self.image.raw_pixels()
    }

    fn raw_pixel_offset(&self) -> usize {
        // To the old offset, add:
        //  - the old row stride for each row being skipped
        //  - the offset into the start of the row
        self.image.raw_pixel_offset() + self.image.raw_pixel_row_stride() * self.top + self.left
    }

    fn raw_pixel_row_stride(&self) -> usize {
        // Row stride is unchanged, because down one row in a view is also down one row in the
        // underlying raw buffer
        self.image.raw_pixel_row_stride()
    }
}

impl<I> ImageMut for ImageView<I>
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

    fn raw_pixels_mut(&mut self) -> &mut [Self::Pixel] {
        self.image.raw_pixels_mut()
    }
}
