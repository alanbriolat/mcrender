use std::fs::File;
use std::path::PathBuf;

use anyhow::{Result, anyhow};
use clap::Parser;
use config::FileFormat;
use image::imageops::FilterType;
use image::{ImageBuffer, Rgb, Rgba, RgbaImage};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

use mcrender::asset::AssetCache;
use mcrender::canvas::Image;
use mcrender::render::{DirectoryRenderCache, Renderer};
use mcrender::settings::{Settings, convert_rgb};
use mcrender::world::{BIndex, BlockRef, DimensionID, RCoords};

#[derive(Debug, clap::Parser)]
struct Cli {
    #[arg(short, long)]
    assets: PathBuf,
    #[arg(long, default_value_t = false)]
    no_color: bool,
    #[arg(short, long)]
    config: Vec<String>,

    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, clap::Subcommand)]
enum Commands {
    AssetPreview {
        name: String,
        /// Write image to specified file
        target: PathBuf,
        /// Set a block state property
        #[arg(short, long, value_name = "PROP=VALUE")]
        prop: Vec<String>,
        #[arg(long, default_value = mcrender::asset::DEFAULT_BIOME)]
        biome: String,
        /// Rescale image before display/output
        #[arg(long, default_value_t = 8)]
        scale: u32,
        /// Apply a solid background (to help with image bounds)
        #[arg(long, value_parser = parse_rgb_u8)]
        background: Option<Rgb<u8>>,
    },
    RenderTest {
        source: PathBuf,
        target: PathBuf,
        #[arg(short, long)]
        cache_dir: Option<PathBuf>,
    },
}

fn parse_rgb_u8(s: &str) -> Result<Rgb<u8>, String> {
    let value = u32::from_str_radix(s, 16).map_err(|err| err.to_string())?;
    Ok(convert_rgb(value))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .with_ansi(!cli.no_color)
        .init();
    log::debug!("args: {:?}", cli);

    let mut builder = Settings::config_builder();
    for config_path in cli.config {
        builder = builder.add_source(config::File::new(config_path.as_str(), FileFormat::Toml));
    }
    let config = builder.build()?;
    let settings = Settings::from_config(&config)?;
    // log::debug!("biome_colors: {:#?}", &settings.biome_colors);
    // log::debug!("asset_rules: {:#?}", &settings.asset_rules);

    match &cli.command {
        Commands::AssetPreview {
            name,
            prop,
            biome,
            scale,
            background,
            target,
        } => {
            let mut asset_cache = AssetCache::new(cli.assets.clone(), &settings)?;
            let mut block_state = mcrender::world::BlockState::new(name.into());
            for raw_prop in prop.iter() {
                let Some((key, value)) = raw_prop.split_once("=") else {
                    return Err(anyhow!("invalid --prop argument: {:?}", raw_prop));
                };
                block_state = block_state.with_property(key, value);
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
            let wrapped = ImageBuffer::from(&**asset);
            let mut image = image::imageops::resize(
                &wrapped,
                wrapped.width() * scale,
                wrapped.height() * scale,
                FilterType::Nearest,
            );
            if let Some(background) = background {
                let background_rgba = Rgba([background[0], background[1], background[2], 255]);
                let mut new_image =
                    RgbaImage::from_pixel(image.width(), image.height(), background_rgba);
                image::imageops::overlay(&mut new_image, &image, 0, 0);
                image = new_image;
            }
            log::info!("writing asset to {:?}", target);
            let mut output_file = File::create(target)?;
            image.write_to(&mut output_file, image::ImageFormat::Png)?;
        }

        Commands::RenderTest {
            source,
            target,
            cache_dir,
        } => {
            let asset_cache = AssetCache::new(cli.assets, &settings)?;
            let world_info = mcrender::world::WorldInfo::try_from_path(source.clone())?;
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
