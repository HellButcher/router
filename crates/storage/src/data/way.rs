use std::sync::atomic::AtomicU64;

use super::{
    SimpleHeader,
    attrib::{HighwayClass, SurfaceQuality, WayFlags},
    dim_restriction::DimRestriction,
};
use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct WayId(pub i64);

/// Sentinel: no edge registered as `first_edge_idx` yet.
pub const NO_EDGE: u64 = u64::MAX;

/// Shared OSM-way metadata. One entry per OSM way (deduplicated across segments).
///
/// Topology (from/to nodes, linked-list pointers) lives in [`Edge`].
#[repr(C)]
#[derive(Debug)]
pub struct Way {
    pub id: WayId,
    /// Index of the first [`Edge`] in `edges.bin` that references this Way.
    /// Used by the inspect API. Set to [`NO_EDGE`] until Phase 4.
    pub first_edge_idx: AtomicU64,
    pub flags: WayFlags,
    pub highway: HighwayClass,
    /// Road surface quality tier.
    pub surface_quality: SurfaceQuality,
    _pad_0: u8,
    /// Physical dimension restrictions (0 in any field = no restriction).
    pub dim: DimRestriction,
}

const _: () = assert!(std::mem::size_of::<Way>() == 24);

impl Default for Way {
    fn default() -> Self {
        Self::new(WayId(0))
    }
}

impl Way {
    pub const fn new(id: WayId) -> Self {
        Self {
            id,
            first_edge_idx: AtomicU64::new(NO_EDGE),
            flags: WayFlags::empty(),
            _pad_0: 0,
            highway: HighwayClass::Unknown,
            surface_quality: SurfaceQuality::Unknown,
            dim: DimRestriction::NONE,
        }
    }

    /// Returns the index of the first edge on the way.
    #[inline]
    pub fn first_edge_idx(&self) -> usize {
        self.first_edge_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }
}

unsafe impl TablePod for Way {}

impl Item for Way {
    type Key = WayId;

    #[inline]
    fn key(&self) -> &WayId {
        &self.id
    }
}

impl TableData for Way {
    type Header = SimpleHeader<Way>;
}

impl Versioned for Way {
    const VERSION: u32 = 6;
}
