use image::RgbaImage;

use crate::asset::{AssetCache, TILE_SIZE};
use crate::world::{CHUNK_SIZE, Chunk, REGION_SIZE, RegionInfo};

pub struct Renderer {
    asset_cache: AssetCache,
}

impl Renderer {
    pub fn new(asset_cache: AssetCache) -> Self {
        Self { asset_cache }
    }

    #[tracing::instrument(skip_all)]
    pub fn render_chunk(&mut self, chunk: &Chunk) -> anyhow::Result<RgbaImage> {
        let chunk_height = (chunk.sections.len() * CHUNK_SIZE) as u32;
        let tile_map = TileMap::new(
            CHUNK_SIZE as u32,
            CHUNK_SIZE as u32,
            chunk_height,
            TILE_SIZE,
        );
        let mut output = RgbaImage::new(tile_map.width, tile_map.height);
        for (bindex, block_state) in chunk.iter_blocks() {
            let Some(asset) = self.asset_cache.get_asset(block_state) else {
                continue;
            };
            let (output_x, output_y) =
                tile_map.tile_position(bindex.x as u32, bindex.z as u32, bindex.y as u32);
            image::imageops::overlay(&mut output, &asset.image, output_x, output_y);
        }

        Ok(output)
    }
}

struct TileMap {
    tile_size: u32,
    width: u32,
    height: u32,
    origin_x: u32,
    origin_bottom_y: u32,
}

impl TileMap {
    fn new(x_blocks: u32, z_blocks: u32, y_blocks: u32, tile_size: u32) -> Self {
        let width = (tile_size / 2) * (x_blocks + z_blocks);
        let height = (tile_size / 2) * y_blocks + (tile_size / 4) * (x_blocks + z_blocks);
        let origin_x = (tile_size / 2) * (z_blocks - 1);
        let origin_bottom_y = (tile_size / 2) * (y_blocks - 1);
        TileMap {
            tile_size,
            width,
            height,
            origin_x,
            origin_bottom_y,
        }
    }

    fn tile_position(&self, x: u32, z: u32, y: u32) -> (i64, i64) {
        let output_x = self.origin_x as i64 + (self.tile_size as i64 / 2) * (x as i64 - z as i64);
        let output_y = self.origin_bottom_y as i64 - (self.tile_size as i64 / 2) * y as i64
            + (self.tile_size as i64 / 4) * (x as i64 + z as i64);
        (output_x, output_y)
    }
}
