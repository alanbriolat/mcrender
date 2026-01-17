use std::marker::PhantomData;

use crate::canvas::{Image, ImageMut, Pixel, Rgba, TransmutablePixel, private};

pub struct ImageBuf<P: Pixel, Container = Vec<<P as Pixel>::Subpixel>> {
    width: usize,
    height: usize,
    len: usize,
    data: Container,
    _phantom: PhantomData<P>,
}

impl<P: Pixel, Container> ImageBuf<P, Container> {
    pub fn into_inner(self) -> Container {
        self.data
    }
}

impl<P, Container> ImageBuf<P, Container>
where
    P: TransmutablePixel,
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

impl<P, Container> ImageBuf<P, Container>
where
    P: TransmutablePixel,
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

impl<P: Pixel> ImageBuf<P, Vec<P::Subpixel>> {
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

impl<P, Container> Image for ImageBuf<P, Container>
where
    P: TransmutablePixel,
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

impl<P, Container> ImageMut for ImageBuf<P, Container>
where
    P: TransmutablePixel,
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

/// Adapt image-rs `image::RgbaImage` as `ImageBuf` sharing the same underlying buffer.
impl<'a> From<&'a image::RgbaImage> for ImageBuf<Rgba<u8>, &'a [u8]> {
    fn from(image: &'a image::RgbaImage) -> Self {
        Self::from_raw(
            image.width() as usize,
            image.height() as usize,
            image.as_ref(),
        )
        .unwrap()
    }
}

/// Adapt image-rs `image::RgbaImage` as `ImageBuf` sharing the same underlying buffer.
impl<'a> From<&'a mut image::RgbaImage> for ImageBuf<Rgba<u8>, &'a mut [u8]> {
    fn from(image: &'a mut image::RgbaImage) -> Self {
        Self::from_raw(
            image.width() as usize,
            image.height() as usize,
            image.as_mut(),
        )
        .unwrap()
    }
}

/// Adapt `ImageBuf` as image-rs `image::ImageBuffer` sharing the same underlying buffer.
impl<'a, B> From<&'a ImageBuf<Rgba<u8>, B>> for image::ImageBuffer<image::Rgba<u8>, &'a [u8]>
where
    B: AsRef<[u8]>,
{
    fn from(image: &'a ImageBuf<Rgba<u8>, B>) -> Self {
        image::ImageBuffer::from_raw(image.width as u32, image.height as u32, image.data.as_ref())
            .unwrap()
    }
}

/// Adapt `ImageBuf` as image-rs `image::ImageBuffer` sharing the same underlying buffer.
impl<'a, B> From<&'a mut ImageBuf<Rgba<u8>, B>>
    for image::ImageBuffer<image::Rgba<u8>, &'a mut [u8]>
where
    B: AsMut<[u8]>,
{
    fn from(image: &'a mut ImageBuf<Rgba<u8>, B>) -> Self {
        image::ImageBuffer::from_raw(image.width as u32, image.height as u32, image.data.as_mut())
            .unwrap()
    }
}

/// Convert `ImageBuf` into image-rs `image::RgbaImage`, giving it ownership of the underlying buffer.
impl From<ImageBuf<Rgba<u8>, Vec<u8>>> for image::RgbaImage {
    fn from(image: ImageBuf<Rgba<u8>, Vec<u8>>) -> Self {
        assert!(image.width <= u32::MAX as usize);
        assert!(image.height <= u32::MAX as usize);
        image::RgbaImage::from_raw(image.width as u32, image.height as u32, image.into_inner())
            .unwrap()
    }
}
