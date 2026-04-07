use std::sync::atomic::AtomicU64;

use router_types::coordinate::LatLon;

use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};

use super::SimpleHeader;

pub const NO_WAY: u64 = u64::MAX;

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct NodeId(pub i64);

#[repr(C)]
#[derive(Debug)]
pub struct Node {
    pub id: NodeId,
    pub pos: LatLon,
    pub(crate) first_way: AtomicU64,
    pub(crate) first_way_reverse: AtomicU64,
}

impl Default for Node {
    fn default() -> Self {
        Self::new(NodeId(0), LatLon::ZERO)
    }
}

impl Node {
    #[inline]
    pub const fn new(id: NodeId, pos: LatLon) -> Self {
        Self {
            id,
            pos,
            first_way: AtomicU64::new(NO_WAY),
            first_way_reverse: AtomicU64::new(NO_WAY),
        }
    }

    /// Returns the linked-list pointer to the first outbound way, or [`NO_WAY`].
    #[inline]
    pub fn first_way(&self) -> u64 {
        self.first_way.load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Returns the linked-list pointer to the first inbound way, or [`NO_WAY`].
    #[inline]
    pub fn first_way_reverse(&self) -> u64 {
        self.first_way_reverse
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn is_connected(&self) -> bool {
        self.first_way.load(std::sync::atomic::Ordering::Acquire) != NO_WAY
            || self
                .first_way_reverse
                .load(std::sync::atomic::Ordering::Acquire)
                != NO_WAY
    }
}

unsafe impl TablePod for Node {}

impl Item for Node {
    type Key = NodeId;

    #[inline]
    fn key(&self) -> &NodeId {
        &self.id
    }
}

impl TableData for Node {
    type Header = SimpleHeader<Node>;
}

impl Versioned for Node {
    // The version number should be incremented whenever the in-memory representation of `Way` changes in a non-compatible way, such that old data files can no longer be read correctly.
    const VERSION: u32 = 1;
}
