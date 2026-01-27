use lru::LruCache;
use std::sync::Arc;

use crate::settings::Settings;
use crate::world::{CCoords, Chunk, DimensionInfo, RCoords, REGION_SIZE};

#[derive(Clone, Debug, Default)]
pub enum ChunkBounds {
    #[default]
    Unbounded,
    MinMax(CCoords, CCoords),
}

impl ChunkBounds {
    pub fn single_chunk(coords: CCoords) -> Self {
        ChunkBounds::MinMax(coords, coords + CCoords((1, 1).into()))
    }

    pub fn single_region(coords: RCoords) -> Self {
        let base = coords.to_chunk_coords();
        ChunkBounds::MinMax(
            base,
            base + CCoords((REGION_SIZE as i32, REGION_SIZE as i32).into()),
        )
    }

    pub fn contains(&self, coords: &CCoords) -> bool {
        match self {
            ChunkBounds::Unbounded => true,
            ChunkBounds::MinMax(min, max) => {
                (min.x()..max.x()).contains(&coords.x()) && (min.z()..max.z()).contains(&coords.z())
            }
        }
    }
}

pub struct ChunkCache<'i, 's> {
    dim_info: &'i DimensionInfo,
    settings: &'s Settings,
    bounds: ChunkBounds,
    cache: LruCache<CCoords, Option<Arc<Chunk>>>,
}

impl<'i, 's> ChunkCache<'i, 's> {
    pub fn new(
        dim_info: &'i DimensionInfo,
        settings: &'s Settings,
        bounds: ChunkBounds,
        capacity: usize,
    ) -> Self {
        Self {
            dim_info,
            settings,
            bounds,
            cache: LruCache::new(capacity.try_into().unwrap()),
        }
    }

    pub fn get(&mut self, coords: CCoords) -> Option<Arc<Chunk>> {
        if !self.bounds.contains(&coords) {
            return None;
        }

        self.cache
            .get_or_insert(coords, || {
                self.dim_info
                    .get_raw_chunk(coords)
                    .ok()
                    .flatten()
                    .and_then(|raw_chunk| raw_chunk.parse(self.settings).ok())
                    .filter(|chunk| chunk.fully_generated)
                    .map(Arc::new)
            })
            .clone()
    }
}
