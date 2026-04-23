use std::sync::atomic::AtomicU64;

use router_types::coordinate::LatLon;

use crate::{
    data::Versioned,
    pod::{Item, TablePod},
    tablefile::TableData,
};

use super::{SimpleHeader, attrib::NodeFlags};

pub const NO_EDGE: u64 = u64::MAX;

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct NodeId(pub i64);

#[repr(C)]
#[derive(Debug)]
pub struct Node {
    pub id: NodeId,
    pub pos: LatLon,
    pub(crate) first_edge_idx_outbound: AtomicU64,
    pub(crate) first_edge_idx_inbound: AtomicU64,
    /// Access restrictions and routing hints derived from OSM node tags.
    pub flags: NodeFlags,
    _pad: [u8; 7],
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
            first_edge_idx_outbound: AtomicU64::new(NO_EDGE),
            first_edge_idx_inbound: AtomicU64::new(NO_EDGE),
            flags: NodeFlags::empty(),
            _pad: [0; 7],
        }
    }

    /// Returns the linked-list pointer to the first outbound way, or [`NO_WAY`].
    #[inline]
    pub fn first_edge_idx_outbound(&self) -> usize {
        self.first_edge_idx_outbound
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    /// Returns the linked-list pointer to the first inbound way, or [`NO_WAY`].
    #[inline]
    pub fn first_edge_idx_inbound(&self) -> usize {
        self.first_edge_idx_inbound
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    pub fn is_connected(&self) -> bool {
        self.first_edge_idx_outbound
            .load(std::sync::atomic::Ordering::Acquire)
            != NO_EDGE
            || self
                .first_edge_idx_inbound
                .load(std::sync::atomic::Ordering::Acquire)
                != NO_EDGE
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
    const VERSION: u32 = 2;
}
