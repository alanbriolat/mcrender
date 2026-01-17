use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufWriter;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Mutex;

use image::{ImageReader, RgbaImage};

use crate::asset::{AssetCache, SPRITE_SIZE};
use crate::canvas;
use crate::canvas::{ImageBuf, Rgba8};
use crate::coords::{PointXZY, Vec2D};
use crate::world::{CCoords, CHUNK_SIZE, REGION_SIZE, RawChunk, RegionInfo, WORLD_HEIGHT};

const CHUNK_TILE_MAP: TileMap = TileMap::new(
    PointXZY::new(CHUNK_SIZE, CHUNK_SIZE, WORLD_HEIGHT),
    SPRITE_SIZE,
);
const REGION_TILE_MAP: TileMap = TileMap::new(
    PointXZY::new(
        CHUNK_SIZE * REGION_SIZE,
        CHUNK_SIZE * REGION_SIZE,
        WORLD_HEIGHT,
    ),
    SPRITE_SIZE,
);
const BLOCK_COUNT_SINGLE: PointXZY<u32> = PointXZY::new(1, 1, 1);
const BLOCK_COUNT_CHUNK: PointXZY<u32> = PointXZY::new(CHUNK_SIZE, CHUNK_SIZE, WORLD_HEIGHT);

pub struct Renderer<'s> {
    asset_cache: AssetCache<'s>,
}

impl<'s> Renderer<'s> {
    pub fn new(asset_cache: AssetCache<'s>) -> Self {
        Self { asset_cache }
    }

    #[tracing::instrument(skip_all, fields(coords = %raw_chunk.coords))]
    pub fn render_chunk(&mut self, raw_chunk: &RawChunk) -> anyhow::Result<ImageBuf<Rgba8>> {
        let chunk = raw_chunk.parse()?;
        let mut output = ImageBuf::<Rgba8>::from_pixel(
            CHUNK_TILE_MAP.image_size.0 as usize,
            CHUNK_TILE_MAP.image_size.1 as usize,
            [0, 0, 0, 0].into(),
        );
        for block in chunk.iter_blocks() {
            let Some(asset) = self.asset_cache.get_asset(&block) else {
                continue;
            };
            let (output_x, output_y) =
                CHUNK_TILE_MAP.tile_position(block.index.into(), BLOCK_COUNT_SINGLE);
            canvas::overlay_at(&mut output, &**asset, output_x as isize, output_y as isize);
        }
        Ok(output)
    }

    #[tracing::instrument(skip_all, fields(coords = %region_info.coords))]
    pub fn render_region(&mut self, region_info: &RegionInfo) -> anyhow::Result<ImageBuf<Rgba8>> {
        let region = region_info.open()?;
        let mut output = ImageBuf::<Rgba8>::from_pixel(
            REGION_TILE_MAP.image_size.0 as usize,
            REGION_TILE_MAP.image_size.1 as usize,
            [0, 0, 0, 0].into(),
        );
        for raw_chunk in region.into_iter() {
            let raw_chunk = raw_chunk?;
            let chunk_output = self.render_chunk(&raw_chunk)?;
            let (output_x, output_y) = REGION_TILE_MAP.tile_position(
                PointXZY::new(
                    raw_chunk.index.x() * CHUNK_SIZE,
                    raw_chunk.index.z() * CHUNK_SIZE,
                    0,
                ),
                BLOCK_COUNT_CHUNK,
            );
            canvas::overlay_at(
                &mut output,
                &chunk_output,
                output_x as isize,
                output_y as isize,
            );
        }
        Ok(output.into())
    }
}

struct TileMap {
    image_size: Vec2D<u32>,
    origin: Vec2D<i64>,
    x_offset: Vec2D<i64>,
    z_offset: Vec2D<i64>,
    y_offset: Vec2D<i64>,
    t_offset: Vec2D<i64>,
}

impl TileMap {
    /// Create a new tile map for a group of blocks (defined by `count`) based on a square sprite
    /// of `tile_size`.
    const fn new(count: PointXZY<u32>, tile_size: u32) -> Self {
        let image_size = Vec2D(
            // Screen X coordinate of right-most edge <X=x Z=0 Y=...>
            (tile_size / 2) * (count.x() + count.z()),
            // Screen Y coordinate of bottom-most edge <X=x Z=z Y=0>
            (tile_size / 4) * (count.x() + count.z()) + (tile_size / 2) * count.y(),
        );
        let tile_size = tile_size as i64;
        // Screen coords of bottom-north-west (0, 0, 0) of block coordinate space
        let origin = Vec2D(
            (tile_size / 2) * count.z() as i64,
            (tile_size / 2) * count.y() as i64,
        );
        // Screen offset for each step east (+X) in block coordinate space
        let x_offset = Vec2D(tile_size / 2, tile_size / 4);
        // Screen offset for each step south (+Z) in block coordinate space
        let z_offset = Vec2D(-(tile_size / 2), tile_size / 4);
        // Screen offset for each step up (+Y) in block coordinate space
        let y_offset = Vec2D(0, -(tile_size / 2));
        // Screen offset from part of tile that represents the origin of a block to the top-left of the tile
        let t_offset = Vec2D(-(tile_size / 2), -(tile_size / 2));
        TileMap {
            origin,
            image_size,
            x_offset,
            z_offset,
            y_offset,
            t_offset,
        }
    }

    /// Get the top-left screen position to render at for a group of blocks (defined by `count`)
    /// at `coords` relative to the origin of the tile map.
    fn tile_position(&self, coords: PointXZY<u32>, count: PointXZY<u32>) -> (i64, i64) {
        let (x, z, y) = (coords.x() as i64, coords.z() as i64, coords.y() as i64);
        // Calculate screen coords of bottom-north-west of the group of blocks
        let coords_offset = (self.x_offset * x) + (self.z_offset * z) + (self.y_offset * y);
        // Scale up the tile offset based on the number of blocks being rendered
        let t_offset = self.t_offset * Vec2D(count.z() as i64, count.y() as i64);
        // Calculate the final position
        (self.origin + coords_offset + t_offset).into()
    }
}
