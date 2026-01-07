use std::fs::File;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;
use image::imageops::FilterType;
use image::{Rgba, RgbaImage};
use imageproc::rect::Rect;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

use crate::asset::AssetCache;
use crate::render::{DirectoryRenderCache, Renderer};
use crate::world::{BIndex, BlockRef, DimensionID, RCoords};

mod asset;
mod coords;
mod render;
mod world;

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(short, long)]
    assets: PathBuf,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    AssetPreview {
        name: String,
        /// Set a block state property
        #[arg(short, long, value_name = "PROP=VALUE")]
        prop: Vec<String>,
        #[arg(long, default_value = "plains")]
        biome: String,
        /// Rescale image before display/output
        #[arg(long, default_value_t = 8)]
        scale: u32,
        /// Write image to specified file
        #[arg(short, long)]
        target: Option<PathBuf>,
    },
    RenderTest {
        source: PathBuf,
        target: PathBuf,
        #[arg(short, long)]
        cache_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .init();

    let cli = Cli::parse();
    log::debug!("args: {:?}", cli);

    match &cli.command {
        Commands::AssetPreview {
            name,
            prop,
            biome,
            scale,
            target,
        } => {
            let mut asset_cache = AssetCache::new(cli.assets.clone())?;
            let mut block_state = world::BlockState::new(name.into());
            for raw_prop in prop.iter() {
                let Some((key, value)) = raw_prop.split_once("=") else {
                    return Err(anyhow!("invalid --prop argument: {:?}", raw_prop));
                };
                block_state = block_state.with_property(key.to_owned(), value.to_owned());
            }
            let block_ref = BlockRef {
                index: BIndex((0, 0, 0).into()),
                state: &block_state,
                biome,
            };
            // TODO: biome
            let asset = asset_cache
                .get_asset(&block_ref)
                .ok_or(anyhow!("no such asset"))?;
            let image = image::imageops::resize(
                &asset.image,
                asset.image.width() * scale,
                asset.image.height() * scale,
                FilterType::Nearest,
            );
            if let Some(target) = target {
                log::info!("writing asset to {:?}", target);
                let mut output_file = File::create(target)?;
                image.write_to(&mut output_file, image::ImageFormat::Png)?;
            } else {
                log::info!("displaying asset");
                let mut display_image = RgbaImage::new(image.width(), image.height());
                let box_rect =
                    Rect::at(0, 0).of_size(display_image.width(), display_image.height());
                imageproc::drawing::draw_filled_rect_mut(
                    &mut display_image,
                    box_rect,
                    Rgba([20, 30, 40, 255]),
                );
                image::imageops::overlay(&mut display_image, &image, 0, 0);
                imageproc::window::display_image(
                    "asset-preview",
                    &display_image,
                    display_image.width(),
                    display_image.width(),
                );
            }
        }

        Commands::RenderTest {
            source,
            target,
            cache_dir,
        } => {
            let asset_cache = AssetCache::new(cli.assets)?;
            let world_info = world::WorldInfo::try_from_path(source.clone())?;
            log::debug!("world_info: {:?}", world_info);
            let dim_info = world_info
                .get_dimension(&DimensionID::Overworld)
                .ok_or(anyhow!("no such dimension"))?;
            // log::debug!("dim_info: {:?}", dim_info);
            let region_info = dim_info
                .get_region(RCoords((0, 0).into()))
                .ok_or(anyhow!("no such region"))?;
            // log::debug!("region_info: {:?}", region_info);
            let raw_chunk = region_info.open()?.into_iter().next().unwrap()?;
            // log::debug!("raw_chunk: {:?}", raw_chunk);
            let chunk = raw_chunk.parse()?;
            // log::debug!("chunk: {:?}", chunk);
            let mut renderer = Renderer::new(asset_cache);
            if let Some(cache_dir) = cache_dir {
                renderer.set_render_cache(DirectoryRenderCache::new(cache_dir.clone())?);
            }
            let image = renderer.get_chunk(&raw_chunk)?;
            // let image = renderer.get_region(&region_info)?;

            let mut output_file = File::create(target)?;
            image.write_to(&mut output_file, image::ImageFormat::Png)?;
        }

        _ => unimplemented!(),
    }

    Ok(())
}
