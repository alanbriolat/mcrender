use crate::canvas::{Rgb, Rgba, scalar};

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
