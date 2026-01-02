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
use derivative::Derivative;
use serde::Deserialize;

const SECTOR_SIZE: usize = 4096;
pub const REGION_SIZE: usize = 32;
const REGION_HEADER_SIZE: usize = 2 * SECTOR_SIZE;
const REGION_CHUNK_COUNT: usize = REGION_SIZE * REGION_SIZE;
pub const CHUNK_SIZE: usize = 16;
const SECTION_BLOCK_COUNT: usize = CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE;

const COMPRESSION_METHOD_ZLIB: u8 = 2;

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum DimensionID {
    Overworld,
    Nether,
    TheEnd,
    // Other(String),
}

/// Global region coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct RCoords {
    pub x: isize,
    pub z: isize,
}

/// Global chunk coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CCoords {
    pub x: isize,
    pub z: isize,
}

impl CCoords {
    pub fn to_region_coords(self) -> (RCoords, CIndex) {
        (
            RCoords {
                x: self.x.div_euclid(REGION_SIZE as isize),
                z: self.z.div_euclid(REGION_SIZE as isize),
            },
            CIndex {
                x: self.x.rem_euclid(REGION_SIZE as isize) as usize,
                z: self.z.rem_euclid(REGION_SIZE as isize) as usize,
            },
        )
    }
}

/// 2D chunk index within a region.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct CIndex {
    pub x: usize,
    pub z: usize,
}

impl CIndex {
    pub fn to_chunk_coords(self, region_coords: RCoords) -> CCoords {
        CCoords {
            x: region_coords.x * REGION_SIZE as isize + self.x as isize,
            z: region_coords.z * REGION_SIZE as isize + self.z as isize,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BCoords {
    pub x: isize,
    pub z: isize,
    pub y: isize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct BIndex {
    pub x: usize,
    pub z: usize,
    pub y: usize,
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
            && let Ok(x) = isize::from_str(raw_x)
            && let Ok(z) = isize::from_str(raw_z)
        {
            let coords = RCoords { x, z };
            Ok(Self { coords, path })
        } else {
            Err(anyhow!("not a region filename (r.X.Z.mca)"))
        }
    }

    pub fn open(&self) -> anyhow::Result<Region<File>> {
        let file = File::open(&self.path)?;
        Region::from_stream(self.clone(), file)
    }
    // pub fn iter_chunks(&self) -> impl Iterator<Item=anyhow::Result<Chunk>> {
    //     let file = File::open(&self.path).unwrap();
    //     let mut region = fastanvil::Region::from_stream(file).unwrap();
    //     let mut region_iter = region.iter();
    //     std::iter::from_fn(move || region_iter.next()).map(|result| {
    //         match result {
    //             Ok(chunk) => {
    //                 fastnbt::from_reader(chunk.data.as_slice()).context("reading NBT")
    //             },
    //             Err(err) => Err(anyhow!(err)),
    //         }
    //     })
    //     // std::iter::from_fn()
    //     // region.into_iter().map(|result| {
    //     //     match result {
    //     //         Ok(chunk) => {
    //     //             fastnbt::from_reader(chunk.data.as_slice()).context("reading NBT")
    //     //         },
    //     //         Err(err) => Err(anyhow!(err)),
    //     //     }
    //     // })
    //     // unimplemented!()
    // }
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

    fn read_chunk_data(&mut self, index: usize) -> anyhow::Result<Option<Vec<u8>>> {
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
        Ok(Some(chunk_data))
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
        while let Some(index) = self.index_iter.next() {
            match self.region.read_chunk_data(index) {
                Ok(Some(data)) => {
                    let index = CIndex {
                        x: index % 32,
                        z: index / 32,
                    };
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
        log::debug!("chunk_nbt: {:?}", chunk_nbt);

        let mut chunk = Chunk {
            coords: CCoords {
                x: chunk_nbt.x_pos as isize,
                z: chunk_nbt.z_pos as isize,
            },
            sections: Vec::with_capacity(chunk_nbt.sections.len()),
        };
        let chunk_base_coords = BCoords {
            x: chunk.coords.x * CHUNK_SIZE as isize,
            z: chunk.coords.z * CHUNK_SIZE as isize,
            y: chunk_nbt.y_pos as isize * CHUNK_SIZE as isize,
        };

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
                    let bits = max(4, u64::BITS - palette_count.leading_zeros()) as usize;
                    let packing = u64::BITS as usize / bits;
                    let mask = (1u64 << bits) - 1;
                    data.iter()
                        .flat_map(|v| {
                            let mut v = v as u64;
                            let status = chunk_nbt.status.to_string();
                            std::iter::repeat_with(move || {
                                let next = v & mask;
                                v = v >> bits;
                                if next >= palette_count {
                                    panic!("block index {} > palette_count {} (bits={}, packing={}, mask={}, status={})", next, palette_count, bits, packing, mask, status);
                                }
                                next as u16
                            })
                            .take(packing)
                        })
                        .take(SECTION_BLOCK_COUNT)
                        .collect()
                }
            };
            let section = Section {
                base: BCoords {
                    y: section_nbt.y as isize * CHUNK_SIZE as isize,
                    ..chunk_base_coords
                },
                block_palette,
                block_indices,
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
    }

    #[derive(Derivative, Deserialize)]
    #[derivative(Debug)]
    pub(super) struct BlockStates<'a> {
        pub palette: Vec<BlockState<'a>>,
        #[serde(borrow)]
        // #[derivative(Debug = "ignore")]
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
}

#[derive(Debug)]
pub struct Chunk {
    pub coords: CCoords,
    pub sections: Vec<Section>,
}

impl Chunk {
    pub fn iter_blocks(&self) -> impl Iterator<Item = (BIndex, &BlockState)> {
        self.sections.iter().enumerate().flat_map(|(i, section)| {
            let y_offset = i * CHUNK_SIZE;
            section.iter_blocks().map(move |(bindex, block_state)| {
                (
                    BIndex {
                        y: bindex.y + y_offset,
                        ..bindex
                    },
                    block_state,
                )
            })
        })
    }
}

#[derive(Debug)]
pub struct Section {
    pub base: BCoords,
    pub block_palette: Vec<BlockState>,
    pub block_indices: Vec<u16>,
}

impl Section {
    pub fn iter_blocks(&self) -> impl Iterator<Item = (BIndex, &BlockState)> {
        self.block_indices
            .iter()
            .enumerate()
            .map(|(i, &palette_index)| {
                let x = i & 0xF;
                let z = (i >> 4) & 0xF;
                let y = (i >> 8) & 0xF;
                (
                    BIndex { x, z, y },
                    &self.block_palette[palette_index as usize],
                )
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
}
