use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use image::{GenericImageView, Rgba, RgbaImage};
use imageproc::geometric_transformations::{Interpolation, Projection, warp_into};

use crate::world::BlockState;

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

pub struct AssetCache {
    path: PathBuf,
    textures: Mutex<HashMap<PathBuf, Arc<RgbaImage>>>,
    // block_textures: HashMap<(String, Side), RgbaImage>,
    blocks: HashMap<BlockState, Option<Arc<Asset>>>,
    projection_east: Projection,
    projection_south: Projection,
    projection_top: Projection,
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
                blocks: HashMap::new(),
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

    pub fn get_asset(&mut self, block_state: &BlockState) -> Option<Arc<Asset>> {
        if let Some(asset) = self.blocks.get(block_state) {
            return asset.clone();
        }
        let asset = match self.create_asset(block_state) {
            Ok(Some(asset)) => Some(Arc::new(asset)),
            Ok(None) => None,
            Err(err) => {
                log::error!("failed to create asset for {:?}: {}", block_state, err);
                None
            }
        };
        self.blocks.insert(block_state.clone(), asset.clone());
        asset
    }

    fn create_asset(&self, block_state: &BlockState) -> anyhow::Result<Option<Asset>> {
        log::debug!("creating asset for {:?}", block_state);
        match block_state
            .name
            .split_once(":")
            .ok_or(anyhow!("invalid block name"))?
        {
            ("minecraft", name) => self.create_minecraft_asset(name, block_state),
            _ => unimplemented!(),
        }
    }

    fn create_minecraft_asset(
        &self,
        name: &str,
        block_state: &BlockState,
    ) -> anyhow::Result<Option<Asset>> {
        match name {
            "air" => Ok(None),
            "podzol" => self.create_solid_block_top_side("podzol_top", "podzol_side", block_state),
            name @ "deepslate" | name if name.ends_with("_log") || name.ends_with("_stem") => self
                .create_solid_block_top_side(format!("{}_top", name).as_str(), name, block_state),
            name => self.create_solid_block_uniform(name, block_state),
        }
    }

    /// Create an asset for a solid block with a different top texture and same side textures.
    fn create_solid_block_top_side(
        &self,
        top_name: &str,
        side_name: &str,
        block_state: &BlockState,
    ) -> anyhow::Result<Option<Asset>> {
        let top_texture = self.get_block_texture(top_name)?;
        let side_texture = self.get_block_texture(side_name)?;
        let output = match block_state.get_property("axis") {
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

    /// Create an asset for a solid block with the same texture on each face.
    fn create_solid_block_uniform(
        &self,
        name: &str,
        _block_state: &BlockState,
    ) -> anyhow::Result<Option<Asset>> {
        let texture = self.get_block_texture(name)?;
        let output = self.render_solid_block(&texture, &texture, &texture);
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
