use std::borrow::Cow;

use serde::Deserialize;

use crate::proplist::DefaultPropList as PropList;

#[derive(Debug, Deserialize)]
pub(super) struct Chunk<'a> {
    // #[serde(rename = "DataVersion")]
    // pub data_version: u32,
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
    #[serde(rename = "BlockLight")]
    #[serde(borrow)]
    pub block_light: Option<fastnbt::borrow::ByteArray<'a>>,
    #[serde(rename = "SkyLight")]
    pub sky_light: Option<fastnbt::ByteArray>,
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
    pub properties: Option<PropList>,
}

#[derive(Deserialize, derive_more::Debug)]
pub(super) struct Biomes<'a> {
    #[serde(borrow)]
    pub palette: Vec<Cow<'a, str>>,
    #[serde(borrow)]
    #[debug(ignore)]
    pub data: Option<fastnbt::borrow::LongArray<'a>>,
}
