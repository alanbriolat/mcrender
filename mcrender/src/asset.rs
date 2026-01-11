use std::collections::HashMap;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use image::imageops::overlay;
use image::{GenericImageView, Rgb, Rgba, RgbaImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};

use crate::proplist::PropList;
use crate::settings::{AssetRenderSpec, Settings};
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

#[derive(
    Clone, Eq, PartialEq, Hash, Ord, PartialOrd, derive_more::Deref, derive_more::DerefMut,
)]
pub struct AssetInfo(PropList);

const PROP_NAME: &str = "_asset";
const PROP_BIOME: &str = "_biome";
pub const DEFAULT_BIOME: &str = "minecraft:plains";

impl AssetInfo {
    pub fn new<V: AsRef<str>>(name: V) -> Self {
        Self(PropList::new()).with_property(PROP_NAME, name)
    }

    pub fn with_property<K: AsRef<str>, V: AsRef<str>>(mut self, k: K, v: V) -> Self {
        self.insert(k.as_ref(), v.as_ref());
        self
    }

    pub fn with_properties<K: AsRef<str>, V: AsRef<str>>(
        mut self,
        iter: impl IntoIterator<Item = (K, V)>,
    ) -> Self {
        for (k, v) in iter.into_iter() {
            self.insert(k.as_ref(), v.as_ref());
        }
        self
    }

    pub fn with_biome<V: AsRef<str>>(mut self, v: V) -> Self {
        self.insert(PROP_BIOME, v.as_ref());
        self
    }

    pub fn get_property<K: AsRef<str>>(&self, k: K) -> Option<&str> {
        self.get(k.as_ref())
    }

    pub fn short_name(&self) -> &str {
        let name = self.get(PROP_NAME).unwrap();
        if let Some((_left, right)) = name.split_once(":") {
            right
        } else {
            name
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

pub struct AssetCache<'s> {
    path: PathBuf,
    textures: Mutex<HashMap<PathBuf, Arc<RgbaImage>>>,
    assets: Mutex<HashMap<AssetInfo, Option<Arc<Asset>>>>,
    projection_east: Projection,
    projection_south: Projection,
    projection_top: Projection,
    projection_west: Projection,
    projection_north: Projection,
    projection_bottom: Projection,
    settings: &'s Settings,
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

impl<'s> AssetCache<'s> {
    pub fn new(path: PathBuf, settings: &'s Settings) -> anyhow::Result<AssetCache<'s>> {
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
                projection_west: flatten_projection([
                    Projection::from_matrix([1., 0., 0., -0.5, 1., 0., 0., 0., 1.]).unwrap(),
                    Projection::scale(12. / 16., 19. / 24.),
                    Projection::translate(0., 5.),
                ]),
                projection_north: flatten_projection([
                    Projection::from_matrix([1., 0., 0., 0.5, 1., 0., 0., 0., 1.]).unwrap(),
                    Projection::scale(13. / 16., 19. / 24.),
                    Projection::translate(11.5, -0.8),
                ]),
                projection_bottom: flatten_projection([
                    Projection::translate(-8., -8.),
                    Projection::rotate(45f32.to_radians()),
                    Projection::scale(1.17, 1.17),
                    Projection::scale(1.0, 0.5),
                    Projection::translate(11.5, 17.5),
                ]),
                settings,
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
        let (rule, info) = self.settings.asset_rules.get(block);

        // TODO: RwLock instead?
        let mut assets = self.assets.lock().unwrap();
        if let Some(cached) = assets.get(&info) {
            return cached.clone();
        }

        match self.create_asset(&info, &rule.render) {
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

    #[tracing::instrument(skip_all, fields(key = %info))]
    fn create_asset(
        &self,
        info: &AssetInfo,
        renderer: &AssetRenderSpec,
    ) -> anyhow::Result<Option<Asset>> {
        use AssetRenderSpec::*;

        log::debug!("creating asset");
        match &renderer {
            Nothing => Ok(None),

            SolidUniform { texture } => {
                let texture_name = texture.apply(info);
                let texture = self.get_block_texture(texture_name)?;
                let output = self.render_solid_block(&texture, &texture, &texture, &TINT_BLOCK_3D);
                Ok(Some(Asset { image: output }))
            }

            SolidTopSide {
                top_texture,
                side_texture,
            } => {
                let top_texture = self.get_block_texture(top_texture.apply(info))?;
                let side_texture = self.get_block_texture(side_texture.apply(info))?;
                self.create_solid_block_top_side(info, &top_texture, &side_texture)
            }

            Leaves {
                texture,
                tint_color,
            } => {
                let texture_name = texture.apply(info);
                let mut texture = (*self.get_block_texture(texture_name)?).clone();
                if let Some(actual_tint_color) = tint_color.apply(info, self.settings) {
                    tint_in_place(&mut texture, actual_tint_color);
                }
                let output = self.render_solid_block(&texture, &texture, &texture, &TINT_BLOCK_3D);
                Ok(Some(Asset { image: output }))
            }

            Plant {
                texture,
                tint_color,
            } => {
                let texture_name = texture.apply(info);
                let mut texture = (*self.get_block_texture(texture_name)?).clone();
                if let Some(actual_tint_color) = tint_color
                    .as_ref()
                    .and_then(|tc| tc.apply(info, self.settings))
                {
                    tint_in_place(&mut texture, actual_tint_color);
                }
                let output = self.render_plant(&texture);
                Ok(Some(Asset { image: output }))
            }

            Crop { texture } => {
                let texture_name = texture.apply(info);
                let texture = self.get_block_texture(texture_name)?;
                let output = self.render_crop(&texture);
                Ok(Some(Asset { image: output }))
            }

            Grass { tint_color } => {
                let actual_tint_color = tint_color
                    .apply(info, self.settings)
                    .unwrap_or(Rgb([255, 255, 255]));
                self.create_grass_block(info, actual_tint_color)
            }

            Vine { tint_color } => {
                let texture_name = info.short_name();
                let mut texture = (*self.get_block_texture(texture_name)?).clone();
                if let Some(actual_tint_color) = tint_color
                    .as_ref()
                    .and_then(|tc| tc.apply(info, self.settings))
                {
                    tint_in_place(&mut texture, actual_tint_color);
                }
                let top_texture = if let Some("true") = info.get_property("up") {
                    Some(&texture)
                } else {
                    None
                };
                let south_texture = if let Some("true") = info.get_property("south") {
                    Some(&texture)
                } else {
                    None
                };
                let east_texture = if let Some("true") = info.get_property("east") {
                    Some(&texture)
                } else {
                    None
                };
                let bottom_texture = if let Some("true") = info.get_property("down") {
                    Some(&texture)
                } else {
                    None
                };
                let north_texture = if let Some("true") = info.get_property("north") {
                    Some(&texture)
                } else {
                    None
                };
                let west_texture = if let Some("true") = info.get_property("west") {
                    Some(&texture)
                } else {
                    None
                };
                let output = self.render_transparent_block(
                    top_texture,
                    south_texture,
                    east_texture,
                    bottom_texture,
                    north_texture,
                    west_texture,
                    &TINT_BLOCK_3D,
                );
                Ok(Some(Asset { image: output }))
            }

            Water { tint_color } => {
                let actual_tint_color = tint_color
                    .apply(info, self.settings)
                    .unwrap_or(Rgb([255, 255, 255]));
                self.create_water_block(info, actual_tint_color)
            }

            _ => unimplemented!(),
        }
    }

    /// Create an asset for a solid block with a different top texture and same side textures.
    fn create_solid_block_top_side(
        &self,
        info: &AssetInfo,
        top_texture: &RgbaImage,
        side_texture: &RgbaImage,
    ) -> anyhow::Result<Option<Asset>> {
        let output = match info.get_property("axis") {
            None | Some("y") => {
                self.render_solid_block(&top_texture, &side_texture, &side_texture, &TINT_BLOCK_3D)
            }
            Some("x") => {
                let rotated_side_texture = image::imageops::rotate90(side_texture);
                self.render_solid_block(
                    &rotated_side_texture,
                    &rotated_side_texture,
                    &top_texture,
                    &TINT_BLOCK_3D,
                )
            }
            Some("z") => {
                let rotated_side_texture = image::imageops::rotate90(side_texture);
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

    fn create_grass_block(
        &self,
        _info: &AssetInfo,
        biome_tint: Rgb<u8>,
    ) -> anyhow::Result<Option<Asset>> {
        let mut top = (*self.get_block_texture("grass_block_top")?).clone();
        tint_in_place(&mut top, biome_tint);
        let mut side_overlay = (*self.get_block_texture("grass_block_side_overlay")?).clone();
        tint_in_place(&mut side_overlay, biome_tint);
        let mut side = (*self.get_block_texture("dirt")?).clone();
        overlay(&mut side, &side_overlay, 0, 0);
        let output = self.render_solid_block(&top, &side, &side, &TINT_BLOCK_3D);
        Ok(Some(Asset { image: output }))
    }

    fn create_water_block(
        &self,
        info: &AssetInfo,
        tint_color: Rgb<u8>,
    ) -> anyhow::Result<Option<Asset>> {
        // let mut texture = (*self.get_block_texture("water_still")?).clone();
        let mut texture = RgbaImage::from_pixel(16, 16, Rgba([255, 255, 255, 120]));
        tint_in_place(&mut texture, tint_color);
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
        overlay(&mut output, &east, 0, 0);
        overlay(&mut output, &south, 0, 0);
        overlay(&mut output, &top, 0, 0);
        output
    }

    fn render_transparent_block(
        &self,
        top_texture: Option<&RgbaImage>,
        south_texture: Option<&RgbaImage>,
        east_texture: Option<&RgbaImage>,
        bottom_texture: Option<&RgbaImage>,
        north_texture: Option<&RgbaImage>,
        west_texture: Option<&RgbaImage>,
        tints: &SolidBlockTints,
    ) -> RgbaImage {
        let mut output = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        if let Some(texture) = bottom_texture {
            let projected = self.render_block_face(texture, Face::Bottom, tints.top);
            overlay(&mut output, &projected, 0, 0);
        }
        if let Some(texture) = north_texture {
            let projected = self.render_block_face(texture, Face::North, tints.south);
            overlay(&mut output, &projected, 0, 0);
        }
        if let Some(texture) = west_texture {
            let projected = self.render_block_face(texture, Face::West, tints.east);
            overlay(&mut output, &projected, 0, 0);
        }
        if let Some(texture) = east_texture {
            let projected = self.render_block_face(texture, Face::East, tints.east);
            overlay(&mut output, &projected, 0, 0);
        }
        if let Some(texture) = south_texture {
            let projected = self.render_block_face(texture, Face::South, tints.south);
            overlay(&mut output, &projected, 0, 0);
        }
        if let Some(texture) = top_texture {
            let projected = self.render_block_face(texture, Face::Top, tints.top);
            overlay(&mut output, &projected, 0, 0);
        }
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
            Face::West => &self.projection_west,
            Face::North => &self.projection_north,
            Face::Bottom => &self.projection_bottom,
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

    /// Render a simple plant, where in-game a single-texture is rendered in an X in the
    /// bottom-center of the block.
    fn render_plant(&self, texture: &RgbaImage) -> RgbaImage {
        let mut buffer = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        let front_projection = flatten_projection([
            Projection::scale(1., 12. / 16.),
            Projection::translate(4., 6.),
        ]);
        warp_into(
            texture,
            &front_projection,
            Interpolation::Nearest,
            Rgba([0, 0, 0, 0]),
            &mut buffer,
        );
        buffer
    }

    /// Render a slightly more complex plant, where in-game a single texture is rendered in a #
    /// shape in the bottom-center of the block.
    fn render_crop(&self, texture: &RgbaImage) -> RgbaImage {
        let south = self.render_block_face(texture, Face::South, TINT_BLOCK_NONE.south);
        let south_back = south.view(0, 6, 2, 13);
        let south_mid = south.view(2, 7, 8, 16);
        let south_front = south.view(10, 11, 2, 13);
        let east = self.render_block_face(texture, Face::East, TINT_BLOCK_NONE.east);
        let east_back = east.view(22, 6, 2, 13);
        let east_mid = east.view(14, 7, 8, 16);
        let east_front = east.view(12, 11, 2, 13);
        let mut output = RgbaImage::new(TILE_SIZE, TILE_SIZE);
        overlay(&mut output, south_back.deref(), 10, 1);
        overlay(&mut output, east_back.deref(), 12, 1);
        overlay(&mut output, south_back.deref(), 2, 5);
        overlay(&mut output, east_mid.deref(), 4, 2);
        overlay(&mut output, south_mid.deref(), 12, 2);
        overlay(&mut output, east_back.deref(), 20, 5);
        overlay(&mut output, east_front.deref(), 2, 6);
        overlay(&mut output, south_mid.deref(), 4, 6);
        overlay(&mut output, east_mid.deref(), 12, 6);
        overlay(&mut output, south_front.deref(), 20, 6);
        overlay(&mut output, east_front.deref(), 10, 10);
        overlay(&mut output, south_front.deref(), 12, 10);
        output
    }
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
