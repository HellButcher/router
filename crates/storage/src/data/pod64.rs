use crate::{data::Versioned, pod::TablePod, tablefile::TableData};

use super::SimpleHeader;

/// A plain `u64` value stored in a [`TableFile`].
///
/// Used for temp files that carry a flat array of raw indices or offsets
/// (e.g. `node_refs.bin`, `node_ref_offsets.bin`).
#[repr(transparent)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pod64(pub u64);

unsafe impl TablePod for Pod64 {}

impl TableData for Pod64 {
    type Header = SimpleHeader<Pod64>;
}

impl Versioned for Pod64 {
    const VERSION: u32 = 1;
}
