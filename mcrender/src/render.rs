use std::ops::RangeInclusive;

use anyhow::anyhow;

use crate::asset::{AssetCache, SPRITE_SIZE};
use crate::canvas;
use crate::canvas::{ImageBuf, ImageMut, Overlay, Pixel, Rgba8};
use crate::coords::{CoordsXZ, Vec2D};
use crate::settings::Settings;
use crate::world::{
    CCoords, CHUNK_SIZE, Chunk, DimensionInfo, RCoords, REGION_SIZE, Section, WORLD_HEIGHT,
};

/// Get the image width required to render an `x`-by-`z` area of blocks (regardless of how tall).
///
/// In isometric view, with `(0, 0)` in the center of the image, each extra block in the `x` or `z`
/// direction is offset by half a sprite width horizontally.
pub const fn render_width(x: usize, z: usize) -> usize {
    (x + z) * (SPRITE_SIZE / 2)
}

/// Get the image height required to render an `x`-by-`z` area of blocks with height `y`.
///
/// In isometric view, with `(0, 0)` at the top-center of the image, each extra block in the `x` or
/// `z` direction is offset by a quarter of a sprite height vertically; only the top half of the
/// sprite represents the top of the block, and each step is offset by half the top face in both
/// directions. Each block in the `y` direction adds half the sprite height, i.e. the portion that
/// corresponds to the side of the block.
pub const fn render_height(x: usize, z: usize, y: usize) -> usize {
    let top = (x + z) * (SPRITE_SIZE / 4);
    let vertical = y * (SPRITE_SIZE / 2);
    top + vertical
}

/// The image width required to fully render a chunk section (16x16 blocks horizontally).
pub const SECTION_RENDER_WIDTH: usize = render_width(CHUNK_SIZE as usize, CHUNK_SIZE as usize);
/// The image height required to fully render a chunk section (16x16x16 blocks).
pub const SECTION_RENDER_HEIGHT: usize = render_height(
    CHUNK_SIZE as usize,
    CHUNK_SIZE as usize,
    CHUNK_SIZE as usize,
);

/// The image width required to fully render a chunk. The same as for a section.
pub const CHUNK_RENDER_WIDTH: usize = SECTION_RENDER_WIDTH;
/// The image height required to fully render a chunk (16x16x384 blocks).
pub const CHUNK_RENDER_HEIGHT: usize = render_height(
    CHUNK_SIZE as usize,
    CHUNK_SIZE as usize,
    WORLD_HEIGHT as usize,
);

/// Within the space required to render a chunk section, the offset at which a sprite for the
/// block at `(0, 0, 0)` (west-north-bottom) in section-relative block coordinates (block index)
/// would be rendered.
const SECTION_ORIGIN: Vec2D<isize> = Vec2D(
    // (0, 0, ?) is at the "back" of the isometric view, and therefore in the middle; take into
    // account the width of the sprite so that the midline of the sprite = midline of the image
    SECTION_RENDER_WIDTH as isize / 2 - SPRITE_SIZE as isize / 2,
    // (0, 0, 15) is at the "top" of the isometric view, and therefore (0, 0, 0), being the 16 block
    // down, is offset downwards by 15x the portion of the sprite that covers the vertical face
    (CHUNK_SIZE as isize - 1) * (SPRITE_SIZE as isize / 2),
);
/// Screen offset for each step east (+X) in block coordinate space.
const BLOCK_OFFSET_X: Vec2D<isize> = Vec2D(SPRITE_SIZE as isize / 2, SPRITE_SIZE as isize / 4);
/// Screen offset for each step south (+Z) in block coordinate space.
const BLOCK_OFFSET_Z: Vec2D<isize> = Vec2D(-(SPRITE_SIZE as isize / 2), SPRITE_SIZE as isize / 4);
/// Screen offset for each step up (+Y) in block coordinate space.
const BLOCK_OFFSET_Y: Vec2D<isize> = Vec2D(0, -(SPRITE_SIZE as isize / 2));

/// Screen offset for each step east (+X) in chunk coordinate space.
const CHUNK_OFFSET_X: Vec2D<isize> = Vec2D(
    SECTION_RENDER_WIDTH as isize / 2,
    SECTION_RENDER_HEIGHT as isize / 4,
);
/// Screen offset for each step south (+Z) in chunk coordinate space.
const CHUNK_OFFSET_Z: Vec2D<isize> = Vec2D(
    -(SECTION_RENDER_WIDTH as isize / 2),
    SECTION_RENDER_HEIGHT as isize / 4,
);

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
                + BLOCK_OFFSET_X * block.index.x() as isize
                + BLOCK_OFFSET_Z * block.index.z() as isize
                + BLOCK_OFFSET_Y * block.index.y() as isize
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

    #[tracing::instrument(level = "debug", skip_all, fields(coords = %chunk.coords))]
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

pub struct DimensionRenderer<'i, 's> {
    dim_info: &'i DimensionInfo,
    renderer: Renderer<'s>,
    col_range: RangeInclusive<i32>,
    row_range: RangeInclusive<i32>,
}

impl<'i, 's> DimensionRenderer<'i, 's> {
    pub fn new(dim_info: &'i DimensionInfo, renderer: Renderer<'s>) -> Self {
        let min_chunk = dim_info.min_region_coords().to_chunk_coords();
        let max_chunk = dim_info.max_region_coords().to_chunk_coords();
        let min_row = (min_chunk.x() + min_chunk.z()) / 4;
        let max_row = (max_chunk.x() + max_chunk.z()) / 4
            + (CHUNK_RENDER_HEIGHT / SECTION_RENDER_HEIGHT) as i32;
        let min_col = (min_chunk.x() - max_chunk.z()) / 2;
        let max_col = (max_chunk.x() - min_chunk.z()) / 2;

        Self {
            dim_info,
            renderer,
            col_range: min_col..=max_col,
            row_range: min_row..=max_row,
        }
    }

    pub fn col_range(&self) -> RangeInclusive<i32> {
        self.col_range.clone()
    }

    pub fn row_range(&self) -> RangeInclusive<i32> {
        self.row_range.clone()
    }

    /// Consume the `MapRenderer` and return the wrapped `Renderer`.
    pub fn into_renderer(self) -> Renderer<'s> {
        self.renderer
    }

    const TILE_BUFFER_WIDTH: usize = CHUNK_RENDER_WIDTH;
    const TILE_BUFFER_HEIGHT: usize = CHUNK_RENDER_HEIGHT + 3 * (SECTION_RENDER_HEIGHT / 4);
    const TILE_BUFFER_LEN_PIXELS: usize = Self::TILE_BUFFER_WIDTH * Self::TILE_BUFFER_HEIGHT;
    const TILE_BUFFER_SPLIT_PIXELS: usize = Self::TILE_BUFFER_WIDTH * SECTION_RENDER_HEIGHT;
    const TILE_BUFFER_SPLIT_CHANNELS: usize =
        Self::TILE_BUFFER_SPLIT_PIXELS * <Rgba8 as Pixel>::CHANNELS;

    const TILE_RENDER_CHUNK_OFFSETS: [CoordsXZ; 6] = [
        CoordsXZ::new(0, 0),
        CoordsXZ::new(1, 0),
        CoordsXZ::new(0, 1),
        CoordsXZ::new(1, 1),
        CoordsXZ::new(2, 1),
        CoordsXZ::new(1, 2),
    ];

    #[tracing::instrument(level = "debug", skip_all, fields(col = %col))]
    pub fn render_map_column<F>(&self, col: i32, f: F) -> anyhow::Result<()>
    where
        F: Fn(Vec2D<i32>, &ImageBuf<Rgba8, &[u8]>) -> bool,
    {
        let background = self.renderer.settings.background_color.to_rgba();
        let mut buffer = ImageBuf::<Rgba8>::from_pixel(
            Self::TILE_BUFFER_WIDTH,
            Self::TILE_BUFFER_HEIGHT,
            background,
        );

        for row in self.row_range() {
            // Figure out the chunk coords of the next 6 chunks that need to be rendered
            // to cover the next tile down the column, and render them if they exist
            let anchor = CoordsXZ::new(2 * row + col, 2 * row - col);
            for offset in Self::TILE_RENDER_CHUNK_OFFSETS.iter().copied() {
                let image_offset =
                    CHUNK_OFFSET_X * offset.x() as isize + CHUNK_OFFSET_Z * offset.z() as isize;
                let coords = anchor + offset;
                let Some(raw_chunk) = self.dim_info.get_raw_chunk(CCoords(coords)).unwrap() else {
                    continue;
                };
                let Ok(chunk) = raw_chunk.parse() else {
                    log::error!("failed to parse chunk {coords}");
                    continue;
                };
                if !chunk.fully_generated {
                    continue;
                }
                self.renderer
                    .render_chunk_at(&chunk, &mut buffer, image_offset.0, image_offset.1)
                    .unwrap();
            }

            // TODO: optimise out tiles that don't show anything
            // Create tile image from top section of buffer
            let image = ImageBuf::from_raw(
                Self::TILE_BUFFER_WIDTH,
                SECTION_RENDER_HEIGHT,
                &buffer.channels()[..Self::TILE_BUFFER_SPLIT_CHANNELS],
            )
            .unwrap();
            // Pass the tile to the callback
            let keep_rendering = f((col, row).into(), &image);
            if !keep_rendering {
                // Stop rendering if the callback said they're done
                break;
            }
            // Shift the buffer up to prepare for next tile down
            buffer
                .channels_mut()
                .copy_within(Self::TILE_BUFFER_SPLIT_CHANNELS.., 0);
            buffer.pixels_mut()[Self::TILE_BUFFER_LEN_PIXELS - Self::TILE_BUFFER_SPLIT_PIXELS..]
                .fill(background);
        }

        Ok(())
    }

    const REGION_SIZE_BLOCKS: usize = (REGION_SIZE * CHUNK_SIZE) as usize;
    const REGION_RENDER_WIDTH: usize =
        render_width(Self::REGION_SIZE_BLOCKS, Self::REGION_SIZE_BLOCKS);
    const REGION_RENDER_HEIGHT: usize = render_height(
        Self::REGION_SIZE_BLOCKS,
        Self::REGION_SIZE_BLOCKS,
        WORLD_HEIGHT as usize,
    );
    const REGION_ORIGIN: Vec2D<isize> = Vec2D(
        Self::REGION_RENDER_WIDTH as isize / 2 - CHUNK_RENDER_WIDTH as isize / 2,
        0,
    );

    #[tracing::instrument(level = "debug", skip_all, fields(coords = %coords))]
    pub fn render_region(&self, coords: RCoords) -> anyhow::Result<ImageBuf<Rgba8>> {
        let mut output = ImageBuf::from_pixel(
            Self::REGION_RENDER_WIDTH,
            Self::REGION_RENDER_HEIGHT,
            self.renderer.settings.background_color.to_rgba(),
        );
        let region_info = self
            .dim_info
            .get_region(coords)
            .ok_or(anyhow!("no such region"))?;
        let region = region_info.open()?;
        for raw_chunk in region.into_iter() {
            let raw_chunk = raw_chunk?;
            let image_offset = Self::REGION_ORIGIN
                + CHUNK_OFFSET_X * raw_chunk.index.x() as isize
                + CHUNK_OFFSET_Z * raw_chunk.index.z() as isize;
            let chunk = raw_chunk.parse()?;
            self.renderer
                .render_chunk_at(&chunk, &mut output, image_offset.0, image_offset.1)?;
        }
        Ok(output)
    }

    #[tracing::instrument(level = "debug", skip_all, fields(coords = %coords))]
    pub fn render_chunk(&self, coords: CCoords) -> anyhow::Result<ImageBuf<Rgba8>> {
        let mut output = ImageBuf::from_pixel(
            CHUNK_RENDER_WIDTH,
            CHUNK_RENDER_HEIGHT,
            self.renderer.settings.background_color.to_rgba(),
        );
        let raw_chunk = self
            .dim_info
            .get_raw_chunk(coords)?
            .ok_or(anyhow!("no such chunk"))?;
        let chunk = raw_chunk.parse()?;
        self.renderer.render_chunk_at(&chunk, &mut output, 0, 0)?;
        Ok(output)
    }
}
