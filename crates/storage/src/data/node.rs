use std::sync::atomic::AtomicI64;

use router_types::coordinate::LatLon;

use crate::{
    pod::{Item, TablePod},
    tablefile::TableData,
};

use super::SimpleHeader;

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct NodeId(pub i64);

#[repr(C)]
#[derive(Debug, Default)]
pub struct Node {
    pub id: NodeId,
    pub pos: LatLon,
    pub(crate) first_way: AtomicI64,
    pub(crate) first_way_reverse: AtomicI64,
}

impl Node {
    #[inline]
    pub const fn new(id: NodeId, pos: LatLon) -> Self {
        Self {
            id,
            pos,
            first_way: AtomicI64::new(0),
            first_way_reverse: AtomicI64::new(0),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.first_way.load(std::sync::atomic::Ordering::Acquire) != 0
            || self
                .first_way_reverse
                .load(std::sync::atomic::Ordering::Acquire)
                != 0
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
