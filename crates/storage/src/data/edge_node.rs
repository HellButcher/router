use std::sync::atomic::AtomicU64;

use router_types::country::CountryId;

use crate::{data::Versioned, pod::TablePod, tablefile::TableData};

use super::SimpleHeader;

pub const NO_TURN: u64 = u64::MAX;

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
    /// `geometry_len`: positive = forward traversal, negative = backward. |geometry_len| ≥ 2.
    pub fn new(
        way_idx: u64,
        dist_m: u32,
        country_id: CountryId,
        geometry_from_idx: u64,
        geometry_len: i16,
    ) -> Self {
        debug_assert!(
            geometry_len.abs() >= 2,
            "geometry_len magnitude must be ≥ 2"
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
    pub fn geometry_from_idx(&self) -> usize {
        self.geometry_from_idx as usize
    }

    /// Exclusive end index into `geometry.bin` (`geometry_from_idx + |geometry_len|`).
    #[inline]
    pub fn geometry_to_idx(&self) -> usize {
        self.geometry_from_idx as usize + self.geometry_count() as usize
    }

    /// Number of geometry points in the slice (always ≥ 2).
    #[inline]
    pub fn geometry_count(&self) -> u16 {
        self.geometry_len.unsigned_abs()
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
