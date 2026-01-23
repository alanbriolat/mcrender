use std::fs;
use std::fs::File;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{Result, anyhow};
use clap::Parser;
use config::FileFormat;
use image::imageops::FilterType;
use image::{ImageBuffer, Rgba, RgbaImage};
use rayon::prelude::*;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::fmt::format::FmtSpan;

use mcrender::asset::AssetCache;
use mcrender::canvas::Rgb8;
use mcrender::coords::CoordsXZ;
use mcrender::render::{DimensionRenderer, Renderer};
use mcrender::settings::{Settings, convert_rgb};
use mcrender::world::{BIndex, BlockRef, CCoords, DimensionID, RCoords};

#[derive(Debug, clap::Parser)]
struct Cli {
    /// Set `assets_path` configuration option
    #[arg(short, long)]
    assets_path: Option<String>,
    /// Disable color in log output
    #[arg(long, default_value_t = false)]
    no_color: bool,
    /// Don't load builtin configuration
    #[arg(long, default_value_t = false)]
    no_builtin_config: bool,
    /// Don't load configuration at ./mcrender.toml
    #[arg(long, default_value_t = false)]
    no_default_config: bool,
    /// Load additional configuration files
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
        background: Option<Rgb8>,
    },
    RenderRegion {
        source: PathBuf,
        target: PathBuf,
        #[arg(long, default_value = "000000", value_parser = parse_rgb_u8)]
        background: Rgb8,
        #[arg(long, value_parser = parse_coords_xz)]
        coords: CoordsXZ,
        // TODO: dimension
    },
    RenderChunk {
        source: PathBuf,
        target: PathBuf,
        #[arg(long, default_value = "000000", value_parser = parse_rgb_u8)]
        background: Rgb8,
        #[arg(long, value_parser = parse_coords_xz)]
        coords: CoordsXZ,
        // TODO: dimension
    },
    RenderTiles {
        source: PathBuf,
        target: PathBuf,
        #[arg(long, default_value = "000000", value_parser = parse_rgb_u8)]
        background: Rgb8,
        #[arg(long)]
        column: Option<i32>,
        // TODO: dimension
    },
}

fn parse_rgb_u8(s: &str) -> Result<Rgb8, String> {
    let value = u32::from_str_radix(s, 16).map_err(|err| err.to_string())?;
    Ok(convert_rgb(value))
}

fn parse_coords_xz(s: &str) -> Result<CoordsXZ, String> {
    let (raw_x, raw_z) = s.split_once(',').ok_or("expected x,z format")?;
    let x = i32::from_str(raw_x).map_err(|err| err.to_string())?;
    let z = i32::from_str(raw_z).map_err(|err| err.to_string())?;
    Ok(CoordsXZ::new(x, z))
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_span_events(FmtSpan::CLOSE)
        .with_ansi(!cli.no_color)
        .init();
    log::debug!("args: {:?}", cli);

    if cli.no_builtin_config {
        log::warn!("ignoring built-in config");
    } else {
        log::info!("using built-in config");
    }
    let mut builder = Settings::config_builder(cli.no_builtin_config)
        .set_override_option("assets_path", cli.assets_path)?;
    if let Ok(true) = std::fs::exists("mcrender.toml") {
        if cli.no_default_config {
            log::warn!("ignoring default config: ./mcrender.toml");
        } else {
            log::info!("using default config: ./mcrender.toml");
            builder = builder.add_source(config::File::new("mcrender.toml", FileFormat::Toml));
        }
    }
    for config_path in cli.config {
        log::info!("using additional config: {}", &config_path);
        builder = builder.add_source(config::File::new(config_path.as_str(), FileFormat::Toml));
    }
    let config = builder.build()?;
    let settings = Settings::from_config(config)?;
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
            let asset_cache = AssetCache::new(&settings)?;
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

        Commands::RenderRegion {
            source,
            target,
            background,
            coords,
        } => {
            let renderer = Renderer::new(&settings)?;
            let world_info = mcrender::world::WorldInfo::try_from_path(source.clone())?;
            log::debug!("world_info: {:?}", world_info);
            let dim_info = world_info
                .get_dimension(&DimensionID::Overworld)
                .ok_or(anyhow!("no such dimension"))?;
            let dim_renderer = DimensionRenderer::new(dim_info, renderer, *background);
            let coords = RCoords(*coords);
            let image = dim_renderer.render_region(coords)?;
            log::info!("writing output to {:?}", target);
            let output_image = ImageBuffer::from(&image);
            let mut output_file = File::create(target)?;
            output_image.write_to(&mut output_file, image::ImageFormat::Png)?;
        }

        Commands::RenderChunk {
            source,
            target,
            background,
            coords,
        } => {
            let renderer = Renderer::new(&settings)?;
            let world_info = mcrender::world::WorldInfo::try_from_path(source.clone())?;
            log::debug!("world_info: {:?}", world_info);
            let dim_info = world_info
                .get_dimension(&DimensionID::Overworld)
                .ok_or(anyhow!("no such dimension"))?;
            let dim_renderer = DimensionRenderer::new(dim_info, renderer, *background);
            let coords = CCoords(*coords);
            let image = dim_renderer.render_chunk(coords)?;
            log::info!("writing output to {:?}", target);
            let output_image = ImageBuffer::from(&image);
            let mut output_file = File::create(target)?;
            output_image.write_to(&mut output_file, image::ImageFormat::Png)?;
        }

        Commands::RenderTiles {
            source,
            target,
            background,
            column,
        } => {
            let target_dir = target.join("tiles/0");
            let renderer = Renderer::new(&settings)?;
            let world_info = mcrender::world::WorldInfo::try_from_path(source.clone())?;
            log::debug!("world_info: {:?}", world_info);
            let dim_info = world_info
                .get_dimension(&DimensionID::Overworld)
                .ok_or(anyhow!("no such dimension"))?;
            let dim_renderer = DimensionRenderer::new(dim_info, renderer, *background);
            // TODO: make blank-tile.png using background color
            let col_range = match column {
                Some(col) => *col..=*col,
                None => dim_renderer.col_range(),
            };
            col_range.into_par_iter().for_each(|col| {
                // TODO: share a renderer but using RwLock (instead of Mutex) and less lock holding
                //      during asset generation so there's less contention in AssetCache
                let renderer = Renderer::new(&settings).unwrap();
                let dim_renderer = DimensionRenderer::new(dim_info, renderer, *background);
                dim_renderer
                    .render_map_column(col, |coords, image| {
                        let tile_target = target_dir.join(format!("{}/{}.png", coords.0, coords.1));
                        let tile_target_dir = tile_target.parent().unwrap();
                        log::info!(
                            "writing tile ({}, {}) to {:?}",
                            coords.0,
                            coords.1,
                            &tile_target
                        );
                        fs::create_dir_all(&tile_target_dir).unwrap();
                        let output_image = ImageBuffer::from(image);
                        let mut output_file = File::create(tile_target).unwrap();
                        output_image
                            .write_to(&mut output_file, image::ImageFormat::Png)
                            .unwrap();
                        true
                    })
                    .unwrap();
            });
        }
    }

    Ok(())
}
