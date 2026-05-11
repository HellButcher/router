use std::sync::atomic::AtomicU64;

use router_types::country::CountryId;

use crate::{data::Versioned, pod::TablePod, tablefile::TableData};

use super::SimpleHeader;

pub const NO_TURN: u64 = u64::MAX;

/// Temporary per-EdgeNode chain record used only during import (Phase 4b / Phase 5).
///
/// Holds the "next" pointers for the two per-node EdgeNode linked lists that
/// are built in Phase 4b and consumed in Phase 5 to enumerate all
/// (incoming × outgoing) EdgeNode pairs at each intersection.
/// The file is deleted after TurnEdge construction.
#[repr(C)]
#[derive(Debug)]
pub struct EdgeNodeChain {
    /// Next EdgeNode in the from-node's outgoing list.
    pub(crate) next_outgoing: AtomicU64,
    /// Next EdgeNode in the to-node's incoming list.
    pub(crate) next_incoming: AtomicU64,
}

const _: () = assert!(std::mem::size_of::<EdgeNodeChain>() == 16);

impl EdgeNodeChain {
    pub const NONE: u64 = NO_TURN;

    pub fn next_outgoing(&self) -> usize {
        self.next_outgoing
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    pub fn next_incoming(&self) -> usize {
        self.next_incoming
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }
}

impl Default for EdgeNodeChain {
    fn default() -> Self {
        Self {
            next_outgoing: AtomicU64::new(NO_TURN),
            next_incoming: AtomicU64::new(NO_TURN),
        }
    }
}

unsafe impl crate::pod::TablePod for EdgeNodeChain {}

impl crate::tablefile::TableData for EdgeNodeChain {
    type Header = SimpleHeader<EdgeNodeChain>;
}

impl crate::data::Versioned for EdgeNodeChain {
    const VERSION: u32 = 1;
}

/// A directed compressed road segment — the routing node in the edge-based graph.
///
/// One `EdgeNode` is emitted per directed segment between two intersection nodes.
/// Bidirectional ways emit two `EdgeNode`s sharing the same geometry slice.
/// The **sign** of `geometry_len` encodes traversal direction:
/// - positive → forward (read `geometry[offset..offset+len]` left to right)
/// - negative → backward (read `geometry[offset..offset+|len|]` right to left)
#[repr(C)]
#[derive(Debug)]
pub struct EdgeNode {
    /// Index of the directional [`Way`] entry in `ways.bin`.
    pub way_idx: u64,
    /// Total Haversine distance of the segment in metres.
    pub dist_m: u32,
    /// Country where the segment's from-position is located.
    pub country_id: CountryId,
    _pad: u8,
    /// Signed geometry count. Sign encodes traversal direction; magnitude is slice length (≥ 2).
    /// positive → forward, negative → backward.
    pub geometry_len: i16,
    /// Head of outbound [`TurnEdge`] linked list (forward search).
    pub(crate) first_outbound_turn_idx: AtomicU64,
    /// Head of inbound [`TurnEdge`] linked list (backward search).
    pub(crate) first_inbound_turn_idx: AtomicU64,
    /// Index of the first geometry point in `geometry.bin` for this segment's way.
    pub geometry_from_idx: u64,
}

const _: () = assert!(std::mem::size_of::<EdgeNode>() == 40);

impl EdgeNode {
    /// `geometry_from_idx`: from-node index (= last stored node for backward edges).
    /// `geometry_len`: signed delta to to-node (`to = from + len`); positive = forward,
    ///   negative = backward; |geometry_len| ≥ 1.
    pub fn new(
        way_idx: u64,
        dist_m: u32,
        country_id: CountryId,
        geometry_from_idx: u64,
        geometry_len: i16,
    ) -> Self {
        debug_assert!(
            geometry_len.abs() >= 1,
            "geometry_len magnitude must be ≥ 1"
        );
        Self {
            way_idx,
            dist_m,
            country_id,
            _pad: 0,
            geometry_len,
            first_outbound_turn_idx: AtomicU64::new(NO_TURN),
            first_inbound_turn_idx: AtomicU64::new(NO_TURN),
            geometry_from_idx,
        }
    }

    #[inline]
    pub fn way_idx(&self) -> usize {
        self.way_idx as usize
    }

    /// From-node index in `geometry.bin`. For backward edges this is the last stored node.
    #[inline]
    pub fn geometry_from_idx(&self) -> usize {
        self.geometry_from_idx as usize
    }

    /// To-node index in `geometry.bin` (`geometry_from_idx + geometry_len`).
    /// For backward edges `geometry_to_idx() < geometry_from_idx()`.
    #[inline]
    pub fn geometry_to_idx(&self) -> usize {
        (self.geometry_from_idx as isize + self.geometry_len as isize) as usize
    }

    /// Number of geometry points in the slice (always ≥ 2).
    #[inline]
    pub fn geometry_count(&self) -> usize {
        self.geometry_len.unsigned_abs() as usize + 1
    }

    /// Storage range of geometry point indices (always ascending, direction-independent).
    #[inline]
    pub fn geometry_range(&self) -> std::ops::RangeInclusive<usize> {
        if self.is_backward() {
            self.geometry_to_idx()..=self.geometry_from_idx()
        } else {
            self.geometry_from_idx()..=self.geometry_to_idx()
        }
    }

    /// True if this EdgeNode traverses its geometry slice in reverse (backward direction).
    #[inline]
    pub fn is_backward(&self) -> bool {
        self.geometry_len < 0
    }

    #[inline]
    pub fn first_outbound_turn_idx(&self) -> usize {
        self.first_outbound_turn_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }

    #[inline]
    pub fn first_inbound_turn_idx(&self) -> usize {
        self.first_inbound_turn_idx
            .load(std::sync::atomic::Ordering::Relaxed) as usize
    }
}

unsafe impl TablePod for EdgeNode {}

impl TableData for EdgeNode {
    type Header = SimpleHeader<EdgeNode>;
}

impl Versioned for EdgeNode {
    const VERSION: u32 = 1;
}
