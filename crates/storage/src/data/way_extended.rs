use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};

use super::{SimpleHeader, dim_restriction::DimRestriction, way::WayId};

/// Extended attributes for an OSM way, keyed by [`WayId`].
/// All segments of the same OSM way share one entry.
/// Only ways that have at least one extended attribute appear in `way_extended.bin`.
/// Use [`WayFlags::HAS_EXTENDED`] to check whether a lookup is needed.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct WayExtended {
    /// OSM way ID — used as the lookup key.
    pub id: WayId,
    pub dim: DimRestriction,
    _pad: [u8; 5],
}

const _: () = assert!(std::mem::size_of::<WayExtended>() == 16);

impl WayExtended {
    pub fn new(id: WayId, dim: DimRestriction) -> Self {
        Self {
            id,
            dim,
            _pad: [0; 5],
        }
    }
}

unsafe impl TablePod for WayExtended {}

impl Item for WayExtended {
    type Key = WayId;

    #[inline]
    fn key(&self) -> &WayId {
        &self.id
    }
}

impl TableData for WayExtended {
    type Header = SimpleHeader<WayExtended>;
}

impl Versioned for WayExtended {
    const VERSION: u32 = 1;
}
