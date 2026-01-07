use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use image::{GenericImageView, Rgba, RgbaImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};

use crate::world::{BlockRef, BlockState};

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

impl AssetInfo {
    pub fn new(name: String) -> Self {
        AssetInfo(BTreeMap::new()).with_property(PROP_NAME.to_owned(), name)
    }

    pub fn with_property(mut self, k: String, v: String) -> Self {
        self.insert(k, v);
        self
    }

    pub fn with_properties(mut self, iter: impl IntoIterator<Item = (String, String)>) -> Self {
        self.extend(iter);
        self
    }

    pub fn with_biome(mut self, v: String) -> Self {
        self.insert(PROP_BIOME.to_owned(), v);
        self
    }

    pub fn short_name(&self) -> &str {
        let name = &self[PROP_NAME];
        if let Some((_left, right)) = name.split_once(":") {
            right
        } else {
            name.as_str()
        }
    }

    pub fn get_property(&self, k: &str) -> Option<&str> {
        self.get(k).map(|v| v.as_str())
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

        let f: &dyn Fn(&AssetInfo) -> anyhow::Result<Option<Asset>> = match info.short_name() {
            "air" => &|_| Ok(None),
            "podzol" => &|info| self.create_solid_block_top_side(info, "_top", "_side"),
            name @ "deepslate" | name if name.ends_with("_log") || name.ends_with("_stem") => {
                &|info| self.create_solid_block_top_side(info, "_top", "")
            }
            _ => &|info| self.create_solid_block_uniform(info),
        };
        self.get_or_create_asset(info, f)
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
        let output = self.render_solid_block(&texture, &texture, &texture);
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
            None | Some("y") => self.render_solid_block(&top_texture, &side_texture, &side_texture),
            Some("x") => {
                let rotated_side_texture = image::imageops::rotate90(side_texture.as_ref());
                self.render_solid_block(&rotated_side_texture, &rotated_side_texture, &top_texture)
            }
            Some("z") => {
                let rotated_side_texture = image::imageops::rotate90(side_texture.as_ref());
                self.render_solid_block(&side_texture, &top_texture, &rotated_side_texture)
            }
            Some(axis) => {
                return Err(anyhow!("unsupported axis value: {}", axis));
            }
        };
        Ok(Some(Asset { image: output }))
    }

    /// Render a solid block with the 3 specified face textures.
    fn render_solid_block(
        &self,
        top_texture: &RgbaImage,
        south_texture: &RgbaImage,
        east_texture: &RgbaImage,
    ) -> RgbaImage {
        let top = self.render_block_face(top_texture, Face::Top);
        let mut south = self.render_block_face(south_texture, Face::South);
        image::imageops::colorops::brighten_in_place(&mut south, -25);
        let mut east = self.render_block_face(east_texture, Face::East);
        image::imageops::colorops::brighten_in_place(&mut east, -40);

        let mut output = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        image::imageops::overlay(&mut output, &east, 0, 0);
        image::imageops::overlay(&mut output, &south, 0, 0);
        image::imageops::overlay(&mut output, &top, 0, 0);
        output
    }

    /// Project a 16x16 `texture` onto a face of a 24x24 isometric cube.
    fn render_block_face(&self, texture: &RgbaImage, face: Face) -> RgbaImage {
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
        buffer
    }
}

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
