use std::sync::atomic::AtomicU64;

use crate::{data::Versioned, pod::TablePod, tablefile::TableData};

use super::{SimpleHeader, attrib::TurnFlags, edge::EdgeFlags, edge_node::NO_TURN};

/// A legal turn between two [`EdgeNode`]s at a shared intersection node.
///
/// Each `TurnEdge` participates in two linked lists simultaneously:
/// - the outbound list of `from_edge_node_idx` (forward search)
/// - the inbound list of `to_edge_node_idx` (backward search)
#[repr(C)]
#[derive(Debug)]
pub struct TurnEdge {
    /// Source [`EdgeNode`] (X in X→Y).
    pub from_edge_node_idx: u64,
    /// Destination [`EdgeNode`] (Y in X→Y).
    pub to_edge_node_idx: u64,
    /// Signed turn angle in degrees: negative = left, positive = right, ±180 = U-turn.
    pub turn_angle: i16,
    /// Vehicles prohibited by an OSM restriction (0 = unrestricted).
    pub restriction_mask: EdgeFlags,
    /// Flags derived from the via-node (traffic signals, toll booth).
    pub turn_flags: TurnFlags,
    _pad: [u8; 4],
    /// Next entry in the from-node's outbound list.
    pub(crate) next_outbound_idx: AtomicU64,
    /// Next entry in the to-node's inbound list.
    pub(crate) next_inbound_idx: AtomicU64,
}

const _: () = assert!(std::mem::size_of::<TurnEdge>() == 40);

impl TurnEdge {
    pub fn new(
        from_edge_node_idx: u64,
        to_edge_node_idx: u64,
        turn_angle: i16,
        restriction_mask: EdgeFlags,
        turn_flags: TurnFlags,
    ) -> Self {
        Self {
            from_edge_node_idx,
            to_edge_node_idx,
            turn_angle,
            restriction_mask,
            turn_flags,
            _pad: [0; 4],
            next_outbound_idx: AtomicU64::new(NO_TURN),
            next_inbound_idx: AtomicU64::new(NO_TURN),
        }
    }

    #[inline]
    pub fn from_edge_node_idx(&self) -> usize {
        self.from_edge_node_idx as usize
    }

    #[inline]
    pub fn to_edge_node_idx(&self) -> usize {
        self.to_edge_node_idx as usize
    }

    #[inline]
    pub fn next_outbound_idx(&self) -> usize {
        self.next_outbound_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    #[inline]
    pub fn next_inbound_idx(&self) -> usize {
        self.next_inbound_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }
}

unsafe impl TablePod for TurnEdge {}

impl TableData for TurnEdge {
    type Header = SimpleHeader<TurnEdge>;
}

impl Versioned for TurnEdge {
    const VERSION: u32 = 1;
}
