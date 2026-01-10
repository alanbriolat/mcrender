use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use config::builder::DefaultState;
use config::{Config, ConfigBuilder, File, FileFormat};
use image::Rgb;
use serde::{Deserialize, Deserializer};

use crate::asset::AssetInfo;
use crate::world::BlockRef;

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum AssetRenderSpec {
    Nothing,
    SolidUniform {
        #[serde(default)]
        texture: AssetStringBuilder,
    },
    SolidTopSide {
        top_texture: AssetStringBuilder,
        side_texture: AssetStringBuilder,
    },
    Leaves {
        #[serde(default)]
        texture: AssetStringBuilder,
        tint_color: TintColor,
    },
    Plant {
        #[serde(default)]
        texture: AssetStringBuilder,
        tint_color: Option<TintColor>,
    },
    Crop {
        #[serde(default)]
        texture: AssetStringBuilder,
    },
    Grass {
        tint_color: TintColor,
    },
    Vine {
        tint_color: Option<TintColor>,
    },
    Water {
        tint_color: TintColor,
    },
}

impl AssetRenderSpec {
    pub fn is_biome_aware(&self) -> bool {
        match self {
            // Optional tint_color
            AssetRenderSpec::Plant { tint_color, .. }
            | AssetRenderSpec::Vine { tint_color, .. } => tint_color
                .as_ref()
                .map(|c| c.is_biome_aware())
                .unwrap_or(false),
            // Required tint_color
            AssetRenderSpec::Leaves { tint_color, .. }
            | AssetRenderSpec::Grass { tint_color, .. }
            | AssetRenderSpec::Water { tint_color, .. } => tint_color.is_biome_aware(),
            // Others
            _ => false,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStringComponent {
    Name,
    Literal(String),
    Property(String),
    PropertyMap {
        name: String,
        values: BTreeMap<String, String>,
    },
}

impl AssetStringComponent {
    pub fn apply<'a>(&'a self, info: &'a AssetInfo) -> &'a str {
        use AssetStringComponent::*;

        match self {
            Name => info.short_name(),
            Literal(literal) => literal.as_str(),
            Property(name) => info.get(name).unwrap(),
            PropertyMap { name, values } => {
                if let Some(prop_value) = info.get(name) {
                    if let Some(mapped_value) = values.get(prop_value) {
                        return mapped_value.as_str();
                    }
                }
                ""
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AssetStringBuilder(Vec<AssetStringComponent>);

impl Default for AssetStringBuilder {
    fn default() -> Self {
        AssetStringBuilder(vec![AssetStringComponent::Name])
    }
}

impl AssetStringBuilder {
    pub fn apply(&self, info: &AssetInfo) -> String {
        let mut result = String::new();
        for c in self.0.iter() {
            result.push_str(c.apply(info));
        }
        result
    }
}

#[derive(derive_more::Debug, Deserialize)]
#[debug("AssetRule {{\n    render: {render:?},\n    properties: {properties:?},\n}}")]
pub struct AssetRule {
    pub render: AssetRenderSpec,
    #[serde(default)]
    pub properties: BTreeSet<String>,
}

impl AssetRule {}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TintColor {
    Literal(#[serde(deserialize_with = "deserialize_rgb_u8")] Rgb<u8>),
    BiomeLookup(String),
}

impl TintColor {
    pub fn is_biome_aware(&self) -> bool {
        match self {
            TintColor::BiomeLookup(_) => true,
            _ => false,
        }
    }

    pub fn apply(&self, info: &AssetInfo, settings: &Settings) -> Option<Rgb<u8>> {
        match self {
            TintColor::Literal(literal) => Some(literal.clone()),
            TintColor::BiomeLookup(section) => {
                let biome = info.short_biome();
                if let Some(color_map) = settings.biome_colors.get(section) {
                    let biome_tint = color_map.get(biome);
                    log::debug!(
                        "got biome tint: section={} biome={} tint=#{:02X}{:02X}{:02X}",
                        section,
                        biome,
                        biome_tint[0],
                        biome_tint[1],
                        biome_tint[2]
                    );
                    Some(biome_tint)
                } else {
                    None
                }
            }
        }
    }
}

#[derive(Debug)]
pub struct AssetRules {
    default: Arc<AssetRule>,
    rules: BTreeMap<String, Arc<AssetRule>>,
}

impl AssetRules {
    pub fn get(&self, block: &BlockRef) -> (Arc<AssetRule>, AssetInfo) {
        let mut info = AssetInfo::new(block.state.name.to_owned());
        let rule = self.rules.get(info.short_name()).unwrap_or(&self.default);
        info = info.with_properties(block.state.properties.iter().filter_map(|(k, v)| {
            if self.default.properties.contains(k) || rule.properties.contains(k) {
                Some((k.to_owned(), v.to_owned()))
            } else {
                None
            }
        }));
        if rule.render.is_biome_aware() {
            info = info.with_biome(block.biome);
        }
        (rule.clone(), info)
    }
}

impl<'de> Deserialize<'de> for AssetRules {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawAssetRule {
            names: Option<Vec<String>>,
            #[serde(flatten)]
            rule: AssetRule,
        }

        let mut raw = BTreeMap::<String, RawAssetRule>::deserialize(deserializer)?;
        let Some(raw_default) = raw.remove("_default") else {
            return Err(serde::de::Error::missing_field("_default"));
        };
        let default = Arc::new(raw_default.rule);
        let mut rules = BTreeMap::new();
        for (rule_name, raw_rule) in raw.into_iter() {
            let names = raw_rule.names.unwrap_or_else(|| vec![rule_name]);
            let rule = Arc::new(raw_rule.rule);
            for name in names.into_iter() {
                rules.insert(name, rule.clone());
            }
        }

        Ok(AssetRules { default, rules })
    }
}

#[derive(Debug)]
pub struct ColorMap {
    default: Rgb<u8>,
    lookup: BTreeMap<String, Rgb<u8>>,
}

impl ColorMap {
    pub fn get(&self, biome: &str) -> Rgb<u8> {
        self.lookup.get(biome).cloned().unwrap_or(self.default)
    }
}

impl<'de> Deserialize<'de> for ColorMap {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawColorEntry {
            #[serde(deserialize_with = "deserialize_rgb_u8")]
            color: Rgb<u8>,
            #[serde(default)]
            aliases: Vec<String>,
        }

        let mut raw = BTreeMap::<String, RawColorEntry>::deserialize(deserializer)?;
        let Some(raw_default) = raw.remove("_default") else {
            return Err(serde::de::Error::missing_field("_default"));
        };
        let default = raw_default.color;
        let mut biomes = BTreeMap::new();
        for (biome, RawColorEntry { color, aliases }) in raw.into_iter() {
            biomes.insert(biome, color);
            for alias in aliases.into_iter() {
                biomes.insert(alias, color);
            }
        }
        Ok(Self {
            default,
            lookup: biomes,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub asset_rules: AssetRules,
    pub biome_colors: BTreeMap<String, ColorMap>,
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
            asset_rules: config.get("asset_rules")?,
            biome_colors: config.get("biome_colors")?,
        })
    }
}

pub const fn convert_rgb(raw: u32) -> Rgb<u8> {
    Rgb([(raw >> 16) as u8, (raw >> 8) as u8, raw as u8])
}

fn deserialize_rgb_u8<'de, D>(deserializer: D) -> Result<Rgb<u8>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(convert_rgb(u32::deserialize(deserializer)?))
}
