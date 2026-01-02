use std::path::PathBuf;

use crate::asset::AssetCache;
use crate::world::{DimensionID, RCoords};
use anyhow::{Result, anyhow};
use clap::Parser;
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use imageproc::drawing::Canvas;
use imageproc::rect::Rect;
use tracing_subscriber::EnvFilter;

mod asset;
mod world;

#[derive(Debug, Parser)]
struct Args {
    #[arg(short, long)]
    assets: PathBuf,
    #[arg(short, long)]
    source: PathBuf,
    #[arg(short, long)]
    target: PathBuf,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    log::debug!("args: {:?}", args);

    let mut asset_cache = AssetCache::new(args.assets)?;

    let world_info = world::WorldInfo::try_from_path(args.source)?;
    log::debug!("world_info: {:?}", world_info);
    let dim_info = world_info
        .get_dimension(&DimensionID::Overworld)
        .ok_or(anyhow!("no such dimension"))?;
    log::debug!("dim_info: {:?}", dim_info);
    let region_info = dim_info
        .get_region(RCoords { x: 0, z: 0 })
        .ok_or(anyhow!("no such region"))?;
    log::debug!("region_info: {:?}", region_info);
    let raw_chunk = region_info.open()?.into_iter().next().unwrap()?;
    log::debug!("raw_chunk: {:?}", raw_chunk);
    let chunk = raw_chunk.parse()?;
    log::debug!("chunk: {:?}", chunk);

    // Testing asset loading
    let stone_block = world::BlockState::new("minecraft:stone".into());
    let asset = asset_cache
        .get_asset(&stone_block)
        .ok_or(anyhow!("no such asset"))?;

    let image = image::imageops::resize(
        &asset.image,
        asset.image.width() * 8,
        asset.image.height() * 8,
        FilterType::Nearest,
    );
    let mut display_image = RgbaImage::new(image.width() * 2, image.height() * 2);
    let box_rect = Rect::at(0, 0).of_size(display_image.width(), display_image.height());
    imageproc::drawing::draw_filled_rect_mut(&mut display_image, box_rect, Rgba([20, 30, 40, 255]));
    image::imageops::overlay(&mut display_image, &image, image.width() as i64 / 2, 0);
    image::imageops::overlay(
        &mut display_image,
        &image,
        image.width() as i64,
        image.height() as i64 / 4,
    );
    image::imageops::overlay(
        &mut display_image,
        &image,
        image.width() as i64 / 2,
        image.height() as i64 / 2,
    );
    imageproc::window::display_image(
        "blah",
        &display_image,
        display_image.width(),
        display_image.width(),
    );

    Ok(())
}
