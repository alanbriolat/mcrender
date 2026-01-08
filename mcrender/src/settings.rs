use std::collections::HashMap;

use config::builder::DefaultState;
use config::{Config, ConfigBuilder, File, FileFormat};
use image::Rgb;
use serde::Deserialize;

#[derive(Debug, Default, Deserialize)]
struct RawColorEntry {
    color: u32,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RawColorMap(HashMap<String, RawColorEntry>);

#[derive(Debug)]
pub struct BiomeColorMap {
    default: Rgb<u8>,
    biomes: HashMap<String, Rgb<u8>>,
}

impl BiomeColorMap {
    fn from_raw(mut raw: RawColorMap) -> anyhow::Result<BiomeColorMap> {
        let Some(raw_default) = raw.0.remove("_default") else {
            return Err(anyhow::anyhow!(
                "missing _default.color in biome_colors.<kind> config"
            ));
        };
        let default = convert_rgb(raw_default.color);
        let mut biomes = HashMap::new();
        for (biome, RawColorEntry { color, aliases }) in raw.0.into_iter() {
            let color = convert_rgb(color);
            biomes.insert(biome, color);
            for alias in aliases.into_iter() {
                biomes.insert(alias, color);
            }
        }
        Ok(BiomeColorMap { default, biomes })
    }

    pub fn get(&self, biome: &str) -> Rgb<u8> {
        self.biomes.get(biome).cloned().unwrap_or(self.default)
    }
}

#[derive(Debug)]
pub struct BiomeColors {
    pub grass: BiomeColorMap,
    pub foliage: BiomeColorMap,
    pub dry_foliage: BiomeColorMap,
    pub water: BiomeColorMap,
}

#[derive(Debug)]
pub struct Settings {
    pub biome_colors: BiomeColors,
}

impl Settings {
    pub fn config_builder() -> ConfigBuilder<DefaultState> {
        Config::builder().add_source(File::from_str(
            include_str!("settings_default.toml"),
            FileFormat::Toml,
        ))
    }
    pub fn from_config(config: &Config) -> anyhow::Result<Settings> {
        Ok(Settings {
            biome_colors: BiomeColors {
                grass: BiomeColorMap::from_raw(config.get("biome_colors.grass")?)?,
                foliage: BiomeColorMap::from_raw(config.get("biome_colors.foliage")?)?,
                dry_foliage: BiomeColorMap::from_raw(config.get("biome_colors.dry_foliage")?)?,
                water: BiomeColorMap::from_raw(config.get("biome_colors.water")?)?,
            },
        })
    }
}

const fn convert_rgb(raw: u32) -> Rgb<u8> {
    Rgb([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}
