use std::sync::atomic::AtomicU64;

use crate::{
    pod::{Item, TablePod},
    tablefile::TableData,
};

use super::{
    SimpleHeader,
    attrib::{HighwayClass, WayFlags},
    node::{NO_WAY, NodeId},
};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct WayId(pub i64);

#[repr(C)]
#[derive(Debug)]
pub struct Way {
    pub id: WayId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub(crate) next_way: AtomicU64,
    pub(crate) next_way_reverse: AtomicU64,
    /// Maximum speed in km/h; 0 means use highway-class default.
    pub max_speed: u8,
    pub highway: HighwayClass,
    pub flags: WayFlags,
    pub _pad: u8,
}

impl Default for Way {
    fn default() -> Self {
        Self::new(WayId(0), NodeId(0), NodeId(0))
    }
}

impl Way {
    #[inline]
    pub const fn new(id: WayId, from_node: NodeId, to_node: NodeId) -> Self {
        Self {
            id,
            from_node,
            to_node,
            next_way: AtomicU64::new(NO_WAY),
            next_way_reverse: AtomicU64::new(NO_WAY),
            max_speed: 0,
            highway: HighwayClass::Unknown,
            flags: WayFlags::empty(),
            _pad: 0,
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
