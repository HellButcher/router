use std::sync::atomic::{AtomicU8, AtomicU32, AtomicU64};

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
    /// Head of the linked list of EdgeNodes that start at this node (outgoing).
    pub(crate) first_outgoing_edge_node_idx: AtomicU64,
    /// Head of the linked list of EdgeNodes that end at this node (incoming).
    pub(crate) first_incoming_edge_node_idx: AtomicU64,
    /// Access restrictions and routing hints derived from OSM node tags.
    pub flags: AtomicU8,
    _pad: [u8; 3],
    pub num_refs: AtomicU32,
}

const _: () = assert!(std::mem::size_of::<Node>() == 40);

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
            first_outgoing_edge_node_idx: AtomicU64::new(NO_EDGE),
            first_incoming_edge_node_idx: AtomicU64::new(NO_EDGE),
            flags: AtomicU8::new(0),
            _pad: [0; 3],
            num_refs: AtomicU32::new(0),
        }
    }

    #[inline]
    pub fn node_flags(&self) -> NodeFlags {
        NodeFlags::from_bits_truncate(self.flags.load(std::sync::atomic::Ordering::Relaxed))
    }

    #[inline]
    pub fn first_outgoing_edge_node_idx(&self) -> usize {
        self.first_outgoing_edge_node_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    #[inline]
    pub fn first_incoming_edge_node_idx(&self) -> usize {
        self.first_incoming_edge_node_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    pub fn is_connected(&self) -> bool {
        self.first_outgoing_edge_node_idx
            .load(std::sync::atomic::Ordering::Acquire)
            != NO_EDGE
            || self
                .first_incoming_edge_node_idx
                .load(std::sync::atomic::Ordering::Acquire)
                != NO_EDGE
    }
}

unsafe impl TablePod for Node {}

impl Item for Node {
    #[inline]
    fn key(&self) -> u64 {
        self.id.0 as u64
    }
}

impl TableData for Node {
    type Header = SimpleHeader<Node>;
}

impl Versioned for Node {
    const VERSION: u32 = 3;
}
