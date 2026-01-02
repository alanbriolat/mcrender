use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::world::{DimensionID, RCoords};

mod world;

#[derive(Debug, Parser)]
struct Args {
    source_dir: PathBuf,
    target_dir: PathBuf,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    log::debug!("args: {:?}", args);

    let world_info = world::WorldInfo::try_from_path(args.source_dir)?;
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

    Ok(())
}
