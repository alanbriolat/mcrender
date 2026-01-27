use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};
use parking_lot::RwLock;

use crate::canvas;
use crate::canvas::{ImageBuf, ImageMut, Multiply, Overlay, Pixel, Rgb, Rgb8, Rgba8};
use crate::render::texture::TextureCache;

pub struct Sprite(pub Vec<SpriteLayer>);

impl Sprite {
    pub fn new() -> Sprite {
        Sprite(Vec::new())
    }

    pub fn with_capacity(capacity: usize) -> Sprite {
        Sprite(Vec::with_capacity(capacity))
    }

    pub fn add_new_layer<B: Into<Arc<SpriteBuffer>>>(
        &mut self,
        buffer: B,
        render_mode: RenderMode,
    ) {
        self.0.push(SpriteLayer {
            buffer: buffer.into(),
            render_mode,
        });
    }

    pub fn render_at<'c, I>(&self, output: &mut I, x: isize, y: isize)
    where
        I: ImageMut,
        [I::Pixel]: Overlay<[Rgba8]>,
    {
        for layer in self.0.iter() {
            layer.render_at(output, x, y);
        }
    }
}

pub struct SpriteLayer {
    pub buffer: Arc<SpriteBuffer>,
    pub render_mode: RenderMode,
}

impl SpriteLayer {
    pub fn render_at<I>(&self, output: &mut I, x: isize, y: isize)
    where
        I: ImageMut,
        [I::Pixel]: Overlay<[Rgba8]>,
    {
        use RenderMode::*;
        // TODO: context-awareness, lighting, etc.
        match self.render_mode {
            _ => {
                canvas::overlay_final_at(output, &*self.buffer, x, y);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderMode {
    Solid,
    SolidTop,
    SolidEast,
    SolidSouth,
    Translucent,
    TranslucentTop,
    TranslucentEast,
    TranslucentSouth,
}

pub const SPRITE_SIZE: usize = 24;
const SPRITE_BUF_SIZE: usize = SPRITE_SIZE * SPRITE_SIZE * <Rgba8 as Pixel>::CHANNELS;

pub type SpriteBuffer = ImageBuf<Rgba8, [u8; SPRITE_BUF_SIZE]>;

pub fn new_sprite_buffer() -> SpriteBuffer {
    ImageBuf::from_raw(SPRITE_SIZE, SPRITE_SIZE, [0; SPRITE_BUF_SIZE]).unwrap()
}

#[derive(Clone, Copy, Debug, Hash, Eq, PartialEq)]
pub enum Aspect {
    BlockTop,
    BlockBottom,
    BlockWest,
    BlockEast,
    BlockNorth,
    BlockSouth,
    BlockTopRotated,
    BlockEastRotated,
    BlockSouthRotated,
    // PlantTop,
    PlantBottom,
    // PlantWest,
    // PlantEast,
    // PlantNorth,
    // PlantSouth,
}

/// Flatten a sequence of projections into a single projection, in reverse order so they can be
/// written as a natural sequence of operations.
fn flatten_projection(projections: impl IntoIterator<Item = Projection>) -> Projection {
    projections
        .into_iter()
        .reduce(|acc, item| item * acc)
        .unwrap()
}

struct AspectProjection {
    projection: Projection,
    interpolation: Interpolation,
    tint: Option<Rgb8>,
}

static ASPECT_PROJECTIONS: OnceLock<HashMap<Aspect, AspectProjection>> = OnceLock::new();

const TINT_SOUTH: Rgb8 = Rgb([220, 220, 220]);
const TINT_EAST: Rgb8 = Rgb([200, 200, 200]);

fn init_aspect_projections() -> HashMap<Aspect, AspectProjection> {
    let mut projections = HashMap::new();
    projections.insert(
        Aspect::BlockTop,
        AspectProjection {
            projection: flatten_projection([
                Projection::translate(-8., -8.),
                Projection::rotate(45f32.to_radians()),
                Projection::scale(1.17, 1.17),
                Projection::scale(1.0, 0.5),
                Projection::translate(11.5, 5.5),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: None,
        },
    );
    projections.insert(
        Aspect::BlockBottom,
        AspectProjection {
            projection: flatten_projection([
                Projection::translate(-8., -8.),
                Projection::rotate(45f32.to_radians()),
                Projection::scale(1.17, 1.17),
                Projection::scale(1.0, 0.5),
                Projection::translate(11.5, 17.5),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: None,
        },
    );
    projections.insert(
        Aspect::BlockWest,
        AspectProjection {
            projection: flatten_projection([
                Projection::from_matrix([1., 0., 0., -0.5, 1., 0., 0., 0., 1.]).unwrap(),
                Projection::scale(12. / 16., 19. / 24.),
                Projection::translate(0., 5.),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: Some(TINT_EAST),
        },
    );
    projections.insert(
        Aspect::BlockEast,
        AspectProjection {
            projection: flatten_projection([
                Projection::from_matrix([1., 0., 0., -0.5, 1., 0., 0., 0., 1.]).unwrap(),
                Projection::scale(12. / 16., 19. / 24.),
                Projection::translate(12., 11.5),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: Some(TINT_EAST),
        },
    );
    projections.insert(
        Aspect::BlockNorth,
        AspectProjection {
            projection: flatten_projection([
                Projection::from_matrix([1., 0., 0., 0.5, 1., 0., 0., 0., 1.]).unwrap(),
                Projection::scale(13. / 16., 19. / 24.),
                Projection::translate(11.5, -0.8),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: Some(TINT_SOUTH),
        },
    );
    projections.insert(
        Aspect::BlockSouth,
        AspectProjection {
            projection: flatten_projection([
                Projection::from_matrix([1., 0., 0., 0.5, 1., 0., 0., 0., 1.]).unwrap(),
                Projection::scale(13. / 16., 19. / 24.),
                Projection::translate(-0.5, 5.6),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: Some(TINT_SOUTH),
        },
    );
    projections.insert(
        Aspect::BlockTopRotated,
        AspectProjection {
            projection: flatten_projection([
                Projection::translate(-8., -8.),
                Projection::rotate(135f32.to_radians()),
                Projection::scale(1.20, 1.14),
                Projection::scale(1.0, 0.5),
                Projection::translate(10.6, 5.3),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: None,
        },
    );
    projections.insert(
        Aspect::BlockEastRotated,
        AspectProjection {
            projection: flatten_projection([
                Projection::translate(-8., -8.),
                Projection::rotate(90f32.to_radians()),
                Projection::translate(8., 8.),
                Projection::from_matrix([1., 0., 0., -0.5, 1., 0., 0., 0., 1.]).unwrap(),
                Projection::scale(12. / 16., 19. / 24.),
                Projection::translate(11., 12.),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: Some(TINT_EAST),
        },
    );
    projections.insert(
        Aspect::BlockSouthRotated,
        AspectProjection {
            projection: flatten_projection([
                Projection::translate(-8., -8.),
                Projection::rotate(90f32.to_radians()),
                Projection::translate(8., 8.),
                Projection::from_matrix([1., 0., 0., 0.5, 1., 0., 0., 0., 1.]).unwrap(),
                Projection::scale(13. / 16., 19. / 24.),
                Projection::translate(-1., 5.5),
            ]),
            interpolation: Interpolation::Bilinear,
            tint: Some(TINT_SOUTH),
        },
    );
    projections.insert(
        Aspect::PlantBottom,
        AspectProjection {
            projection: flatten_projection([
                Projection::scale(1., 12. / 16.),
                Projection::translate(4., 6.),
            ]),
            interpolation: Interpolation::Nearest,
            tint: None,
        },
    );
    projections
}

fn get_aspect_projection(aspect: Aspect) -> &'static AspectProjection {
    ASPECT_PROJECTIONS
        .get_or_init(init_aspect_projections)
        .get(&aspect)
        .unwrap()
}

pub struct PartialSpriteCache {
    textures: TextureCache,
    cache: RwLock<HashMap<(Cow<'static, str>, Aspect, Option<Rgb8>), Arc<SpriteBuffer>>>,
}

impl PartialSpriteCache {
    pub fn new(textures: TextureCache) -> Self {
        Self {
            textures,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn get(&self, name: &str, aspect: Aspect) -> anyhow::Result<Arc<SpriteBuffer>> {
        self.get_tinted(name, aspect, None)
    }

    pub fn get_tinted(
        &self,
        name: &str,
        aspect: Aspect,
        tint: Option<Rgb8>,
    ) -> anyhow::Result<Arc<SpriteBuffer>> {
        // Try to get the buffer with just a read lock
        if let Some(image) = self.cache.read().get(&(Cow::Borrowed(name), aspect, tint)) {
            return Ok(image.clone());
        }

        // Generate the buffer, not holding the lock while we do so
        let texture = self.textures.get(name)?;
        let mut buffer = self.render_aspect(&*texture, aspect);
        if let Some(tint_color) = tint {
            buffer.pixels_mut().multiply(&tint_color);
        }

        // Get the write lock
        let mut cache = self.cache.write();
        if let Some(image) = cache.get(&(Cow::Borrowed(name), aspect, tint)) {
            // If something else populated the cache in the meantime, reuse that entry
            Ok(image.clone())
        } else {
            // Otherwise store the new cache entry
            let buffer = Arc::new(buffer);
            cache.insert((Cow::Owned(name.to_owned()), aspect, tint), buffer.clone());
            Ok(buffer)
        }
    }

    fn render_aspect(&self, texture: &image::RgbaImage, aspect: Aspect) -> SpriteBuffer {
        let ap = get_aspect_projection(aspect);
        let mut image = image::RgbaImage::new(SPRITE_SIZE as u32, SPRITE_SIZE as u32);
        warp_into(
            texture,
            &ap.projection,
            ap.interpolation,
            [0, 0, 0, 0].into(),
            &mut image,
        );
        let samples = image.into_flat_samples().samples;
        let mut raw_buf = [0; SPRITE_BUF_SIZE];
        raw_buf.copy_from_slice(&samples[..SPRITE_BUF_SIZE]);
        let mut output = ImageBuf::from_raw(SPRITE_SIZE, SPRITE_SIZE, raw_buf).unwrap();
        if let Some(tint) = ap.tint {
            output.pixels_mut().multiply(&tint);
        }
        output
    }
}
