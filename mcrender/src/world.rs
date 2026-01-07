/*
Anvil file format notes:

- A region's chunk offset table is ordered by (Z, X).
- A chunk's blocks are ordered by (Y, Z, X).
 */
use std::cmp::max;
use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::ops::Range;
use std::path::PathBuf;
use std::str::FromStr;
use std::{fs, io};

use anyhow::anyhow;
use byteorder::{BigEndian, ReadBytesExt};
use bytes::Buf;
use derivative::Derivative; // TODO: replace with derive_more::Debug
use serde::Deserialize;

use crate::coords::{CoordsXZ, CoordsXZY, IndexXZ, IndexXZY};

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
        for entry in fs::read_dir(regions_path).unwrap() {
            if let Ok(region) = RegionInfo::try_from_path(entry?.path()) {
                regions.insert(region.coords, region);
            }
        }
        Ok(Self { path, regions })
    }

    pub fn get_region(&self, region_coords: RCoords) -> Option<&RegionInfo> {
        self.regions.get(&region_coords)
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
            index_iter: 0..REGION_CHUNK_COUNT as u32,
        }
    }

    pub fn info(&self) -> &RegionInfo {
        &self.info
    }

    fn read_chunk_data(&mut self, index: u32) -> anyhow::Result<Option<Vec<u8>>> {
        let offset_count = self.chunks[index as usize];
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
        Ok(Some(chunk_data))
    }
}

pub struct RegionChunkIter<S: Read + Seek> {
    region: Region<S>,
    index_iter: Range<u32>,
}

impl<S: Read + Seek> RegionChunkIter<S> {
    pub fn into_inner(self) -> Region<S> {
        self.region
    }
}

impl<S: Read + Seek> Iterator for RegionChunkIter<S> {
    type Item = anyhow::Result<RawChunk>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(index) = self.index_iter.next() {
            match self.region.read_chunk_data(index) {
                Ok(Some(data)) => {
                    let index = CIndex((index % 32, index / 32).into());
                    let coords = index.to_chunk_coords(self.region.info.coords);
                    return Some(Ok(RawChunk {
                        index,
                        coords,
                        data,
                    }));
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
    pub fn parse(&self) -> anyhow::Result<Chunk> {
        let chunk_nbt: nbt::Chunk = fastnbt::from_bytes(self.data.as_slice())?;

        let mut chunk = Chunk {
            coords: CCoords((chunk_nbt.x_pos, chunk_nbt.z_pos).into()),
            sections: Vec::with_capacity(chunk_nbt.sections.len()),
        };
        let chunk_base_coords = BCoords(
            (
                chunk.coords.x() * CHUNK_SIZE as i32,
                chunk.coords.z() * CHUNK_SIZE as i32,
                chunk_nbt.y_pos * CHUNK_SIZE as i32,
            )
                .into(),
        );

        for section_nbt in chunk_nbt.sections.iter() {
            let block_palette = section_nbt
                .block_states
                .palette
                .iter()
                .map(|bs| BlockState {
                    name: bs.name.clone().into_owned(),
                    properties: bs
                        .properties
                        .iter()
                        .flatten()
                        .map(|(k, v)| (k.clone().into_owned(), v.clone().into_owned()))
                        .collect(),
                })
                .collect();
            let block_indices = match section_nbt.block_states.data.as_ref() {
                None => Vec::from([0u16; SECTION_BLOCK_COUNT]),
                Some(data) => {
                    let palette_count = section_nbt.block_states.palette.len() as u64;
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
                        .collect()
                }
            };
            let biome_palette = section_nbt
                .biomes
                .palette
                .iter()
                .map(|biome| biome.clone().into_owned())
                .collect();
            let biome_indices = match section_nbt.biomes.data.as_ref() {
                None => Vec::from([0u8; SECTION_BIOME_COUNT]),
                Some(data) => {
                    let palette_count = section_nbt.biomes.palette.len() as u64;
                    let bits = (u64::BITS - (palette_count - 1).leading_zeros()) as usize;
                    let packing = u64::BITS as usize / bits;
                    let mask = (1u64 << bits) - 1;
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
                        .collect()
                }
            };
            let section = Section {
                base: BCoords(
                    (
                        chunk_base_coords.x(),
                        chunk_base_coords.z(),
                        section_nbt.y as i32 * CHUNK_SIZE as i32,
                    )
                        .into(),
                ),
                block_palette,
                block_indices,
                biome_palette,
                biome_indices,
            };
            chunk.sections.push(section);
        }
        Ok(chunk)
    }
}

mod nbt {
    use super::*;
    use std::borrow::Cow;

    #[derive(Debug, Deserialize)]
    pub(super) struct Chunk<'a> {
        #[serde(rename = "DataVersion")]
        pub data_version: u32,
        #[serde(rename = "xPos")]
        pub x_pos: i32,
        #[serde(rename = "zPos")]
        pub z_pos: i32,
        #[serde(rename = "yPos")]
        pub y_pos: i32,
        #[serde(rename = "Status")]
        pub status: Cow<'a, str>,
        #[serde(borrow)]
        pub sections: Vec<Section<'a>>,
    }

    #[derive(Debug, Deserialize)]
    pub(super) struct Section<'a> {
        #[serde(rename = "Y")]
        pub y: i8,
        #[serde(borrow)]
        pub block_states: BlockStates<'a>,
        #[serde(borrow)]
        pub biomes: Biomes<'a>,
    }

    #[derive(Deserialize, derive_more::Debug)]
    pub(super) struct BlockStates<'a> {
        pub palette: Vec<BlockState<'a>>,
        #[serde(borrow)]
        #[debug(ignore)]
        pub data: Option<fastnbt::borrow::LongArray<'a>>,
    }

    #[derive(Debug, Deserialize)]
    pub(super) struct BlockState<'a> {
        #[serde(rename = "Name")]
        #[serde(borrow)]
        pub name: Cow<'a, str>,
        #[serde(rename = "Properties")]
        pub properties: Option<HashMap<Cow<'a, str>, Cow<'a, str>>>,
    }

    #[derive(Deserialize, derive_more::Debug)]
    pub(super) struct Biomes<'a> {
        #[serde(borrow)]
        pub palette: Vec<Cow<'a, str>>,
        #[serde(borrow)]
        #[debug(ignore)]
        pub data: Option<fastnbt::borrow::LongArray<'a>>,
    }
}

#[derive(Debug)]
pub struct Chunk {
    pub coords: CCoords,
    pub sections: Vec<Section>,
}

impl Chunk {
    pub fn iter_blocks(&self) -> impl Iterator<Item = BlockRef<'_>> {
        self.sections.iter().enumerate().flat_map(|(i, section)| {
            let y_offset = i * CHUNK_SIZE as usize;
            section.iter_blocks().map(move |block| BlockRef {
                index: block.index + BIndex((0, 0, y_offset as u32).into()),
                ..block
            })
        })
    }
}

#[derive(Debug)]
pub struct Section {
    pub base: BCoords,
    pub block_palette: Vec<BlockState>,
    pub block_indices: Vec<u16>,
    pub biome_palette: Vec<String>,
    pub biome_indices: Vec<u8>,
}

impl Section {
    pub fn iter_blocks(&self) -> impl Iterator<Item = BlockRef<'_>> {
        self.block_indices
            .iter()
            .enumerate()
            .map(|(i, &palette_index)| {
                let x = i & 0xF;
                let z = (i >> 4) & 0xF;
                let y = (i >> 8) & 0xF;
                let index = BIndex((x as u32, z as u32, y as u32).into());
                let state = &self.block_palette[palette_index as usize];
                let biome_index_index = ((y >> 2) << 4) | ((z >> 2) << 2) | (x >> 2);
                let biome_index = self.biome_indices[biome_index_index] as usize;
                let biome = self.biome_palette[biome_index].as_str();
                BlockRef {
                    index,
                    state,
                    biome,
                }
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct BlockState {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}

impl BlockState {
    pub fn new(name: String) -> BlockState {
        BlockState {
            name,
            properties: BTreeMap::new(),
        }
    }

    pub fn with_property(mut self, key: String, value: String) -> Self {
        self.properties.insert(key, value);
        self
    }

    pub fn get_property(&self, key: &str) -> Option<&str> {
        self.properties.get(key).map(|v| v.as_str())
    }
}

#[derive(Clone, Debug)]
pub struct BlockRef<'a> {
    // coords: BCoords,
    pub index: BIndex,
    pub state: &'a BlockState,
    pub biome: &'a str,
}
