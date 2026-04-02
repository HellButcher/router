use std::sync::atomic::AtomicI64;

use crate::{
    pod::{Item, TablePod},
    tablefile::TableData,
};

use super::{SimpleHeader, node::NodeId};

#[repr(transparent)]
#[derive(Copy, Clone, Debug, Default, PartialOrd, Ord, PartialEq, Eq)]
pub struct WayId(pub i64);

#[repr(C)]
#[derive(Debug, Default)]
pub struct Way {
    pub id: WayId,
    pub from_node: NodeId,
    pub to_node: NodeId,
    pub(crate) next_way: AtomicI64,
    pub(crate) next_way_reverse: AtomicI64,
}

impl Way {
    #[inline]
    pub const fn new(id: WayId, from_node: NodeId, to_node: NodeId) -> Self {
        Self {
            id,
            from_node,
            to_node,
            next_way: AtomicI64::new(0),
            next_way_reverse: AtomicI64::new(0),
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
