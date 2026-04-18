use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};
use super::{
    SimpleHeader,
    attrib::{HighwayClass, SurfaceQuality, WayFlags},
    dim_restriction::DimRestriction,
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
    pub first_edge_idx: u64,
    pub flags: WayFlags,
    /// Maximum speed in km/h; 0 means use highway-class default.
    pub max_speed: u8,
    pub highway: HighwayClass,
    /// Road surface quality tier.
    pub surface_quality: SurfaceQuality,
    /// Physical dimension restrictions (0 in any field = no restriction).
    pub dim: DimRestriction,
    _pad: u8,
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
            first_edge_idx: NO_EDGE,
            flags: WayFlags::empty(),
            max_speed: 0,
            highway: HighwayClass::Unknown,
            surface_quality: SurfaceQuality::Unknown,
            dim: DimRestriction::NONE,
            _pad: 0,
        }
    }

    #[inline]
    pub fn effective_max_speed(&self, highway_default: u8) -> u8 {
        if self.max_speed > 0 {
            self.max_speed
        } else {
            highway_default
        }
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
    const VERSION: u32 = 5;
}
