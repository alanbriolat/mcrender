use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::anyhow;
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};

use crate::world::BlockState;

pub const TILE_SIZE: u32 = 24;

pub struct AssetCache {
    path: PathBuf,
    cache: HashMap<BlockState, Option<Arc<Asset>>>,
}

impl AssetCache {
    pub fn new(path: PathBuf) -> anyhow::Result<AssetCache> {
        if !path.is_dir() || !path.join(".mcassetsroot").exists() {
            Err(anyhow::anyhow!("not a minecraft assets dir"))
        } else {
            Ok(AssetCache {
                path,
                cache: HashMap::new(),
            })
        }
    }

    pub fn get_asset(&mut self, block_state: &BlockState) -> Option<Arc<Asset>> {
        if let Some(asset) = self.cache.get(block_state) {
            return asset.clone();
        }
        let asset = match self.create_asset(block_state) {
            Ok(asset) => Some(Arc::new(asset)),
            Err(err) => {
                log::error!("failed to create asset for {:#?}: {}", block_state, err);
                None
            }
        };
        self.cache.insert(block_state.clone(), asset.clone());
        asset
    }

    fn create_asset(&self, block_state: &BlockState) -> anyhow::Result<Asset> {
        self.create_simple_block_asset(&block_state.name)
    }

    fn create_simple_block_asset(&self, name: &str) -> anyhow::Result<Asset> {
        let (_, block_name) = name.split_once(":").ok_or(anyhow!("invalid block name"))?;
        let texture_path = self
            .path
            .join("minecraft/textures/block")
            .join(block_name)
            .with_added_extension("png");
        let image = image::open(texture_path)?.to_rgba8();
        let top = transform_top_texture(&image);
        let side = transform_side_texture(&image);
        let left = image::imageops::brighten(&side, -25);
        let right = image::imageops::brighten(&image::imageops::flip_horizontal(&side), -40);
        let mut output = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        image::imageops::overlay(&mut output, &right, 12, 6);
        image::imageops::overlay(&mut output, &left, 0, 6);
        image::imageops::overlay(&mut output, &top, 0, 0);
        Ok(Asset { image: output })
    }
}

// Methodology copied from Minecraft-Overviewer's Textures.transform_image_top()
fn transform_top_texture(top: &RgbaImage) -> RgbaImage {
    let img = image::imageops::resize(top, 17, 17, FilterType::Lanczos3);
    let mut output = RgbaImage::new(TILE_SIZE, TILE_SIZE / 2);
    let projection = [
        Projection::translate(-8.5, -8.5),
        Projection::rotate(45f32.to_radians()),
        Projection::translate(12., 12.),
        Projection::scale(1.0, 0.5),
    ]
    .into_iter()
    .reduce(|acc, item| item * acc)
    .unwrap();
    warp_into(
        &img,
        &projection,
        Interpolation::Nearest,
        Rgba([0, 0, 0, 0]),
        &mut output,
    );
    output
}

// Methodology copied from Minecraft-Overviewer's Textures.transform_image_side()
fn transform_side_texture(side: &RgbaImage) -> RgbaImage {
    let img = image::imageops::resize(side, 13, 13, FilterType::Lanczos3);
    let mut output = RgbaImage::new(TILE_SIZE / 2, (TILE_SIZE * 3) / 4);
    let projection = Projection::from_matrix([1., 0., 0., 0.5, 1., 0., 0., 0., 1.]).unwrap();
    warp_into(
        &img,
        &projection,
        Interpolation::Nearest,
        Rgba([0, 0, 0, 0]),
        &mut output,
    );
    output
}

pub struct Asset {
    pub image: RgbaImage,
}
