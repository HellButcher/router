use super::{
    SimpleHeader,
    attrib::{HighwayClass, SurfaceQuality, WayFlags},
    dim_restriction::DimRestriction,
    edge::EdgeFlags,
};
use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct WayId(pub i64);

/// Shared OSM-way metadata. One or two entries per OSM way.
///
/// When a way has identical properties in both directions, a single entry covers both
/// (`DIRECTION_FORWARD` and `DIRECTION_BACKWARD` are both unset). When directions
/// differ, two consecutive entries are emitted with the same `id`: the first with
/// `DIRECTION_FORWARD | HAS_PAIR`, the second with `DIRECTION_BACKWARD | HAS_PAIR`.
///
/// `node_refs_idx` and `node_refs_count` index into `node_refs.bin`: the resolved
/// node-table indices for this way's geometry, in forward direction. Paired entries
/// share the same slice.
#[repr(C)]
#[derive(Debug, Clone)]
pub struct Way {
    pub id: WayId,
    pub flags: WayFlags,
    pub highway: HighwayClass,
    /// Road surface quality tier.
    pub surface_quality: SurfaceQuality,
    /// Per-direction vehicle access restrictions.
    pub access: EdgeFlags,
    /// Max speed in km/h for this direction (0 = use profile default for highway class).
    pub max_speed: u8,
    _pad: u8,
    /// Number of entries in `node_refs.bin` starting at `node_refs_idx`.
    pub node_refs_count: u16,
    /// Physical dimension restrictions (0 in any field = no restriction).
    pub dim: DimRestriction,
    _pad2: u32,
    /// Starting index into `node_refs.bin` for this way's node-table index list.
    pub node_refs_idx: u64,
}

const _: () = assert!(std::mem::size_of::<Way>() == 32);

impl Default for Way {
    fn default() -> Self {
        Self::new(WayId(0))
    }
}

impl Way {
    pub const fn new(id: WayId) -> Self {
        Self {
            id,
            flags: WayFlags::empty(),
            highway: HighwayClass::Unknown,
            surface_quality: SurfaceQuality::Unknown,
            access: EdgeFlags::empty(),
            max_speed: 0,
            _pad: 0,
            node_refs_count: 0,
            dim: DimRestriction::NONE,
            _pad2: 0,
            node_refs_idx: 0,
        }
    }

    /// True when this entry covers the forward direction (or both, if `HAS_PAIR` is unset).
    #[inline]
    pub fn is_forward(&self) -> bool {
        !self.flags.contains(WayFlags::DIRECTION_BACKWARD)
    }

    /// True when this entry covers the backward direction (or both, if `HAS_PAIR` is unset).
    #[inline]
    pub fn is_backward(&self) -> bool {
        !self.flags.contains(WayFlags::DIRECTION_FORWARD)
    }
}

unsafe impl TablePod for Way {}

impl Item for Way {
    #[inline]
    fn key(&self) -> u64 {
        self.id.0 as u64
    }
}

impl TableData for Way {
    type Header = SimpleHeader<Way>;
}

impl Versioned for Way {
    const VERSION: u32 = 7;
}
