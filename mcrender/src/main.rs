use anyhow::{Result, anyhow};
use clap::Parser;
use std::fs::File;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::asset::AssetCache;
use crate::render::Renderer;
use crate::world::{DimensionID, RCoords};

mod asset;
mod render;
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
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let args = Args::parse();
    log::debug!("args: {:?}", args);

    let asset_cache = AssetCache::new(args.assets)?;

    let world_info = world::WorldInfo::try_from_path(args.source)?;
    log::debug!("world_info: {:?}", world_info);
    let dim_info = world_info
        .get_dimension(&DimensionID::Overworld)
        .ok_or(anyhow!("no such dimension"))?;
    // log::debug!("dim_info: {:?}", dim_info);
    let region_info = dim_info
        .get_region(RCoords { x: 0, z: 0 })
        .ok_or(anyhow!("no such region"))?;
    // log::debug!("region_info: {:?}", region_info);
    let raw_chunk = region_info.open()?.into_iter().next().unwrap()?;
    // log::debug!("raw_chunk: {:?}", raw_chunk);
    let chunk = raw_chunk.parse()?;
    // log::debug!("chunk: {:?}", chunk);

    let mut renderer = Renderer::new(asset_cache);
    // let image = renderer.render_chunk(&chunk)?;
    let image = renderer.render_region(&region_info)?;

    let mut output_file = File::create(args.target.join("mcrender-output.png"))?;
    image.write_to(&mut output_file, image::ImageFormat::Png)?;

    Ok(())
}
