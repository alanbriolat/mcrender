use std::borrow::Cow;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use image::GenericImageView;
use parking_lot::RwLock;

pub struct TextureCache {
    path: PathBuf,
    cache: RwLock<HashMap<Cow<'static, str>, Arc<image::RgbaImage>>>,
}

impl TextureCache {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            cache: RwLock::new(HashMap::new()),
        }
    }

    pub fn get(&self, name: &str) -> anyhow::Result<Arc<image::RgbaImage>> {
        // Try to get the texture with just a read lock
        if let Some(image) = self.cache.read().get(name) {
            return Ok(image.clone());
        }

        // Read the texture from the file, but don't hold the lock while we do so
        let texture_path = self.path.join(format!("{name}.png"));
        let original_texture = image::open(&texture_path)?.to_rgba8();
        let texture = original_texture.view(0, 0, 16, 16).to_image();

        // Get the write lock
        let mut cache = self.cache.write();
        if let Some(image) = cache.get(name) {
            // If something else populated the cache in the meantime, reuse that entry
            Ok(image.clone())
        } else {
            // Otherwise store the new cache entry
            let image = Arc::new(texture);
            cache.insert(Cow::Owned(name.to_owned()), image.clone());
            Ok(image)
        }
    }

    pub fn insert(&self, name: &str, image: image::RgbaImage) {
        let mut cache = self.cache.write();
        cache.insert(Cow::Owned(name.to_owned()), Arc::new(image));
    }
}
