use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use image::{GenericImageView, Rgb, Rgba, RgbaImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};

use crate::world::BlockRef;

pub const TILE_SIZE: u32 = 24;

/// The sides of a cube/block. The ordering defines the preferred render order.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Face {
    Bottom,
    North,
    West,
    East,
    South,
    Top,
}

#[derive(Clone, Eq, PartialEq, Hash, derive_more::Deref, derive_more::DerefMut)]
struct AssetInfo(BTreeMap<String, String>);

const PROP_NAME: &str = "_asset";
const PROP_BIOME: &str = "_biome";
pub const DEFAULT_BIOME: &str = "minecraft:plains";

impl AssetInfo {
    pub fn new<V: Into<String>>(name: V) -> Self {
        AssetInfo(BTreeMap::new()).with_property(PROP_NAME.to_owned(), name.into())
    }

    pub fn with_property<K: Into<String>, V: Into<String>>(mut self, k: K, v: V) -> Self {
        self.insert(k.into(), v.into());
        self
    }

    pub fn with_properties<K: Into<String>, V: Into<String>>(
        mut self,
        iter: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        self.extend(iter.into_iter().map(|(k, v)| (k.into(), v.into())));
        self
    }

    pub fn with_biome<V: Into<String>>(mut self, v: V) -> Self {
        self.insert(PROP_BIOME.to_owned(), v.into());
        self
    }

    pub fn get_property<K: AsRef<str>>(&self, k: K) -> Option<&str> {
        self.get(k.as_ref()).map(|v| v.as_str())
    }

    pub fn short_name(&self) -> &str {
        let name = &self[PROP_NAME];
        if let Some((_left, right)) = name.split_once(":") {
            right
        } else {
            name.as_str()
        }
    }

    pub fn short_biome(&self) -> &str {
        let biome = self.get_property(PROP_BIOME).unwrap_or(DEFAULT_BIOME);
        if let Some((_left, right)) = biome.split_once(":") {
            right
        } else {
            biome
        }
    }
}

impl std::fmt::Display for AssetInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut iter = self.iter();
        let (k, v) = iter.next().unwrap();
        write!(f, "{k}={v}")?;
        for (k, v) in iter {
            write!(f, ";{k}={v}")?;
        }
        Ok(())
    }
}

pub struct AssetCache {
    path: PathBuf,
    textures: Mutex<HashMap<PathBuf, Arc<RgbaImage>>>,
    assets: Mutex<HashMap<AssetInfo, Option<Arc<Asset>>>>,
    projection_east: Projection,
    projection_south: Projection,
    projection_top: Projection,
    /// Block properties that always affect rendering if present.
    block_common_props: HashSet<String>,
}

/// Flatten a sequence of projections into a single projection, in reverse order so they can be
/// written as a natural sequence of operations.
fn flatten_projection(projections: impl IntoIterator<Item = Projection>) -> Projection {
    projections
        .into_iter()
        .reduce(|acc, item| item * acc)
        .unwrap()
}

const BLOCK_TEXTURE_PATH: &str = "minecraft/textures/block";

impl AssetCache {
    pub fn new(path: PathBuf) -> anyhow::Result<AssetCache> {
        if !path.is_dir() || !path.join(".mcassetsroot").exists() {
            Err(anyhow::anyhow!("not a minecraft assets dir"))
        } else {
            Ok(AssetCache {
                path,
                textures: Mutex::new(HashMap::new()),
                assets: Mutex::new(HashMap::new()),
                projection_east: flatten_projection([
                    Projection::from_matrix([1., 0., 0., -0.5, 1., 0., 0., 0., 1.]).unwrap(),
                    Projection::scale(12. / 16., 19. / 24.),
                    Projection::translate(12., 11.5),
                ]),
                projection_south: flatten_projection([
                    Projection::from_matrix([1., 0., 0., 0.5, 1., 0., 0., 0., 1.]).unwrap(),
                    Projection::scale(13. / 16., 19. / 24.),
                    Projection::translate(-0.5, 5.6),
                ]),
                projection_top: flatten_projection([
                    Projection::translate(-8., -8.),
                    Projection::rotate(45f32.to_radians()),
                    Projection::scale(1.17, 1.17),
                    Projection::scale(1.0, 0.5),
                    Projection::translate(11.5, 5.5),
                ]),
                block_common_props: HashSet::from_iter(
                    [
                        "age",
                        "axis",
                        "berries",
                        "bites",
                        "down",
                        "east",
                        "eggs",
                        "eye",
                        "face",
                        "facing",
                        "flower_amount",
                        "half",
                        "hinge",
                        "layers",
                        "moisture",
                        "north",
                        "open",
                        "orientation",
                        "part",
                        "pickles",
                        "powered",
                        "segment_amount",
                        "shape",
                        "snowy",
                        "south",
                        "type",
                        "up",
                        "waterlogged",
                        "west",
                    ]
                    .into_iter()
                    .map(String::from),
                ),
            })
        }
    }

    pub fn get_texture(&self, path: impl AsRef<Path>) -> anyhow::Result<Arc<RgbaImage>> {
        let mut textures = self.textures.lock().unwrap();
        let path = path.as_ref();
        if !textures.contains_key(path) {
            log::debug!("loading texture {:?}", path);
            let original_texture = image::open(self.path.join(path))?.to_rgba8();
            // TODO: might not always want to do this, especially if using this method for non-block textures
            let texture = original_texture.view(0, 0, 16, 16).to_image();
            textures.insert(path.to_owned(), Arc::new(texture));
        }
        textures
            .get(path)
            .map(|texture| texture.clone())
            .ok_or_else(|| anyhow::anyhow!("texture not found: {:?}", path))
    }

    pub fn get_block_texture(&self, name: impl AsRef<Path>) -> anyhow::Result<Arc<RgbaImage>> {
        self.get_texture(
            Path::new(BLOCK_TEXTURE_PATH)
                .join(name)
                .with_extension("png"),
        )
    }

    pub fn get_asset(&self, block: &BlockRef) -> Option<Arc<Asset>> {
        let info = AssetInfo::new(block.state.name.to_owned()).with_properties(
            block.state.properties.iter().filter_map(|(k, v)| {
                if self.block_common_props.contains(k) {
                    Some((k.to_owned(), v.to_owned()))
                } else {
                    None
                }
            }),
        );

        match info.short_name() {
            "air" => None,
            "grass_block" => self
                .get_or_create_asset(info.with_biome(block.biome.to_owned()), |info| {
                    self.create_grass_block(info)
                }),
            "podzol" => self.get_or_create_asset(info, |info| {
                self.create_solid_block_top_side(info, "_top", "_side")
            }),
            // TODO: "level" should factor in to water block rendering
            "water" => self.get_or_create_asset(
                info.with_biome(block.biome.to_owned()).with_property(
                    "falling",
                    block.state.get_property("falling").unwrap_or("false"),
                ),
                |info| self.create_water_block(info),
            ),
            // TODO: birch and spruce leaves have constant colours applied to them
            "oak_leaves" | "jungle_leaves" | "acacia_leaves" | "dark_oak_leaves"
            | "mangrove_leaves" => self
                .get_or_create_asset(info.with_biome(block.biome.to_owned()), |info| {
                    self.create_leaf_block(info)
                }),
            name @ "deepslate" | name if name.ends_with("_log") || name.ends_with("_stem") => self
                .get_or_create_asset(info, |info| {
                    self.create_solid_block_top_side(info, "_top", "")
                }),
            _ => self.get_or_create_asset(info, |info| self.create_solid_block_uniform(info)),
        }
    }

    fn get_or_create_asset<F>(&self, info: AssetInfo, f: F) -> Option<Arc<Asset>>
    where
        F: FnOnce(&AssetInfo) -> anyhow::Result<Option<Asset>>,
    {
        let mut assets = self.assets.lock().unwrap();
        if let Some(cached) = assets.get(&info) {
            return cached.clone();
        }
        log::debug!("creating asset for {info}");
        let span = tracing::span!(tracing::Level::INFO, "create_asset", key = %info);
        let _enter = span.enter();
        match f(&info) {
            Ok(Some(asset)) => {
                let asset = Some(Arc::new(asset));
                assets.insert(info, asset.clone());
                asset
            }
            Ok(None) => {
                assets.insert(info, None);
                None
            }
            Err(err) => {
                log::error!("failed to create asset for {info}: {err}");
                assets.insert(info, None);
                None
            }
        }
    }

    /// Create an asset for a solid block with the same texture on each face.
    fn create_solid_block_uniform(&self, info: &AssetInfo) -> anyhow::Result<Option<Asset>> {
        let texture = self.get_block_texture(info.short_name())?;
        let output = self.render_solid_block(&texture, &texture, &texture, &TINT_BLOCK_3D);
        Ok(Some(Asset { image: output }))
    }

    /// Create an asset for a solid block with a different top texture and same side textures.
    fn create_solid_block_top_side(
        &self,
        info: &AssetInfo,
        top_suffix: &str,
        side_suffix: &str,
    ) -> anyhow::Result<Option<Asset>> {
        let name = info.short_name();
        let top_texture = self.get_block_texture(format!("{name}{top_suffix}"))?;
        let side_texture = self.get_block_texture(format!("{name}{side_suffix}"))?;
        let output = match info.get_property("axis") {
            None | Some("y") => {
                self.render_solid_block(&top_texture, &side_texture, &side_texture, &TINT_BLOCK_3D)
            }
            Some("x") => {
                let rotated_side_texture = image::imageops::rotate90(side_texture.as_ref());
                self.render_solid_block(
                    &rotated_side_texture,
                    &rotated_side_texture,
                    &top_texture,
                    &TINT_BLOCK_3D,
                )
            }
            Some("z") => {
                let rotated_side_texture = image::imageops::rotate90(side_texture.as_ref());
                self.render_solid_block(
                    &side_texture,
                    &top_texture,
                    &rotated_side_texture,
                    &TINT_BLOCK_3D,
                )
            }
            Some(axis) => {
                return Err(anyhow!("unsupported axis value: {}", axis));
            }
        };
        Ok(Some(Asset { image: output }))
    }

    fn create_grass_block(&self, info: &AssetInfo) -> anyhow::Result<Option<Asset>> {
        let biome = info.short_biome();
        let biome_tint = biome_grass_tint(biome);
        log::debug!(
            "got tint: biome={biome} tint=#{:X}{:X}{:X}",
            biome_tint[0],
            biome_tint[1],
            biome_tint[2]
        );
        let mut top = (*self.get_block_texture("grass_block_top")?).clone();
        tint_in_place(&mut top, biome_tint);
        let mut side_overlay = (*self.get_block_texture("grass_block_side_overlay")?).clone();
        tint_in_place(&mut side_overlay, biome_tint);
        let mut side = (*self.get_block_texture("dirt")?).clone();
        image::imageops::overlay(&mut side, &side_overlay, 0, 0);
        let output = self.render_solid_block(&top, &side, &side, &TINT_BLOCK_3D);
        Ok(Some(Asset { image: output }))
    }

    fn create_leaf_block(&self, info: &AssetInfo) -> anyhow::Result<Option<Asset>> {
        let biome = info.short_biome();
        let biome_tint = biome_foliage_tint(biome);
        log::debug!(
            "got tint: biome={biome} tint=#{:X}{:X}{:X}",
            biome_tint[0],
            biome_tint[1],
            biome_tint[2]
        );
        let mut texture = (*self.get_block_texture(info.short_name())?).clone();
        tint_in_place(&mut texture, biome_tint);
        let output = self.render_solid_block(&texture, &texture, &texture, &TINT_BLOCK_3D);
        Ok(Some(Asset { image: output }))
    }

    fn create_water_block(&self, info: &AssetInfo) -> anyhow::Result<Option<Asset>> {
        let biome = info.short_biome();
        let biome_tint = biome_water_tint(biome);
        // let mut texture = (*self.get_block_texture("water_still")?).clone();
        let mut texture = RgbaImage::from_pixel(16, 16, Rgba([255, 255, 255, 120]));
        tint_in_place(&mut texture, biome_tint);
        let block_tints = if let Some("true") = info.get_property("falling") {
            &TINT_BLOCK_3D
        } else {
            &TINT_BLOCK_NONE
        };
        let output = self.render_solid_block(&texture, &texture, &texture, block_tints);
        Ok(Some(Asset { image: output }))
    }

    /// Render a solid block with the 3 specified face textures.
    fn render_solid_block(
        &self,
        top_texture: &RgbaImage,
        south_texture: &RgbaImage,
        east_texture: &RgbaImage,
        tints: &SolidBlockTints,
    ) -> RgbaImage {
        let top = self.render_block_face(top_texture, Face::Top, tints.top);
        let south = self.render_block_face(south_texture, Face::South, tints.south);
        let east = self.render_block_face(east_texture, Face::East, tints.east);
        let mut output = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        image::imageops::overlay(&mut output, &east, 0, 0);
        image::imageops::overlay(&mut output, &south, 0, 0);
        image::imageops::overlay(&mut output, &top, 0, 0);
        output
    }

    /// Project a 16x16 `texture` onto a face of a 24x24 isometric cube.
    fn render_block_face(
        &self,
        texture: &RgbaImage,
        face: Face,
        tint: Option<Rgb<u8>>,
    ) -> RgbaImage {
        debug_assert_eq!(texture.dimensions(), (16, 16));
        let projection = match face {
            Face::East => &self.projection_east,
            Face::South => &self.projection_south,
            Face::Top => &self.projection_top,
            _ => unimplemented!(),
        };
        let mut buffer = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        warp_into(
            texture,
            projection,
            Interpolation::Bilinear,
            Rgba([0, 0, 0, 0]),
            &mut buffer,
        );
        if let Some(tint_color) = tint {
            tint_in_place(&mut buffer, tint_color);
        }
        buffer
    }
}

macro_rules! rgb_const {
    ($($vis:vis $id:ident : $val:expr);* $(;)?) => {
        $(
            // #[allow(unused)]
            const $id: Rgb<u8> = Rgb([(($val as u32) >> 16) as u8, (($val as u32) >> 8) as u8, (($val as u32) as u8)]);
        )*
    };
}

#[derive(Clone, Debug, Default)]
struct SolidBlockTints {
    top: Option<Rgb<u8>>,
    south: Option<Rgb<u8>>,
    east: Option<Rgb<u8>>,
}

const TINT_BLOCK_NONE: SolidBlockTints = SolidBlockTints {
    top: None,
    south: None,
    east: None,
};
const TINT_BLOCK_3D: SolidBlockTints = SolidBlockTints {
    top: None,
    south: Some(Rgb([220, 220, 220])),
    east: Some(Rgb([200, 200, 200])),
};

// https://minecraft.wiki/w/Block_colors
rgb_const!(
    TINT_GRASS_BADLANDS: 0x90814D;
    TINT_GRASS_DESERT: 0xBFB755;
    TINT_GRASS_STONY_PEAKS: 0x9ABE4B;
    TINT_GRASS_JUNGLE: 0x59C93C;
    TINT_GRASS_SPARSE_JUNGLE: 0x64C73F;
    TINT_GRASS_MUSHROOM_FIELDS: 0x55C93F;
    TINT_GRASS_PLAINS: 0x91BD59;
    TINT_GRASS_SWAMP: 0x6A7039;
    // TINT_GRASS_SWAMP: 0x4C763C;
    TINT_GRASS_FOREST: 0x79C05A;
    TINT_GRASS_DARK_FOREST: 0x507A32;
    TINT_GRASS_PALE_GARDEN: 0x878D76;
    TINT_GRASS_BIRCH_FOREST: 0x88BB67;
    TINT_GRASS_OCEAN: 0x8EB971;
    TINT_GRASS_MEADOW: 0x83BB6D;
    TINT_GRASS_CHERRY_GROVE: 0xB6DB61;
    TINT_GRASS_TAIGA_OLD_PINE: 0x86B87F;
    TINT_GRASS_TAIGA: 0x86B783;
    TINT_GRASS_WINDSWEPT_HILLS: 0x8AB689;
    TINT_GRASS_SNOWY_BEACH: 0x83B593;
    TINT_GRASS_SNOWY_PLAINS: 0x80B497;
);

// https://minecraft.wiki/w/Block_colors
rgb_const!(
    TINT_FOLIAGE_BADLANDS: 0x9E814D;
    TINT_FOLIAGE_DESERT: 0xAEA42A;
    TINT_FOLIAGE_STONY_PEAKS: 0x82AC1E;
    TINT_FOLIAGE_JUNGLE: 0x30BB0B;
    TINT_FOLIAGE_SPARSE_JUNGLE: 0x3EB80F;
    TINT_FOLIAGE_MUSHROOM_FIELDS: 0x2BBB0F;
    TINT_FOLIAGE_PLAINS: 0x77AB2F;
    TINT_FOLIAGE_SWAMP: 0x6A7039;
    TINT_FOLIAGE_MANGROVE_SWAMP: 0x8DB127;
    TINT_FOLIAGE_FOREST: 0x59AE30;
    TINT_FOLIAGE_PALE_GARDEN: 0x878D76;
    TINT_FOLIAGE_BIRCH_FOREST: 0x6BA941;
    TINT_FOLIAGE_OCEAN: 0x71A74D;
    TINT_FOLIAGE_MEADOW: 0x63A948;
    TINT_FOLIAGE_CHERRY_GROVE: 0xB6DB61;
    TINT_FOLIAGE_TAIGA_OLD_PINE: 0x68A55F;
    TINT_FOLIAGE_TAIGA: 0x68A464;
    TINT_FOLIAGE_WINDSWEPT_HILLS: 0x6DA36B;
    TINT_FOLIAGE_SNOWY_BEACH: 0x64A278;
    TINT_FOLIAGE_SNOWY_PLAINS: 0x60A17B;
);

// https://minecraft.wiki/w/Block_colors
rgb_const!(
    TINT_WATER_DEFAULT: 0x3F76E4;
    TINT_WATER_COLD: 0x3D57D6;
    TINT_WATER_FROZEN: 0x3938C9;
    TINT_WATER_LUKEWARM: 0x45ADF2;
    TINT_WATER_SWAMP: 0x617B64;
    TINT_WATER_WARM: 0x43D5EE;
    TINT_WATER_MEADOW: 0x0E4ECF;
    TINT_WATER_MANGROVE_SWAMP: 0x3A7A6A;
    TINT_WATER_CHERRY_GROVE: 0x5DB7EF;
    TINT_WATER_PALE_GARDEN: 0x76889D;
);

fn biome_grass_tint(biome: &str) -> Rgb<u8> {
    match biome {
        b if b.contains("badlands") => TINT_GRASS_BADLANDS,
        "desert" => TINT_GRASS_DESERT,
        b if b.contains("savanna") => TINT_GRASS_DESERT,
        "nether_wastes" | "soul_sand_valley" | "crimson_forest" | "warped_forest"
        | "basalt_deltas" => TINT_GRASS_DESERT,
        "stony_peaks" => TINT_GRASS_STONY_PEAKS,
        "jungle" | "bamboo_jungle" => TINT_GRASS_JUNGLE,
        "sparse_jungle" => TINT_GRASS_SPARSE_JUNGLE,
        "mushroom_fields" => TINT_GRASS_MUSHROOM_FIELDS,
        "plains" | "sunflower_plains" | "beach" | "dripstone_caves" | "deep_dark" => {
            TINT_GRASS_PLAINS
        }
        "swamp" | "mangrove_swamp" => TINT_GRASS_SWAMP,
        "forest" | "flower_forest" => TINT_GRASS_FOREST,
        "dark_forest" => TINT_GRASS_DARK_FOREST,
        "pale_garden" => TINT_GRASS_PALE_GARDEN,
        "birch_forest" | "old_growth_birch_forest" => TINT_GRASS_BIRCH_FOREST,
        "ocean" | "deep_ocean" => TINT_GRASS_OCEAN,
        "warm_ocean" | "lukewarm_ocean" | "deep_lukewarm_ocean" => TINT_GRASS_OCEAN,
        "cold_ocean" | "deep_cold_ocean" | "deep_frozen_ocean" => TINT_GRASS_OCEAN,
        "river" | "lush_caves" => TINT_GRASS_OCEAN,
        "the_end" | "end_highlands" | "end_midlands" | "small_end_islands" | "end_barrens" => {
            TINT_GRASS_OCEAN
        }
        "the_void" => TINT_GRASS_OCEAN,
        "meadow" => TINT_GRASS_MEADOW,
        "cherry_grove" => TINT_GRASS_CHERRY_GROVE,
        "old_growth_pine_taiga" => TINT_GRASS_TAIGA_OLD_PINE,
        "taiga" | "old_growth_spruce_taiga" => TINT_GRASS_TAIGA,
        "windswept_hills" | "windswept_gravelly_hills" | "windswept_forest" | "stony_shore" => {
            TINT_GRASS_WINDSWEPT_HILLS
        }
        "snowy_beach" => TINT_GRASS_SNOWY_BEACH,
        b if b.starts_with("snowy_") => TINT_GRASS_SNOWY_PLAINS,
        "ice_spikes" | "frozen_ocean" | "frozen_river" | "grove" | "frozen_peaks"
        | "jagged_peaks" => TINT_GRASS_SNOWY_PLAINS,
        // Default tint = white so it shows up where it's not being handled
        _ => {
            log::warn!("unhandled biome {biome:?}");
            Rgb([0xFF, 0xFF, 0xFF])
        }
    }
}

fn biome_foliage_tint(biome: &str) -> Rgb<u8> {
    match biome {
        b if b.contains("badlands") => TINT_FOLIAGE_BADLANDS,
        "desert" => TINT_FOLIAGE_DESERT,
        b if b.contains("savanna") => TINT_FOLIAGE_DESERT,
        "nether_wastes" | "soul_sand_valley" | "crimson_forest" | "warped_forest"
        | "basalt_deltas" => TINT_FOLIAGE_DESERT,
        "stony_peaks" => TINT_FOLIAGE_STONY_PEAKS,
        "jungle" | "bamboo_jungle" => TINT_FOLIAGE_JUNGLE,
        "sparse_jungle" => TINT_FOLIAGE_SPARSE_JUNGLE,
        "mushroom_fields" => TINT_FOLIAGE_MUSHROOM_FIELDS,
        "plains" | "sunflower_plains" | "beach" | "dripstone_caves" | "deep_dark" => {
            TINT_FOLIAGE_PLAINS
        }
        "swamp" => TINT_FOLIAGE_SWAMP,
        "mangrove_swamp" => TINT_FOLIAGE_MANGROVE_SWAMP,
        "forest" | "flower_forest" | "dark_forest" => TINT_FOLIAGE_FOREST,
        "pale_garden" => TINT_FOLIAGE_PALE_GARDEN,
        "birch_forest" | "old_growth_birch_forest" => TINT_FOLIAGE_BIRCH_FOREST,
        "ocean" | "deep_ocean" => TINT_FOLIAGE_OCEAN,
        "warm_ocean" | "lukewarm_ocean" | "deep_lukewarm_ocean" => TINT_FOLIAGE_OCEAN,
        "cold_ocean" | "deep_cold_ocean" | "deep_frozen_ocean" => TINT_FOLIAGE_OCEAN,
        "river" | "lush_caves" => TINT_FOLIAGE_OCEAN,
        "the_end" | "end_highlands" | "end_midlands" | "small_end_islands" | "end_barrens" => {
            TINT_FOLIAGE_OCEAN
        }
        "the_void" => TINT_FOLIAGE_OCEAN,
        "meadow" => TINT_FOLIAGE_MEADOW,
        "cherry_grove" => TINT_FOLIAGE_CHERRY_GROVE,
        "old_growth_pine_taiga" => TINT_FOLIAGE_TAIGA_OLD_PINE,
        "taiga" | "old_growth_spruce_taiga" => TINT_FOLIAGE_TAIGA,
        "windswept_hills" | "windswept_gravelly_hills" | "windswept_forest" | "stony_shore" => {
            TINT_FOLIAGE_WINDSWEPT_HILLS
        }
        "snowy_beach" => TINT_FOLIAGE_SNOWY_BEACH,
        b if b.starts_with("snowy_") => TINT_FOLIAGE_SNOWY_PLAINS,
        "ice_spikes" | "frozen_ocean" | "frozen_river" | "grove" | "frozen_peaks"
        | "jagged_peaks" => TINT_FOLIAGE_SNOWY_PLAINS,
        // Default tint = white so it shows up where it's not being handled
        _ => {
            log::warn!("unhandled biome {biome:?}");
            Rgb([0xFF, 0xFF, 0xFF])
        }
    }
}

fn biome_water_tint(biome: &str) -> Rgb<u8> {
    match biome {
        "cold_ocean" | "deep_cold_ocean" | "snowy_taiga" | "snowy_beach" => TINT_WATER_COLD,
        "frozen_ocean" | "deep_frozen_ocean" | "frozen_river" => TINT_WATER_FROZEN,
        "lukewarm_ocean" | "deep_lukewarm_ocean" => TINT_WATER_LUKEWARM,
        "swamp" => TINT_WATER_SWAMP,
        "warm_ocean" => TINT_WATER_WARM,
        "meadow" => TINT_WATER_MEADOW,
        "mangrove_swamp" => TINT_WATER_MANGROVE_SWAMP,
        "cherry_grove" => TINT_WATER_CHERRY_GROVE,
        "pale_garden" => TINT_WATER_PALE_GARDEN,
        _ => TINT_WATER_DEFAULT,
    }
}

fn tint(image: &RgbaImage, tint: Rgb<u8>) -> RgbaImage {
    let mut output = image.clone();
    tint_in_place(&mut output, tint);
    output
}

fn tint_in_place(image: &mut RgbaImage, tint: Rgb<u8>) {
    for pixel in image.pixels_mut() {
        pixel[0] = (pixel[0] as f32 * (tint[0] as f32 / 255.)) as u8;
        pixel[1] = (pixel[1] as f32 * (tint[1] as f32 / 255.)) as u8;
        pixel[2] = (pixel[2] as f32 * (tint[2] as f32 / 255.)) as u8;
    }
}

// fn tint_into(image: &RgbaImage, tint: Rgb<u8>, output: &mut RgbaImage) {
//     for (old, new) in image.pixels().zip(output.pixels_mut()) {
//         new[0] = (old[0] as f32 * (tint[0] as f32 / 255.)) as u8;
//         new[1] = (old[1] as f32 * (tint[1] as f32 / 255.)) as u8;
//         new[2] = (old[2] as f32 * (tint[2] as f32 / 255.)) as u8;
//         new[3] = old[3];
//     }
// }

pub struct Asset {
    pub image: RgbaImage,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_assetinfo() {
        let info = AssetInfo::new("minecraft:birch_log".to_owned())
            .with_property("axis".to_owned(), "z".to_owned());
        assert_eq!(format!("{info}"), "_asset=minecraft:birch_log;axis=z");
        let info = AssetInfo::new("minecraft:leaf_litter".to_owned())
            .with_property("segment_amount".to_owned(), "3".to_owned())
            .with_property("facing".to_owned(), "east".to_owned())
            .with_biome("badlands".to_owned());
        assert_eq!(
            format!("{info}"),
            "_asset=minecraft:leaf_litter;_biome=badlands;facing=east;segment_amount=3"
        );
    }
}
