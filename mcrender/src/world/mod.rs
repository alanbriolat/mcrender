/*
Anvil file format notes:

- A region's chunk offset table is ordered by (Z, X).
- A chunk's blocks are ordered by (Y, Z, X).
 */

mod cache;
mod nbt;
pub use cache::{ChunkBounds, ChunkCache};

use std::cmp::{max, min};
use std::collections::{BTreeMap, HashMap};
use std::fmt::Write;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::{fs, io};

use anyhow::anyhow;
use arcstr::ArcStr;
use bitfields::bitfield;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::Buf;
use derivative::Derivative; // TODO: replace with derive_more::Debug

use crate::coords::{CoordsXZ, CoordsXZY, IndexXZ, IndexXZY};
use crate::proplist::DefaultPropList as PropList;
use crate::settings::{AssetRenderSpec, AssetRule, Settings};
use crate::util::intern_str;

const SECTOR_SIZE: usize = 4096;
pub const REGION_SIZE: u32 = 32;
const REGION_HEADER_SIZE: usize = 2 * SECTOR_SIZE;
const REGION_CHUNK_COUNT: usize = (REGION_SIZE * REGION_SIZE) as usize;
pub const CHUNK_SIZE: u32 = 16;
const SECTION_BLOCK_COUNT: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;
const SECTION_BIOME_COUNT: usize = SECTION_BLOCK_COUNT / (4 * 4 * 4) as usize;
pub const WORLD_HEIGHT: u32 = 384;

const COMPRESSION_METHOD_ZLIB: u8 = 2;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum DimensionID {
    Overworld,
    Nether,
    TheEnd,
    // Other(String),
}

/// Global region coordinates.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Display,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
pub struct RCoords(pub CoordsXZ);

impl RCoords {
    pub fn to_chunk_coords(self) -> CCoords {
        CCoords((self.x() * REGION_SIZE as i32, self.z() * REGION_SIZE as i32).into())
    }
}

/// Global chunk coordinates.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Display,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
pub struct CCoords(pub CoordsXZ);

impl CCoords {
    pub fn south(self) -> Self {
        Self((self.x(), self.z() + 1).into())
    }

    pub fn east(self) -> Self {
        Self((self.x() + 1, self.z()).into())
    }

    pub fn to_region_coords(self) -> (RCoords, CIndex) {
        (
            RCoords(
                (
                    self.x().div_euclid(REGION_SIZE as i32),
                    self.z().div_euclid(REGION_SIZE as i32),
                )
                    .into(),
            ),
            CIndex(
                (
                    self.x().rem_euclid(REGION_SIZE as i32) as u32,
                    self.z().rem_euclid(REGION_SIZE as i32) as u32,
                )
                    .into(),
            ),
        )
            .into()
    }
}

/// 2D chunk index within a region.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Display,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
pub struct CIndex(pub IndexXZ);

impl CIndex {
    pub fn to_chunk_coords(self, region_coords: RCoords) -> CCoords {
        CCoords(
            (
                region_coords.x() * REGION_SIZE as i32 + self.x() as i32,
                region_coords.z() * REGION_SIZE as i32 + self.z() as i32,
            )
                .into(),
        )
    }

    fn to_flat_index(self) -> usize {
        (self.z() * REGION_SIZE + self.x()) as usize
    }

    fn from_flat_index(index: usize) -> Self {
        assert!(
            index < (REGION_SIZE * REGION_SIZE) as usize,
            "not a valid region chunk index"
        );
        Self((index as u32 % REGION_SIZE, index as u32 / REGION_SIZE).into())
    }
}

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Display,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
pub struct BCoords(pub CoordsXZY);

#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Hash,
    derive_more::From,
    derive_more::Into,
    derive_more::Display,
    derive_more::Deref,
    derive_more::DerefMut,
    derive_more::Add,
    derive_more::AddAssign,
    derive_more::Sub,
    derive_more::SubAssign,
    derive_more::Mul,
    derive_more::MulAssign,
)]
pub struct BIndex(pub IndexXZY);

impl BIndex {
    #[inline(always)]
    pub fn up(self) -> Self {
        (self.0 + (0, 0, 1).into()).into()
    }

    #[inline(always)]
    pub fn south(self) -> Self {
        (self.0 + (0, 1, 0).into()).into()
    }

    #[inline(always)]
    pub fn east(self) -> Self {
        (self.0 + (1, 0, 0).into()).into()
    }

    pub fn to_flat_index(self) -> usize {
        (self.y() * CHUNK_SIZE * CHUNK_SIZE + self.z() * CHUNK_SIZE + self.x()) as usize
    }

    pub fn from_flat_index(index: usize) -> Self {
        assert!(
            index < (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize,
            "not a valid section block index"
        );
        let x = index & 0xF;
        let z = (index >> 4) & 0xF;
        let y = (index >> 8) & 0xF;
        Self((x as u32, z as u32, y as u32).into())
    }

    fn to_biome_index(self) -> usize {
        let index = ((self.y() / 4) << 4) | ((self.z() / 4) << 2) | (self.x() / 4);
        let index = index as usize;
        assert!(
            index < SECTION_BIOME_COUNT,
            "not a valid section block index"
        );
        index
    }
}

#[derive(Debug)]
pub struct WorldInfo {
    pub path: PathBuf,
    pub dimensions: HashMap<DimensionID, DimensionInfo>,
}

impl WorldInfo {
    pub fn try_from_path(path: PathBuf) -> anyhow::Result<Self> {
        let mut dimensions = HashMap::new();
        if let Ok(dimension_info) = DimensionInfo::try_from_path(path.clone()) {
            dimensions.insert(DimensionID::Overworld, dimension_info);
        }
        if let Ok(dimension_info) = DimensionInfo::try_from_path(path.join("DIM-1")) {
            dimensions.insert(DimensionID::Nether, dimension_info);
        }
        if let Ok(dimension_info) = DimensionInfo::try_from_path(path.join("DIM1")) {
            dimensions.insert(DimensionID::TheEnd, dimension_info);
        }
        if dimensions.is_empty() {
            Err(anyhow!("No dimensions found"))
        } else {
            Ok(Self { path, dimensions })
        }
    }

    pub fn get_dimension(&self, id: &DimensionID) -> Option<&DimensionInfo> {
        self.dimensions.get(id)
    }
}

#[derive(Debug)]
pub struct DimensionInfo {
    pub path: PathBuf,
    pub regions: BTreeMap<RCoords, RegionInfo>,
}

impl DimensionInfo {
    pub fn try_from_path(path: PathBuf) -> anyhow::Result<Self> {
        log::debug!("DimensionInfo::try_from_path: {:?}", path);
        let regions_path = path.join("region");
        if !regions_path.is_dir() {
            return Err(anyhow!("not a dimension directory"));
        }
        let mut regions = BTreeMap::new();
        for entry in fs::read_dir(regions_path)? {
            if let Ok(region) = RegionInfo::try_from_path(entry?.path()) {
                regions.insert(region.coords, region);
            }
        }
        if regions.len() == 0 {
            return Err(anyhow!("no regions found"));
        }
        Ok(Self { path, regions })
    }

    pub fn get_region(&self, region_coords: RCoords) -> Option<&RegionInfo> {
        self.regions.get(&region_coords)
    }

    /// Get region coordinates such that all existing regions have coordinates `X >= min.x()` and `Z >= min.z()`.
    pub fn min_region_coords(&self) -> RCoords {
        self.regions
            .keys()
            .cloned()
            .reduce(|acc, k| RCoords((min(acc.x(), k.x()), min(acc.z(), k.z())).into()))
            .unwrap()
    }

    /// Get region coordinates such that all existing regions have coordinates `X < max.x()` and `Z < max.z()`.
    pub fn max_region_coords(&self) -> RCoords {
        RCoords((1, 1).into())
            + self
                .regions
                .keys()
                .cloned()
                .reduce(|acc, k| RCoords((max(acc.x(), k.x()), max(acc.z(), k.z())).into()))
                .unwrap()
    }

    /// Get the raw chunk at `chunk_coords`, if such a chunk has data.
    pub fn get_raw_chunk(&self, chunk_coords: CCoords) -> anyhow::Result<Option<RawChunk>> {
        let (region_coords, chunk_index) = chunk_coords.to_region_coords();
        let Some(region_info) = self.regions.get(&region_coords) else {
            // No such region
            return Ok(None);
        };
        // TODO: cache open regions
        let mut region = region_info.open()?;
        let Some(mut raw_chunk) = region.get_raw_chunk(chunk_index)? else {
            // No chunk data for this chunk
            return Ok(None);
        };
        raw_chunk.coords = raw_chunk.index.to_chunk_coords(region_coords);
        Ok(Some(raw_chunk))
    }
}

#[derive(Clone, Debug)]
pub struct RegionInfo {
    pub coords: RCoords,
    pub path: PathBuf,
}

impl RegionInfo {
    pub fn try_from_path(path: PathBuf) -> anyhow::Result<Self> {
        if !path.is_file() {
            return Err(anyhow!("not a file"));
        }
        let filename = path
            .file_name()
            .unwrap()
            .to_str()
            .ok_or(anyhow!("invalid filename"))?;
        if let Some(next) = filename.strip_suffix(".mca")
            && let Some(next) = next.strip_prefix("r.")
            && let Some((raw_x, raw_z)) = next.split_once(".")
            && let Ok(x) = i32::from_str(raw_x)
            && let Ok(z) = i32::from_str(raw_z)
        {
            Ok(Self {
                coords: RCoords((x, z).into()),
                path,
            })
        } else {
            Err(anyhow!("not a region filename (r.X.Z.mca)"))
        }
    }

    pub fn open(&self) -> anyhow::Result<Region<File>> {
        let file = File::open(&self.path)?;
        Region::from_stream(self.clone(), file)
    }
}

pub struct Region<S: Read + Seek> {
    info: RegionInfo,
    chunks: [u32; REGION_CHUNK_COUNT],
    stream: S,
}

impl<S: Read + Seek> Region<S> {
    pub fn from_stream(info: RegionInfo, mut stream: S) -> anyhow::Result<Self> {
        stream.seek(SeekFrom::Start(0))?;
        let mut header = [0u8; REGION_HEADER_SIZE];
        let mut chunks = [0u32; REGION_CHUNK_COUNT];
        stream.read_exact(&mut header)?;
        let mut locations = &header[..(REGION_CHUNK_COUNT * 4)];
        for i in 0..REGION_CHUNK_COUNT {
            chunks[i] = locations.get_u32();
        }
        Ok(Self {
            info,
            chunks,
            stream,
        })
    }

    pub fn into_inner(self) -> S {
        self.stream
    }

    pub fn into_iter(self) -> RegionChunkIter<S> {
        RegionChunkIter {
            region: self,
            index_iter: 0..REGION_CHUNK_COUNT,
        }
    }

    pub fn info(&self) -> &RegionInfo {
        &self.info
    }

    pub fn get_raw_chunk(&mut self, chunk_index: CIndex) -> anyhow::Result<Option<RawChunk>> {
        let Some(mut raw_chunk) = self.get_raw_chunk_by_index(chunk_index.to_flat_index())? else {
            return Ok(None);
        };
        raw_chunk.index = chunk_index;
        Ok(Some(raw_chunk))
    }

    fn get_raw_chunk_by_index(&mut self, index: usize) -> anyhow::Result<Option<RawChunk>> {
        assert!(index < self.chunks.len());
        let offset_count = self.chunks[index];
        // Offset of 0 means there is no chunk data for this chunk
        if offset_count == 0 {
            return Ok(None);
        }

        // Seek to the start of the chunk data
        let offset = (offset_count >> 8) as u64 * SECTOR_SIZE as u64;
        // let raw_size = (offset_count & 0xFF) as u64 * SECTOR_SIZE as u64;
        self.stream.seek(SeekFrom::Start(offset))?;

        // Read the chunk header
        let compressed_size = self.stream.read_u32::<BigEndian>()?;
        let mut chunk_reader = (&mut self.stream).take(compressed_size as u64);
        let compression_method = chunk_reader.read_u8()?;

        // Decompress the chunk data
        if compression_method != COMPRESSION_METHOD_ZLIB {
            // Zlib
            return Err(anyhow!(
                "compression method not supported: {:?}",
                compression_method
            ));
        }
        let mut chunk_decoder = flate2::write::ZlibDecoder::new(vec![]);
        io::copy(&mut chunk_reader, &mut chunk_decoder)?;
        let chunk_data = chunk_decoder.finish()?;

        Ok(Some(RawChunk {
            data: chunk_data,
            index: Default::default(),
            coords: Default::default(),
        }))
    }
}

pub struct RegionChunkIter<S: Read + Seek> {
    region: Region<S>,
    index_iter: Range<usize>,
}

impl<S: Read + Seek> RegionChunkIter<S> {
    pub fn into_inner(self) -> Region<S> {
        self.region
    }
}

impl<S: Read + Seek> Iterator for RegionChunkIter<S> {
    type Item = anyhow::Result<RawChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(i) = self.index_iter.next() {
            match self.region.get_raw_chunk_by_index(i) {
                Ok(Some(mut raw_chunk)) => {
                    raw_chunk.index = CIndex::from_flat_index(i);
                    raw_chunk.coords = raw_chunk.index.to_chunk_coords(self.region.info.coords);
                    return Some(Ok(raw_chunk));
                }
                Ok(None) => continue,
                Err(e) => return Some(Err(e)),
            }
        }
        None
    }
}

fn fmt_byte_count<T>(v: &Vec<T>, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
    write!(f, "[.. {} bytes ..]", v.len())
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct RawChunk {
    pub index: CIndex,
    pub coords: CCoords,
    #[derivative(Debug(format_with = "fmt_byte_count"))]
    pub data: Vec<u8>,
}
impl RawChunk {
    pub fn parse(&self, settings: &Settings) -> anyhow::Result<Chunk> {
        let chunk_nbt: nbt::Chunk = fastnbt::from_bytes(self.data.as_slice())?;

        let mut chunk = Chunk {
            coords: CCoords((chunk_nbt.x_pos, chunk_nbt.z_pos).into()),
            sections: Vec::with_capacity(chunk_nbt.sections.len()),
            fully_generated: chunk_nbt.status == "minecraft:full",
        };
        let chunk_base_coords = BCoords(
            (
                chunk.coords.x() * CHUNK_SIZE as i32,
                chunk.coords.z() * CHUNK_SIZE as i32,
                chunk_nbt.y_pos * CHUNK_SIZE as i32,
            )
                .into(),
        );

        let mut sky_light_data: Vec<Option<fastnbt::ByteArray>> =
            Vec::with_capacity(chunk_nbt.sections.len());

        for section_nbt in chunk_nbt.sections.into_iter() {
            // Collect the block palette (the collection of unique block states that exist in this section)
            let mut block_palette = Vec::with_capacity(section_nbt.block_states.palette.len());
            for bs in section_nbt.block_states.palette.into_iter() {
                let name = intern_str(bs.name);
                let rule = settings.asset_rules.get_rule(&name);
                let mut properties = bs.properties.unwrap_or_else(|| PropList::new());
                // Filter properties to only those relevant to rendering
                rule.filter_properties(&mut properties);
                block_palette.push((BlockState { name, properties }, rule));
            }

            // Record the block state index for each block; if there is no data, then the indexes are all
            // 0 by default, i.e. the first block palette entry (correct according to chunk format)
            let mut block_data = [BlockData::new(); SECTION_BLOCK_COUNT];
            if let Some(data) = section_nbt.block_states.data {
                let palette_count = block_palette.len() as u64;
                let bits = max(4, u64::BITS - (palette_count - 1).leading_zeros()) as usize;
                let packing = u64::BITS as usize / bits;
                let mask = (1u64 << bits) - 1;
                data.iter()
                    .flat_map(|v| {
                        let mut v = v as u64;
                        std::iter::repeat_with(move || {
                            let next = v & mask;
                            v = v >> bits;
                            next as u16
                        })
                        .take(packing)
                    })
                    .take(SECTION_BLOCK_COUNT)
                    .zip(block_data.iter_mut())
                    .for_each(|(v, data)| {
                        data.set_state_index(v);
                    });
            }

            // Collect the biome palette (the collection of biome names used in this section)
            let biome_palette: Vec<_> = section_nbt
                .biomes
                .palette
                .into_iter()
                .map(|biome| intern_str(biome))
                .collect();

            // Record the biome index for each block; biomes indexes apply to 4x4x4 regions, not
            // individual blocks
            if let Some(data) = section_nbt.biomes.data {
                let palette_count = biome_palette.len() as u64;
                let bits = (u64::BITS - (palette_count - 1).leading_zeros()) as usize;
                let packing = u64::BITS as usize / bits;
                let mask = (1u64 << bits) - 1;
                let mut indices = [0u8; SECTION_BIOME_COUNT];
                data.iter()
                    .flat_map(|v| {
                        let mut v = v as u64;
                        std::iter::repeat_with(move || {
                            let next = v & mask;
                            v = v >> bits;
                            next as u8
                        })
                        .take(packing)
                    })
                    .take(SECTION_BIOME_COUNT)
                    .zip(indices.iter_mut())
                    .for_each(|(v, index)| {
                        *index = v;
                    });
                block_data.iter_mut().enumerate().for_each(|(i, data)| {
                    let block_index = BIndex::from_flat_index(i);
                    let biome_index = block_index.to_biome_index();
                    data.set_biome_index(indices[biome_index]);
                });
            }

            // If there's no block light data, then the block light is 0, which is the default value in the struct
            if let Some(data) = section_nbt.block_light {
                data.iter()
                    .flat_map(|v| {
                        let v = v as u8;
                        [v & 0xF, v >> 4]
                    })
                    .zip(block_data.iter_mut())
                    .for_each(|(v, block_data)| {
                        block_data.set_lighting(block_data.lighting().with_block(v));
                    });
            }

            // Save sky light data to process top-to-bottom after all sections have been converted
            sky_light_data.push(section_nbt.sky_light);

            let section = Section {
                base: BCoords(
                    (
                        chunk_base_coords.x(),
                        chunk_base_coords.z(),
                        section_nbt.y as i32 * CHUNK_SIZE as i32,
                    )
                        .into(),
                ),
                block_data,
                block_palette,
                biome_palette,
            };
            chunk.sections.push(section);
        }

        // Process the save sky light data top-to-bottom, because absent data needs to be propagated
        // (default for top of the chunk is full sky light, i.e. 0xFF for each byte)
        let mut sky_light = [-1i8; 2048];
        for (data, section) in sky_light_data
            .into_iter()
            .rev()
            .zip(chunk.sections.iter_mut().rev())
        {
            const LAYER_LEN: usize = (CHUNK_SIZE * CHUNK_SIZE) as usize / 2;
            if let Some(data) = data {
                // Have data for this section, so use it
                sky_light.copy_from_slice(&*data);
            } else {
                // No data for this section,  so duplicate the bottom layer of the section above
                for i in (LAYER_LEN..sky_light.len()).step_by(LAYER_LEN) {
                    sky_light.copy_within(0..LAYER_LEN, i);
                }
            }
            sky_light
                .iter()
                .copied()
                .flat_map(|v| {
                    let v = v as u8;
                    [v & 0xF, v >> 4]
                })
                .zip(section.block_data.iter_mut())
                .for_each(|(v, block_data)| {
                    block_data.set_lighting(block_data.lighting().with_sky(v));
                });
        }

        Ok(chunk)
    }
}

#[derive(Debug)]
pub struct Chunk {
    pub coords: CCoords,
    pub sections: Vec<Section>,
    pub fully_generated: bool,
}

impl Chunk {
    pub fn iter_blocks(&self) -> impl Iterator<Item = BlockInfo<'_>> {
        self.sections.iter().enumerate().flat_map(|(i, section)| {
            let y_offset = i * CHUNK_SIZE as usize;
            section.iter_blocks().map(move |block| BlockInfo {
                index: block.index + BIndex((0, 0, y_offset as u32).into()),
                ..block
            })
        })
    }
}

#[derive(Debug)]
pub struct Section {
    pub base: BCoords,
    pub block_data: [BlockData; SECTION_BLOCK_COUNT],
    pub block_palette: Vec<(BlockState, Arc<AssetRule>)>,
    pub biome_palette: Vec<ArcStr>,
}

impl Section {
    pub fn get_block(&self, index: BIndex) -> BlockInfo<'_> {
        let data = self.block_data[index.to_flat_index()];
        let (state, rule) = &self.block_palette[data.state_index() as usize];
        let biome = self.biome_palette[data.biome_index() as usize].clone();
        BlockInfo {
            index,
            state,
            biome,
            lighting: data.lighting(),
            render: rule.render.clone(),
        }
    }

    pub fn iter_blocks(&self) -> impl Iterator<Item = BlockInfo<'_>> {
        self.block_data.iter().enumerate().map(|(i, &data)| {
            let x = i & 0xF;
            let z = (i >> 4) & 0xF;
            let y = (i >> 8) & 0xF;
            let index = BIndex((x as u32, z as u32, y as u32).into());
            let (state, rule) = &self.block_palette[data.state_index() as usize];
            let biome = self.biome_palette[data.biome_index() as usize].clone();
            BlockInfo {
                index,
                state,
                biome,
                lighting: data.lighting(),
                render: rule.render.clone(),
            }
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct BlockState {
    pub name: ArcStr,
    pub properties: PropList,
}

impl BlockState {
    pub fn new(name: ArcStr) -> BlockState {
        BlockState {
            name,
            properties: PropList::new(),
        }
    }

    /// Get the name of the block without any namespace prefix, e.g. `water` instead of
    /// `minecraft:water`.
    pub fn short_name(&self) -> &str {
        let name = self.name.as_str();
        if let Some((_left, right)) = name.split_once(':') {
            right
        } else {
            name
        }
    }

    pub fn with_property<K: AsRef<str>, V: AsRef<str>>(mut self, key: K, value: V) -> Self {
        self.properties.insert(key.as_ref(), value.as_ref());
        self
    }

    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key)
    }
}

impl std::fmt::Display for BlockState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.name)?;
        if !self.properties.is_empty() {
            f.write_char('{')?;
            self.properties.fmt(f)?;
            f.write_char('}')?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct BlockInfo<'a> {
    // coords: BCoords,
    pub index: BIndex,
    pub state: &'a BlockState,
    pub biome: ArcStr,
    pub lighting: LightLevel,
    pub render: Arc<AssetRenderSpec>,
}

#[bitfield(u8)]
#[derive(Clone, Copy)]
pub struct LightLevel {
    #[bits(4)]
    block: u8,
    #[bits(4)]
    sky: u8,
}

impl LightLevel {
    #[inline(always)]
    pub fn with_block(mut self, v: u8) -> Self {
        self.set_block(v);
        self
    }

    #[inline(always)]
    pub fn with_sky(mut self, v: u8) -> Self {
        self.set_sky(v);
        self
    }

    #[inline(always)]
    pub fn effective(self) -> u8 {
        max(self.block(), self.sky())
    }
}

#[bitfield(u32)]
#[derive(Clone, Copy)]
pub struct BlockData {
    state_index: u16,
    biome_index: u8,
    #[bits(8)]
    lighting: LightLevel,
}
