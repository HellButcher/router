use std::sync::atomic::AtomicU64;

use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};

use router_types::country::CountryId;

use super::{
    SimpleHeader,
    attrib::{HighwayClass, SurfaceQuality, WayFlags},
    node::NO_WAY,
};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct WayId(pub i64);

/// Sentinel value for unresolved node indices (holds a raw NodeId cast).
pub const UNRESOLVED: u64 = u64::MAX;

#[repr(C)]
#[derive(Debug)]
pub struct Way {
    pub id: WayId,
    /// Before node-index resolution: holds the OSM `NodeId` cast to `u64`.
    /// After resolution: the node table index.
    pub from_node_idx: u64,
    /// Before node-index resolution: holds the OSM `NodeId` cast to `u64`.
    /// After resolution: the node table index.
    pub to_node_idx: u64,
    pub(crate) next_way: AtomicU64,
    pub(crate) next_way_reverse: AtomicU64,
    /// Maximum speed in km/h; 0 means use highway-class default.
    pub max_speed: u8,
    pub highway: HighwayClass,
    pub flags: WayFlags,
    /// Road surface quality tier. See [`SurfaceQuality`].
    pub surface_quality: SurfaceQuality,
    /// Country where this way segment is located.
    pub country_id: CountryId,
    _pad3: u8,
    /// Pre-computed haversine distance between `from_node` and `to_node` in metres,
    /// clamped to [`u16::MAX`] (≈ 65 km). Set during the node-index resolution pass.
    pub dist_m: u16,
}

const _: () = assert!(std::mem::size_of::<Way>() == 48);

impl Default for Way {
    fn default() -> Self {
        Self::new(WayId(0), UNRESOLVED, UNRESOLVED)
    }
}

impl Way {
    /// Create a new way. Pass `node_id.0 as u64` for `from_node_idx` and
    /// `to_node_idx` during PBF import; replace with actual table indices after
    /// the node-index resolution pass.
    #[inline]
    pub const fn new(id: WayId, from_node_idx: u64, to_node_idx: u64) -> Self {
        Self {
            id,
            from_node_idx,
            to_node_idx,
            next_way: AtomicU64::new(NO_WAY),
            next_way_reverse: AtomicU64::new(NO_WAY),
            max_speed: 0,
            highway: HighwayClass::Unknown,
            flags: WayFlags::empty(),
            surface_quality: SurfaceQuality::Unknown,
            country_id: CountryId::UNKNOWN,
            _pad3: 0,
            dist_m: 0,
        }
    }

    /// Returns the linked-list pointer to the next outbound way, or [`NO_WAY`].
    #[inline]
    pub fn next_way(&self) -> u64 {
        self.next_way.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the linked-list pointer to the next inbound way, or [`NO_WAY`].
    #[inline]
    pub fn next_way_reverse(&self) -> u64 {
        self.next_way_reverse
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the effective max speed in km/h, using the given highway-class
    /// default when no explicit `max_speed` tag was found (`max_speed == 0`).
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
    // The version number should be incremented whenever the in-memory representation of `Way` changes in a non-compatible way, such that old data files can no longer be read correctly.
    const VERSION: u32 = 2;
}
