use crate::asset::{AssetCache, SPRITE_SIZE};
use crate::canvas;
use crate::canvas::{ImageMut, Overlay, Rgba8};
use crate::coords::{PointXZY, Vec2D};
use crate::settings::Settings;
use crate::world::{CHUNK_SIZE, Chunk, Section, WORLD_HEIGHT};

/// The image width required to fully render a chunk section (16x16x16 blocks).
///
/// In isometric view, this is the left-to-right diagonal across the top of the chunk, which is the
/// sprite width multiplied by the number of blocks on the diagonal.
pub const SECTION_RENDER_WIDTH: usize = SPRITE_SIZE * CHUNK_SIZE as usize;
/// The image height required to fully render a chunk section.
///
/// In isometric view, this is the top-to-bottom diagonal across the top of the chunk, plus the
/// vertical down the side of the section, which are both half the sprite height multiplied by number
/// of blocks diagonally/vertically respectively.
pub const SECTION_RENDER_HEIGHT: usize = (SPRITE_SIZE / 2) * (2 * CHUNK_SIZE as usize);

/// The image width required to fully render a chunk. The same as for a section.
pub const CHUNK_RENDER_WIDTH: usize = SECTION_RENDER_WIDTH;
/// The image height required to fully render a chunk (16x16x384 blocks).
///
/// This is the top-to-bottom diagonal across the top of the chunk (half the sprite height times the
/// number of blocks diagonally), plus the vertical down the side (half the sprite height times the
/// number of blocks vertically).
pub const CHUNK_RENDER_HEIGHT: usize =
    (SPRITE_SIZE / 2) * (CHUNK_SIZE as usize + WORLD_HEIGHT as usize);

/// Within the space required to render a chunk section, the offset at which a sprite for the
/// block at `(0, 0, 0)` in section-relative coordinates would be rendered.
const SECTION_ORIGIN: Vec2D<isize> = Vec2D(
    // (0, 0, ?) is at the "back" of the isometric view, and therefore in the middle; take into
    // account the width of the sprite so that the midline of the sprite = midline of the image
    SECTION_RENDER_WIDTH as isize / 2 - SPRITE_SIZE as isize / 2,
    // (0, 0, 15) is at the "top" of the isometric view, and therefore (0, 0, 0), being the 16 block
    // down, is offset downwards by 15x the portion of the sprite that covers the vertical face
    (CHUNK_SIZE as isize - 1) * (SPRITE_SIZE as isize / 2),
);
/// Screen offset for each step east (+X) in section coordinate space.
const SECTION_OFFSET_X: Vec2D<isize> = Vec2D(SPRITE_SIZE as isize / 2, SPRITE_SIZE as isize / 4);
/// Screen offset for each step south (+Z) in section coordinate space.
const SECTION_OFFSET_Z: Vec2D<isize> = Vec2D(-(SPRITE_SIZE as isize / 2), SPRITE_SIZE as isize / 4);
/// Screen offset for each step up (+Y) in section coordinate space.
const SECTION_OFFSET_Y: Vec2D<isize> = Vec2D(0, -(SPRITE_SIZE as isize / 2));

pub struct Renderer<'s> {
    settings: &'s Settings,
    asset_cache: AssetCache<'s>,
}

impl<'s> Renderer<'s> {
    pub fn new(settings: &'s Settings) -> anyhow::Result<Self> {
        let asset_cache = AssetCache::new(settings)?;
        Ok(Self {
            settings,
            asset_cache,
        })
    }

    pub fn render_section_at<I>(
        &self,
        section: &Section,
        output: &mut I,
        x: isize,
        y: isize,
    ) -> anyhow::Result<()>
    where
        I: ImageMut,
        [I::Pixel]: Overlay<[Rgba8]>,
    {
        for block in section.iter_blocks() {
            // Calculate where the sprite for the block would render
            let start = SECTION_ORIGIN
                + SECTION_OFFSET_X * block.index.x() as isize
                + SECTION_OFFSET_Z * block.index.z() as isize
                + SECTION_OFFSET_Y * block.index.y() as isize
                + Vec2D(x, y);
            let end = start + Vec2D(SPRITE_SIZE as isize, SPRITE_SIZE as isize);
            // Skip the block if it would be entirely out-of-bounds
            if end.0 <= 0
                || end.1 <= 0
                || start.0 >= output.width() as isize
                || start.1 >= output.height() as isize
            {
                continue;
            }
            // Try to get a sprite to render for the block
            let Some(asset) = self.asset_cache.get_asset(&block) else {
                continue;
            };
            // Render the sprite into the correct position
            canvas::overlay_final_at(output, &**asset, start.0, start.1);
        }
        Ok(())
    }

    pub fn render_chunk_at<I>(
        &self,
        chunk: &Chunk,
        output: &mut I,
        x: isize,
        y: isize,
    ) -> anyhow::Result<()>
    where
        I: ImageMut,
        [I::Pixel]: Overlay<[Rgba8]>,
    {
        for (i, section) in chunk.sections.iter().enumerate() {
            let y_offset =
                CHUNK_RENDER_HEIGHT - SECTION_RENDER_HEIGHT - (i * SECTION_RENDER_HEIGHT / 2);
            self.render_section_at(section, output, x, y + y_offset as isize)?;
        }
        Ok(())
    }
}
