use std::sync::atomic::AtomicU64;

use bitflags::bitflags;
use router_types::country::CountryId;

use crate::{data::Versioned, pod::TablePod, tablefile::TableData};

use super::{SimpleHeader, node::NO_EDGE};

bitflags! {
    /// Per-direction vehicle-access restrictions for a single edge.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct EdgeFlags: u8 {
        /// Motor vehicles not allowed.
        const NO_MOTOR   = 0x01;
        /// HGV (heavy goods vehicles) not allowed.
        const NO_HGV     = 0x02;
        /// Bicycles not allowed.
        const NO_BICYCLE = 0x04;
        /// Pedestrians not allowed.
        const NO_FOOT    = 0x08;
    }
}

unsafe impl bytemuck::Zeroable for EdgeFlags {}
unsafe impl bytemuck::Pod for EdgeFlags {}

#[repr(C)]
#[derive(Debug)]
pub struct Edge {
    /// Before node-index resolution: raw NodeId cast to u64. After: node table index.
    pub from_node_idx: u64,
    /// Before node-index resolution: raw NodeId cast to u64. After: node table index.
    pub to_node_idx: u64,
    pub(crate) next_edge: AtomicU64,
    pub(crate) next_edge_reverse: AtomicU64,
    /// Before Way-index resolution: raw WayId cast to u64. After: Way table index.
    pub way_idx: u64,
    /// Haversine distance in metres between from- and to-node.
    /// Set during Phase 4 resolution; 0 before that.
    pub dist_m: u16,
    /// Per-direction max speed in km/h; 0 means fall back to `Way::max_speed`.
    pub max_speed: u8,
    /// Per-direction vehicle access restrictions.
    pub flags: EdgeFlags,
    /// Country where this edge segment is located. Set during Phase 4 resolution.
    pub country_id: CountryId,
    _pad: [u8; 3],
}

const _: () = assert!(std::mem::size_of::<Edge>() == 48);

impl Edge {
    /// Construct an edge for PBF import.
    ///
    /// `from` / `to` are raw `NodeId` values cast to `u64`.
    /// `way_id_raw` is the OSM `WayId` — stored in `way_idx` until Phase 4
    /// overwrites it with the Way table index via [`Edge::resolve`].
    pub fn new(from: u64, to: u64, way_id_raw: i64, flags: EdgeFlags, max_speed: u8) -> Self {
        Self {
            from_node_idx: from,
            to_node_idx: to,
            next_edge: AtomicU64::new(NO_EDGE),
            next_edge_reverse: AtomicU64::new(NO_EDGE),
            way_idx: way_id_raw as u64,
            dist_m: 0,
            max_speed,
            flags,
            country_id: CountryId::UNKNOWN,
            _pad: [0; 3],
        }
    }

    /// Way table index (valid after Phase 4 resolution).
    #[inline]
    pub fn way_idx(&self) -> usize {
        self.way_idx as usize
    }

    #[inline]
    pub fn from_node_idx(&self) -> usize {
        self.from_node_idx as usize
    }

    #[inline]
    pub fn to_node_idx(&self) -> usize {
        self.to_node_idx as usize
    }

    /// Phase 4: replace raw WayId with resolved `way_idx`, and store `dist_m`
    /// and `country_id`.
    pub fn resolve(&mut self, way_idx: usize, dist_m: u16, country_id: CountryId) {
        self.way_idx = way_idx as u64;
        self.dist_m = dist_m;
        self.country_id = country_id;
    }

    #[inline]
    pub fn next_edge(&self) -> usize {
        self.next_edge.load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    #[inline]
    pub fn next_edge_reverse(&self) -> usize {
        self.next_edge_reverse
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }
}

unsafe impl TablePod for Edge {}

impl TableData for Edge {
    type Header = SimpleHeader<Edge>;
}

impl Versioned for Edge {
    const VERSION: u32 = 1;
}
