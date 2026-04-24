/// Entry type for an ID-to-position index backed by [`TableFile`].
///
/// Open the index with `TableFile::<IdEntry>::open_read_only(path)` and look
/// up entries via `TableFile::find(&key)`.
///
/// Entries must be stored in ascending `key` order. After a Morton reorder the
/// file is rebuilt with the same sorted key column but updated `idx` values.
///
/// [`TableFile`]: crate::tablefile::TableFile
use crate::{
    data::{SimpleHeader, Versioned},
    pod::Item,
    tablefile::TableData,
};

/// One entry: an OSM ID (cast to `u64`) mapped to a row index in the primary
/// table file.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct IdEntry {
    pub key: u64,
    pub idx: u64,
}

impl Item for IdEntry {
    fn key(&self) -> u64 {
        self.key
    }
}

impl TableData for IdEntry {
    type Header = SimpleHeader<IdEntry>;
}

impl Versioned for IdEntry {
    const VERSION: u32 = 1;
}
