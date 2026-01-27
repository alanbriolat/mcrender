use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Arc, Mutex};

use anyhow::anyhow;
use arcstr::ArcStr;

use crate::canvas;
use crate::canvas::Image;
use crate::render::sprite::{Aspect, PartialSpriteCache, SpriteBuffer, new_sprite_buffer};
use crate::render::texture::TextureCache;
use crate::settings::{AssetRenderSpec, Settings};
use crate::world::{BlockInfo, BlockState};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct AssetInfo<'s> {
    pub state: Cow<'s, BlockState>,
    pub biome: Option<ArcStr>,
}

impl<'s> AssetInfo<'s> {
    pub fn into_owned(self) -> AssetInfo<'static> {
        AssetInfo {
            state: Cow::Owned(self.state.into_owned()),
            biome: self.biome,
        }
    }

    pub fn biome(&self) -> &str {
        self.biome
            .as_ref()
            .map(|biome| biome.as_str())
            .unwrap_or(DEFAULT_BIOME)
    }
}

impl<'s> std::fmt::Display for AssetInfo<'s> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        self.state.fmt(f)?;
        if let Some(ref biome) = self.biome {
            f.write_char('@')?;
            biome.fmt(f)?;
        }
        Ok(())
    }
}

pub const DEFAULT_BIOME: &str = "minecraft:plains";

pub struct AssetCache<'s> {
    partials: PartialSpriteCache,
    assets: Mutex<HashMap<AssetInfo<'static>, Option<Arc<SpriteBuffer>>>>,
    settings: &'s Settings,
}

const BLOCK_TEXTURE_PATH: &str = "minecraft/textures/block";

impl<'s> AssetCache<'s> {
    pub fn new(settings: &'s Settings) -> anyhow::Result<AssetCache<'s>> {
        if !settings.assets_path.is_dir() || !settings.assets_path.join(".mcassetsroot").exists() {
            return Err(anyhow::anyhow!("not a minecraft assets dir"));
        }
        let path = settings.assets_path.join(BLOCK_TEXTURE_PATH);
        let textures = TextureCache::new(path);
        let partials = PartialSpriteCache::new(textures);

        Ok(AssetCache {
            partials,
            assets: Mutex::new(HashMap::new()),
            settings,
        })
    }

    pub fn get_asset(&self, block: &BlockInfo) -> Option<Arc<SpriteBuffer>> {
        // Only include biome in the cache key if rendering is biome-dependent
        let biome = if block.render.is_biome_aware() {
            Some(block.biome.clone())
        } else {
            None
        };

        // Don't clone the block state unless absolutely necessary
        let info = AssetInfo {
            state: Cow::Borrowed(block.state),
            biome,
        };

        // TODO: RwLock instead?
        let mut assets = self.assets.lock().unwrap();
        if let Some(cached) = assets.get(&info) {
            return cached.clone();
        }

        // Convert to owned, because we'll need to store it as the HashMap key
        let info = info.into_owned();

        match self.create_asset(&info, &*block.render) {
            Ok(Some(sprite)) => {
                let sprite = Some(Arc::new(sprite));
                assets.insert(info, sprite.clone());
                sprite
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
    ) -> anyhow::Result<Option<SpriteBuffer>> {
        use AssetRenderSpec::*;

        log::debug!("creating asset");
        match &renderer {
            Nothing => Ok(None),

            // Render a solid block with the specified texutre on all 3 faces.
            SolidUniform { texture } => {
                let texture_name = texture.apply(&info.state);
                let mut output = new_sprite_buffer();
                const PARTIALS: [Aspect; 3] =
                    [Aspect::BlockEast, Aspect::BlockSouth, Aspect::BlockTop];
                for aspect in PARTIALS {
                    canvas::overlay(&mut output, &*self.partials.get(&texture_name, aspect)?);
                }
                Ok(Some(output))
            }

            SolidTopSide {
                top_texture,
                side_texture,
            } => {
                let top_texture = top_texture.apply(&info.state);
                let side_texture = side_texture.apply(&info.state);
                self.create_solid_block_top_side(info, &top_texture, &side_texture)
            }

            Leaves {
                texture,
                tint_color,
            } => {
                let texture_name = texture.apply(&info.state);
                let tint = tint_color.apply(info.biome(), self.settings);
                let mut output = new_sprite_buffer();
                const PARTIALS: [Aspect; 3] =
                    [Aspect::BlockEast, Aspect::BlockSouth, Aspect::BlockTop];
                for aspect in PARTIALS {
                    canvas::overlay(
                        &mut output,
                        &*self.partials.get_tinted(&texture_name, aspect, tint)?,
                    );
                }
                Ok(Some(output))
            }

            // Render a simple plant, where in-game a single-texture is rendered in an X in the
            // bottom-center of the block.
            Plant {
                texture,
                tint_color,
            } => {
                let texture_name = texture.apply(&info.state);
                let tint = tint_color
                    .as_ref()
                    .map(|tc| tc.apply(info.biome(), self.settings))
                    .flatten();
                let output =
                    (*self
                        .partials
                        .get_tinted(&texture_name, Aspect::PlantBottom, tint)?)
                    .clone();
                Ok(Some(output))
            }

            Crop { texture } => {
                let texture_name = texture.apply(&info.state);
                let output = self.render_crop(&texture_name)?;
                Ok(Some(output))
            }

            Grass { tint_color } => {
                let tint = tint_color.apply(info.biome(), self.settings);
                let mut output = new_sprite_buffer();
                canvas::overlay(&mut output, &*self.partials.get("dirt", Aspect::BlockEast)?);
                canvas::overlay(
                    &mut output,
                    &*self.partials.get_tinted(
                        "grass_block_side_overlay",
                        Aspect::BlockEast,
                        tint,
                    )?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get("dirt", Aspect::BlockSouth)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get_tinted(
                        "grass_block_side_overlay",
                        Aspect::BlockSouth,
                        tint,
                    )?,
                );
                canvas::overlay(
                    &mut output,
                    &*self
                        .partials
                        .get_tinted("grass_block_top", Aspect::BlockTop, tint)?,
                );
                Ok(Some(output))
            }

            Vine { tint_color } => {
                let texture_name = info.state.short_name();
                let tint = tint_color
                    .as_ref()
                    .map(|tc| tc.apply(info.biome(), self.settings))
                    .flatten();
                let mut output = new_sprite_buffer();
                const PARTIALS: [(&str, Aspect); 6] = [
                    ("down", Aspect::BlockBottom),
                    ("north", Aspect::BlockNorth),
                    ("west", Aspect::BlockWest),
                    ("east", Aspect::BlockEast),
                    ("south", Aspect::BlockSouth),
                    ("up", Aspect::BlockTop),
                ];
                for (direction, aspect) in PARTIALS {
                    if let Some("true") = info.state.get_property(direction) {
                        canvas::overlay(
                            &mut output,
                            &*self.partials.get_tinted(texture_name, aspect, tint)?,
                        );
                    }
                }
                Ok(Some(output))
            }

            Water { tint_color } => {
                let tint = tint_color.apply(info.biome(), self.settings);
                let mut output = new_sprite_buffer();
                const PARTIALS: [(&str, Aspect); 3] = [
                    ("water_flow", Aspect::BlockEast),
                    ("water_flow", Aspect::BlockSouth),
                    ("water_still", Aspect::BlockTop),
                ];
                for (texture_name, aspect) in PARTIALS {
                    canvas::overlay(
                        &mut output,
                        &*self.partials.get_tinted(texture_name, aspect, tint)?,
                    );
                }
                Ok(Some(output))
            }
        }
    }

    /// Create an asset for a solid block with a different top texture and same side textures.
    fn create_solid_block_top_side(
        &self,
        info: &AssetInfo,
        top_texture: &str,
        side_texture: &str,
    ) -> anyhow::Result<Option<SpriteBuffer>> {
        let output = match info.state.get_property("axis") {
            None | Some("y") => {
                let mut output = new_sprite_buffer();
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&side_texture, Aspect::BlockEast)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&side_texture, Aspect::BlockSouth)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&top_texture, Aspect::BlockTop)?,
                );
                output
            }
            Some("x") => {
                let mut output = new_sprite_buffer();
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&top_texture, Aspect::BlockEast)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self
                        .partials
                        .get(&side_texture, Aspect::BlockSouthRotated)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&side_texture, Aspect::BlockTopRotated)?,
                );
                output
            }
            Some("z") => {
                let mut output = new_sprite_buffer();
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&side_texture, Aspect::BlockEastRotated)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&top_texture, Aspect::BlockSouth)?,
                );
                canvas::overlay(
                    &mut output,
                    &*self.partials.get(&side_texture, Aspect::BlockTop)?,
                );
                output
            }
            Some(axis) => {
                return Err(anyhow!("unsupported axis value: {}", axis));
            }
        };
        Ok(Some(output))
    }

    /// Render a solid block with the 3 specified face textures.
    fn render_solid_block(
        &self,
        top_texture: &str,
        south_texture: &str,
        east_texture: &str,
    ) -> anyhow::Result<SpriteBuffer> {
        // TODO: unify this with "render_transparent_block()"
        let mut output = new_sprite_buffer();
        canvas::overlay(
            &mut output,
            &*self.partials.get(east_texture, Aspect::BlockEast)?,
        );
        canvas::overlay(
            &mut output,
            &*self.partials.get(south_texture, Aspect::BlockSouth)?,
        );
        canvas::overlay(
            &mut output,
            &*self.partials.get(top_texture, Aspect::BlockTop)?,
        );
        Ok(output)
    }

    /// Render a slightly more complex plant, where in-game a single texture is rendered in a #
    /// shape in the bottom-center of the block.
    fn render_crop(&self, texture_name: &str) -> anyhow::Result<SpriteBuffer> {
        let south = self.partials.get(texture_name, Aspect::BlockSouth)?;
        let south_back = south.view(0, 6, 2, 13);
        let south_mid = south.view(2, 7, 8, 16);
        let south_front = south.view(10, 11, 2, 13);
        let east = self.partials.get(texture_name, Aspect::BlockEast)?;
        let east_back = east.view(22, 6, 2, 13);
        let east_mid = east.view(14, 7, 8, 16);
        let east_front = east.view(12, 11, 2, 13);
        let mut output = new_sprite_buffer();
        canvas::overlay_at(&mut output, &south_back, 10, 1);
        canvas::overlay_at(&mut output, &east_back, 12, 1);
        canvas::overlay_at(&mut output, &south_back, 2, 5);
        canvas::overlay_at(&mut output, &east_mid, 4, 2);
        canvas::overlay_at(&mut output, &south_mid, 12, 2);
        canvas::overlay_at(&mut output, &east_back, 20, 5);
        canvas::overlay_at(&mut output, &east_front, 2, 6);
        canvas::overlay_at(&mut output, &south_mid, 4, 6);
        canvas::overlay_at(&mut output, &east_mid, 12, 6);
        canvas::overlay_at(&mut output, &south_front, 20, 6);
        canvas::overlay_at(&mut output, &east_front, 10, 10);
        canvas::overlay_at(&mut output, &south_front, 12, 10);
        Ok(output)
    }
}

pub const SPRITE_SIZE: usize = 24;
