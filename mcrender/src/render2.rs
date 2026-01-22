use crate::asset::{AssetCache, SPRITE_SIZE};
use crate::canvas;
use crate::canvas::{ImageBuf, ImageMut, Overlay, Pixel, Rgb8, Rgba, Rgba8};
use crate::coords::{CoordsXZ, PointXZY, Vec2D};
use crate::settings::Settings;
use crate::world::{CCoords, CHUNK_SIZE, Chunk, DimensionInfo, Section, WORLD_HEIGHT};

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

/// Screen offset for each chunk step east (+X).
const CHUNK_OFFSET_X: Vec2D<isize> = Vec2D(
    SECTION_RENDER_WIDTH as isize / 2,
    SECTION_RENDER_HEIGHT as isize / 4,
);
/// Screen offset for each chunk step south (+Z).
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

    pub fn render_tiles(
        &self,
        dim_info: &DimensionInfo,
        background: Rgb8,
    ) -> impl Iterator<Item = (isize, isize, ImageBuf<Rgba8>)> {
        const BUFFER_WIDTH: usize = CHUNK_RENDER_WIDTH;
        const BUFFER_HEIGHT: usize = CHUNK_RENDER_HEIGHT + 3 * (SECTION_RENDER_HEIGHT / 4);
        const BUFFER_LEN_PIXELS: usize = BUFFER_WIDTH * BUFFER_HEIGHT;
        const BUFFER_SPLIT_PIXELS: usize = BUFFER_WIDTH * SECTION_RENDER_HEIGHT;
        const BUFFER_SPLIT_CHANNELS: usize = BUFFER_SPLIT_PIXELS * <Rgba8 as Pixel>::CHANNELS;
        let background = background.to_rgba();
        let min_chunk = dim_info.min_region_coords().to_chunk_coords();
        let max_chunk = dim_info.max_region_coords().to_chunk_coords();
        let min_row = (min_chunk.x() + min_chunk.z()) / 4;
        // TODO: add extra for rendering height of front-most chunks
        let max_row = (max_chunk.x() + max_chunk.z()) / 4
            + (CHUNK_RENDER_HEIGHT / SECTION_RENDER_HEIGHT) as i32;
        let min_col = (min_chunk.x() - max_chunk.z()) / 2;
        let max_col = (max_chunk.x() - min_chunk.z()) / 2;
        log::debug!(
            "map tile extent: ({}, {}) to ({}, {})",
            min_col,
            min_row,
            max_col,
            max_row
        );
        (min_col..=max_col).flat_map(move |col| {
            let mut buffer = ImageBuf::<Rgba8>::from_pixel(BUFFER_WIDTH, BUFFER_HEIGHT, background);
            (min_row..=max_row).map(move |row| {
                // Figure out the chunk coords of the next 6 chunks that need to be rendered
                // to cover the next tile down the column
                let anchor = CoordsXZ::new(2 * row + col, 2 * row - col);
                let offsets: [CoordsXZ; _] = [
                    (0, 0).into(),
                    (1, 0).into(),
                    (0, 1).into(),
                    (1, 1).into(),
                    (2, 1).into(),
                    (1, 2).into(),
                ];
                // Render each chunk of the 6 chunks, if they exist
                for offset in offsets {
                    let image_offset =
                        CHUNK_OFFSET_X * offset.x() as isize + CHUNK_OFFSET_Z * offset.z() as isize;
                    let coords = anchor + offset;
                    let Some(raw_chunk) = dim_info.get_raw_chunk(CCoords(coords)).unwrap() else {
                        continue;
                    };
                    let Ok(chunk) = raw_chunk.parse() else {
                        log::error!("failed to parse chunk {coords}");
                        continue;
                    };
                    self.render_chunk_at(&chunk, &mut buffer, image_offset.0, image_offset.1)
                        .unwrap();
                }
                // TODO: optimise out tiles that don't show anything
                // Create tile image from top section of buffer
                let raw = Vec::from(&buffer.channels()[0..BUFFER_SPLIT_CHANNELS]);
                let image =
                    ImageBuf::from_raw(SECTION_RENDER_WIDTH, SECTION_RENDER_HEIGHT, raw).unwrap();
                // Shift the buffer up to prepare for next tile down
                buffer
                    .channels_mut()
                    .copy_within(BUFFER_SPLIT_CHANNELS.., 0);
                buffer.pixels_mut()[BUFFER_LEN_PIXELS - BUFFER_SPLIT_PIXELS..].fill(background);
                // Yield the tile and its coordinates
                (row as isize, col as isize, image)
            })
        })
    }
}
