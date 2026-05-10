use std::{hash::Hasher, marker::PhantomData, mem::size_of};

use crate::pod::{IndexInfo, SupportsIndex, TableDataHeader, TablePod};

use self::{
    edge_node::{EdgeNode, EdgeNodeChain},
    node::{NO_EDGE, Node},
    turn_edge::TurnEdge,
};

pub mod attrib;
pub mod dim_restriction;
pub mod edge;
pub mod edge_node;
pub mod geometry;
pub mod node;
pub mod pod64;
pub mod turn_edge;
pub mod way;

// ── SimpleHeader ──────────────────────────────────────────────────────────────

#[repr(C)]
pub struct SimpleHeader<I> {
    hash: u64,
    version: u64,
    header_size: u64,
    data_size: u64,
    name: [u8; 128],
    _phantom: PhantomData<fn(&I)>,
}

unsafe impl<I> TablePod for SimpleHeader<I> {}

/// Default no-op: `SimpleHeader` files never carry an index.
impl<I> TableDataHeader for SimpleHeader<I> {}

impl<I: 'static + Versioned> SimpleHeader<I> {
    fn name_hash() -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        hash.write(std::any::type_name::<I>().as_bytes());
        hash.finish()
    }

    pub fn verify(&self) -> Result<(), VerifiicationError> {
        if self.hash != Self::name_hash() {
            Err(VerifiicationError::HashMismatch)
        } else if self.version != I::VERSION as u64 {
            Err(VerifiicationError::VersionMismatch)
        } else if self.header_size != size_of::<Self>() as u64 {
            Err(VerifiicationError::HeaderSizeMismatch)
        } else if self.data_size != size_of::<I>() as u64 {
            Err(VerifiicationError::DataSizeMismatch)
        } else {
            Ok(())
        }
    }
}

impl<I: 'static + Versioned> Default for SimpleHeader<I> {
    fn default() -> Self {
        let mut r = Self {
            hash: Self::name_hash(),
            version: I::VERSION as u64,
            header_size: size_of::<Self>() as u64,
            data_size: size_of::<I>() as u64,
            name: [0; 128],
            _phantom: PhantomData,
        };
        let name_bytes = std::any::type_name::<I>().as_bytes();
        let copy_len = name_bytes.len().min(128);
        r.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        r
    }
}

// ── HeaderWithIndex ───────────────────────────────────────────────────────────

/// Drop-in replacement for [`SimpleHeader`] that additionally stores sparse
/// lookup index metadata in the on-disk header block.
///
/// Use as `type Header = HeaderWithIndex<Self>` in [`TableData`] for any data
/// type whose [`TableFile`] should support [`build_index_sorted`].
///
/// [`TableData`]: crate::tablefile::TableData
/// [`TableFile`]: crate::tablefile::TableFile
/// [`build_index_sorted`]: crate::tablefile::TableFile::build_index_sorted
#[repr(C)]
pub struct HeaderWithIndex<I> {
    base: SimpleHeader<I>,
    // Set to zero until build_index_sorted is called.
    num_data_entries: u64,
    entries_per_block: u64,
    num_index_entries: u64,
}

// size_of::<HeaderWithIndex<_>>() = size_of::<SimpleHeader<_>>() + 24 = 160 + 24 = 184
// HEADER_SIZE = 184.next_multiple_of(512) = 512
const _: () = assert!(size_of::<HeaderWithIndex<()>>() == 184);

unsafe impl<I> TablePod for HeaderWithIndex<I> {}

impl<I> TableDataHeader for HeaderWithIndex<I> {
    fn index_info(&self) -> Option<IndexInfo> {
        if self.entries_per_block == 0 {
            return None;
        }
        Some(IndexInfo {
            num_data_entries: self.num_data_entries,
            entries_per_block: self.entries_per_block,
            num_index_entries: self.num_index_entries,
        })
    }
}

impl<I> SupportsIndex for HeaderWithIndex<I> {
    fn set_index_info(&mut self, info: IndexInfo) {
        self.num_data_entries = info.num_data_entries;
        self.entries_per_block = info.entries_per_block;
        self.num_index_entries = info.num_index_entries;
    }
}

impl<I: 'static + Versioned> HeaderWithIndex<I> {
    pub fn verify(&self) -> Result<(), VerifiicationError> {
        self.base.verify()
    }
}

impl<I: 'static + Versioned> Default for HeaderWithIndex<I> {
    fn default() -> Self {
        Self {
            base: SimpleHeader::default(),
            num_data_entries: 0,
            entries_per_block: 0,
            num_index_entries: 0,
        }
    }
}

// ── shared types ──────────────────────────────────────────────────────────────

pub trait Versioned {
    /// Version number for the data format. Must be incremented when the data
    /// format changes in a non-compatible way (e.g. adding or resizing fields).
    const VERSION: u32;
}

#[derive(Copy, Clone, Debug)]
pub enum VerifiicationError {
    HashMismatch,
    VersionMismatch,
    HeaderSizeMismatch,
    DataSizeMismatch,
}

impl VerifiicationError {
    pub fn description(&self) -> &'static str {
        match self {
            VerifiicationError::HashMismatch => "Header hash does not match expected value",
            VerifiicationError::VersionMismatch => "Header version does not match expected value",
            VerifiicationError::HeaderSizeMismatch => "Header size does not match expected value",
            VerifiicationError::DataSizeMismatch => "Data size does not match expected value",
        }
    }
}

impl From<VerifiicationError> for std::io::Error {
    fn from(e: VerifiicationError) -> std::io::Error {
        std::io::Error::new(std::io::ErrorKind::InvalidData, e.description())
    }
}

// ── edge-node → node linking ──────────────────────────────────────────────────

/// CAS-prepend `ptr` (an EdgeNode index) into:
/// - `from_node`'s outgoing EdgeNode list  (EdgeNodes that **start** here)
/// - `to_node`'s   incoming EdgeNode list  (EdgeNodes that **end**   here)
///
/// `next_out` and `next_in` are slots in caller-owned temporary arrays that
/// hold the "next" chain pointer for this EdgeNode's position in each list;
/// they must be initialised to `NO_EDGE` (`u64::MAX`) before the first call.
pub fn link_edge_node_to_nodes(from_node: &Node, to_node: &Node, ptr: u64, chain: &EdgeNodeChain) {
    use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
    let mut current = NO_EDGE;
    while let Err(old) = from_node
        .first_outgoing_edge_node_idx
        .compare_exchange(current, ptr, Acquire, Relaxed)
    {
        chain.next_outgoing.store(old, Release);
        current = old;
    }
    let mut current = NO_EDGE;
    while let Err(old) = to_node
        .first_incoming_edge_node_idx
        .compare_exchange(current, ptr, Acquire, Relaxed)
    {
        chain.next_incoming.store(old, Release);
        current = old;
    }
}

// ── turn-edge linking ─────────────────────────────────────────────────────────

/// Prepend `ptr` (the index of `te` in `turn_edges.bin`) into:
/// - `from_en`'s outbound turn list (forward search)
/// - `to_en`'s inbound turn list (backward search)
///
/// Safe to call single-threaded during import; uses relaxed atomics because
/// there is no concurrent reader at import time.
pub fn link_turn_edge(from_en: &EdgeNode, to_en: &EdgeNode, ptr: u64, te: &TurnEdge) {
    use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
    let mut current = edge_node::NO_TURN;
    while let Err(old) = from_en
        .first_outbound_turn_idx
        .compare_exchange(current, ptr, Acquire, Relaxed)
    {
        te.next_outbound_idx.store(old, Release);
        current = old;
    }
    let mut current = edge_node::NO_TURN;
    while let Err(old) = to_en
        .first_inbound_turn_idx
        .compare_exchange(current, ptr, Acquire, Relaxed)
    {
        te.next_inbound_idx.store(old, Release);
        current = old;
    }
}
