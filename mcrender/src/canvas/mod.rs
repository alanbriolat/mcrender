mod buffer;
mod overlay;
mod pixel;
mod view;

pub use buffer::ImageBuf;
pub use overlay::{overlay, overlay_at};
pub use pixel::*;
pub use view::ImageView;

pub type Rgb8 = Rgb<u8>;
pub type Rgb32f = Rgb<f32>;
pub type Rgba8 = Rgba<u8>;
pub type Rgba32f = Rgba<f32>;

pub trait Image {
    type Pixel: Pixel;

    fn width(&self) -> usize;

    fn height(&self) -> usize;

    fn in_bounds(&self, x: usize, y: usize) -> bool {
        x < self.width() && y < self.height()
    }

    fn get_pixel(&self, x: usize, y: usize) -> Option<&Self::Pixel>;

    fn get_pixel_row(&self, y: usize) -> Option<&[Self::Pixel]>;

    fn pixel_rows(&self) -> impl Iterator<Item = &[Self::Pixel]> + '_ {
        (0..self.height()).map(|y| self.get_pixel_row(y).unwrap())
    }

    fn view(&self, left: usize, top: usize, width: usize, height: usize) -> ImageView<&Self> {
        ImageView::new(self, left, top, width, height)
    }
}

pub trait ImageMut: Image {
    fn get_pixel_mut(&mut self, x: usize, y: usize) -> Option<&mut Self::Pixel>;

    fn get_pixel_row_mut(&mut self, y: usize) -> Option<&mut [Self::Pixel]>;

    fn view_mut(
        &mut self,
        left: usize,
        top: usize,
        width: usize,
        height: usize,
    ) -> ImageView<&mut Self> {
        ImageView::new(self, left, top, width, height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer() {
        let mut buf = ImageBuf::<Rgba<u8>, _>::from_pixel(2, 3, [10, 20, 30, 40].into());
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
        let buf: ImageBuf<Rgb<u8>, _> = ImageBuf::from_raw(3, 4, raw_data).unwrap();
        let view = buf.view(1, 1, 2, 3);
        assert_eq!(view.get_pixel(0, 0), Some(&Rgb([5, 5, 5])));
        let view = view.view(0, 1, 2, 2);
        assert_eq!(view.get_pixel(0, 0), Some(&Rgb([8, 8, 8])));
    }

    #[test]
    fn test_overlay_buffer_rgb_rgb() {
        let mut buf = ImageBuf::from_pixel(8, 6, Rgb::<u8>([127, 127, 127]));
        let other = ImageBuf::from_pixel(2, 3, Rgb::<u8>([1, 1, 1]));
        overlay(&mut buf, &other);
        for y in 0..buf.height() {
            for x in 0..buf.width() {
                if other.in_bounds(x, y) {
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
        let mut buf = ImageBuf::from_pixel(8, 6, bg);
        // Create a mutable view into that buffer
        let mut view = buf.view_mut(1, 2, 7, 4);
        // Create an overlay image, and apply it to the view
        let other = ImageBuf::from_pixel(2, 3, fg);
        overlay(&mut view, &other); // Rows above the view-adjusted overlay are unchanged
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
