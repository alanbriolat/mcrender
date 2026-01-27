use std::ops::{Deref, DerefMut};

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

pub trait Pixel: Copy + Clone + Deref<Target = [Self::Subpixel]> {
    type Subpixel: Subpixel;
    const CHANNELS: usize;
}

/// Mark a `Pixel` type as having the correct layout to transmute between a slice of pixels and a
/// slice of channels.
pub unsafe trait TransmutablePixel: Pixel {
    #[inline(always)]
    fn channels_from_slice(_: private::PrivateToken, pixels: &[Self]) -> &[Self::Subpixel] {
        unsafe {
            std::slice::from_raw_parts(
                pixels.as_ptr() as *const Self::Subpixel,
                pixels.len() * Self::CHANNELS,
            )
        }
    }

    #[inline(always)]
    fn channels_from_slice_mut(
        _: private::PrivateToken,
        pixels: &mut [Self],
    ) -> &mut [Self::Subpixel] {
        unsafe {
            std::slice::from_raw_parts_mut(
                pixels.as_mut_ptr() as *mut Self::Subpixel,
                pixels.len() * Self::CHANNELS,
            )
        }
    }

    #[inline(always)]
    fn slice_from_channels(_: private::PrivateToken, channels: &[Self::Subpixel]) -> &[Self] {
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
        _: private::PrivateToken,
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

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, derive_more::From, derive_more::Into)]
#[repr(transparent)]
pub struct Rgb<T: Subpixel>(pub [T; 3]);

impl<T: Subpixel + Default> Default for Rgb<T> {
    fn default() -> Self {
        Rgb([T::default(), T::default(), T::default()])
    }
}

impl<T: Subpixel> Rgb<T> {
    #[inline(always)]
    pub fn to_rgba(self) -> Rgba<T> {
        Rgba([self[0], self[1], self[2], T::MAX])
    }
}

impl Rgb<u8> {
    #[inline(always)]
    pub fn to_f32(self) -> Rgb<f32> {
        Rgb([
            f32::from(self[0]) / 255.0,
            f32::from(self[1]) / 255.0,
            f32::from(self[2]) / 255.0,
        ])
    }
}

impl Rgb<f32> {
    #[inline(always)]
    pub fn to_u8(self) -> Rgb<u8> {
        Rgb([
            (self[0] * 255.0) as u8,
            (self[1] * 255.0) as u8,
            (self[2] * 255.0) as u8,
        ])
    }
}

impl<T: Subpixel> Deref for Rgb<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Subpixel> DerefMut for Rgb<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Subpixel> Pixel for Rgb<T> {
    type Subpixel = T;
    const CHANNELS: usize = 3;
}

impl From<Rgb<u8>> for image::Rgb<u8> {
    fn from(rgb: Rgb<u8>) -> Self {
        rgb.0.into()
    }
}

impl From<u32> for Rgb<u8> {
    fn from(raw: u32) -> Self {
        Rgb([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
    }
}

impl From<Rgb<u8>> for u32 {
    fn from(rgb: Rgb<u8>) -> Self {
        ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | (rgb[2] as u32)
    }
}

unsafe impl TransmutablePixel for Rgb<u8> {}
unsafe impl TransmutablePixel for Rgb<f32> {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, derive_more::From, derive_more::Into)]
#[repr(transparent)]
pub struct Rgba<T: Subpixel>(pub [T; 4]);

impl<T: Subpixel + Default> Default for Rgba<T> {
    fn default() -> Self {
        Rgba([T::default(), T::default(), T::default(), T::default()])
    }
}

impl<T: Subpixel> Rgba<T> {
    #[inline(always)]
    pub fn to_rgb(self) -> Rgb<T> {
        Rgb([self[0], self[1], self[2]])
    }
}

impl Rgba<u8> {
    #[inline(always)]
    pub fn to_f32(self) -> Rgba<f32> {
        Rgba([
            f32::from(self[0]) / 255.0,
            f32::from(self[1]) / 255.0,
            f32::from(self[2]) / 255.0,
            f32::from(self[3]) / 255.0,
        ])
    }
}

impl Rgba<f32> {
    #[inline(always)]
    pub fn to_u8(self) -> Rgba<u8> {
        Rgba([
            (self[0] * 255.0) as u8,
            (self[1] * 255.0) as u8,
            (self[2] * 255.0) as u8,
            (self[3] * 255.0) as u8,
        ])
    }
}

impl<T: Subpixel> Deref for Rgba<T> {
    type Target = [T];

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: Subpixel> DerefMut for Rgba<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: Subpixel> Pixel for Rgba<T> {
    type Subpixel = T;
    const CHANNELS: usize = 4;
}

impl From<Rgba<u8>> for image::Rgba<u8> {
    fn from(rgba: Rgba<u8>) -> Self {
        rgba.0.into()
    }
}

unsafe impl TransmutablePixel for Rgba<u8> {}
unsafe impl TransmutablePixel for Rgba<f32> {}

pub(crate) mod private {
    #[derive(Clone, Copy)]
    pub struct PrivateToken;
}
