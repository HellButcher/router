use std::sync::atomic::AtomicU64;

use bitflags::bitflags;
use router_types::{country::CountryId, vehicle::VehicleType};

use crate::{data::Versioned, pod::TablePod, tablefile::TableData};

use super::{SimpleHeader, node::NO_EDGE};

bitflags! {
    /// Bitmask of vehicle types that are **blocked** on a way or turn.
    ///
    /// Used in `Way::access` (per-direction road restrictions) and
    /// `TurnEdge::restriction_mask` (per-turn OSM restrictions).
    /// `AccessFlags::empty()` means no vehicle is blocked (unrestricted).
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct AccessFlags: u8 {
        /// Private cars (passenger vehicles) not allowed.
        const NO_CAR        = 0x01;
        /// Motorcycles, mopeds, and scooters not allowed.
        const NO_MOTORCYCLE = 0x02;
        /// HGV (heavy goods vehicles) not allowed.
        const NO_HGV        = 0x04;
        /// Bicycles not allowed.
        const NO_BICYCLE    = 0x08;
        /// Pedestrians not allowed.
        const NO_FOOT       = 0x10;
    }
}

impl AccessFlags {
    /// All motor vehicles (cars + motorcycles + HGV) blocked.
    /// Alias for `NO_CAR | NO_MOTORCYCLE | NO_HGV`.
    pub const NO_MOTOR: Self = Self::NO_CAR.union(Self::NO_MOTORCYCLE).union(Self::NO_HGV);
    /// All vehicle types blocked.
    pub const ALL: Self = Self::NO_MOTOR
        .union(Self::NO_BICYCLE)
        .union(Self::NO_FOOT);

    /// Returns the flag bit that represents a block for `vehicle`.
    #[inline]
    pub const fn flag_for(vehicle: VehicleType) -> Self {
        match vehicle {
            VehicleType::Car => Self::NO_CAR,
            VehicleType::Hgv => Self::NO_HGV,
            VehicleType::Bicycle => Self::NO_BICYCLE,
            VehicleType::Foot => Self::NO_FOOT,
            VehicleType::Motorcycle => Self::NO_MOTORCYCLE,
        }
    }

    /// Returns `true` if `vehicle` is blocked by this flag set.
    #[inline]
    pub fn blocks(self, vehicle: VehicleType) -> bool {
        self.contains(Self::flag_for(vehicle))
    }

    /// Returns `true` if `vehicle` is allowed (not blocked) by this flag set.
    #[inline]
    pub fn allows(self, vehicle: VehicleType) -> bool {
        !self.blocks(vehicle)
    }
}

unsafe impl bytemuck::Zeroable for AccessFlags {}
unsafe impl bytemuck::Pod for AccessFlags {}

/// Compatibility alias; prefer `AccessFlags` in new code.
pub type EdgeFlags = AccessFlags;

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
    pub flags: AccessFlags,
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
    pub fn new(from: u64, to: u64, way_id_raw: i64, flags: AccessFlags, max_speed: u8) -> Self {
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
