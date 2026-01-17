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

#[derive(Copy, Clone, Debug, Eq, PartialEq, derive_more::From, derive_more::Into)]
#[repr(transparent)]
pub struct Rgb<T: Subpixel>(pub [T; 3]);

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

impl<T: Subpixel> Pixel for Rgb<T> {
    type Subpixel = T;
    const CHANNELS: usize = 3;
}

unsafe impl TransmutablePixel for Rgb<u8> {}
unsafe impl TransmutablePixel for Rgb<f32> {}

#[derive(Copy, Clone, Debug, Eq, PartialEq, derive_more::From, derive_more::Into)]
#[repr(transparent)]
pub struct Rgba<T: Subpixel>(pub [T; 4]);

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

impl<T: Subpixel> Pixel for Rgba<T> {
    type Subpixel = T;
    const CHANNELS: usize = 4;
}

unsafe impl TransmutablePixel for Rgba<u8> {}
unsafe impl TransmutablePixel for Rgba<f32> {}

pub(crate) mod private {
    #[derive(Clone, Copy)]
    pub struct PrivateToken;
}
