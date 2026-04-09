use std::{hash::Hasher, marker::PhantomData, mem::size_of};

use crate::pod::TablePod;

use self::{
    node::{NO_WAY, Node, NodeId},
    way::Way,
};

pub mod attrib;
pub mod dim_restriction;
pub mod node;
pub mod way;

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

pub trait Versioned {
    /// Version number for the data format. Must be incremented when the data format changes in a non-compatible way (eg: adding fields, changing field types, etc).
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
        let name_len = name_bytes.len();
        if name_len > 128 {
            r.name.copy_from_slice(&name_bytes[..128]);
        } else {
            r.name[..name_len].copy_from_slice(name_bytes);
        }
        r
    }
}

/// Link each way segment into its from- and to-node's adjacency linked lists.
///
/// Pointers stored in `first_way` / `next_way` are the way table index as
/// `u64`; [`NO_WAY`] (`u64::MAX`) is the end-of-list sentinel.
pub fn link_nodes_and_ways(nodes: &[Node], way_index: usize, way: &Way) {
    let ptr = way_index as u64;

    // During PBF import the fields hold the raw NodeId cast to u64.
    if let Ok(node_from_index) =
        nodes.binary_search_by_key(&NodeId(way.from_node_idx as i64), |n| n.id)
    {
        let mut current = NO_WAY;
        while let Err(old) = nodes[node_from_index].first_way.compare_exchange(
            current,
            ptr,
            std::sync::atomic::Ordering::Acquire,
            std::sync::atomic::Ordering::Relaxed,
        ) {
            way.next_way
                .store(old, std::sync::atomic::Ordering::Release);
            current = old;
        }
    } else {
        // TODO: mark way as border
    };

    if let Ok(node_to_index) = nodes.binary_search_by_key(&NodeId(way.to_node_idx as i64), |n| n.id)
    {
        let mut current = NO_WAY;
        while let Err(old) = nodes[node_to_index].first_way_reverse.compare_exchange(
            current,
            ptr,
            std::sync::atomic::Ordering::Acquire,
            std::sync::atomic::Ordering::Relaxed,
        ) {
            way.next_way_reverse
                .store(old, std::sync::atomic::Ordering::Release);
            current = old;
        }
    } else {
        // TODO: mark way as border
    }
}

/// Convert a stored linked-list pointer back to a way table index.
/// Returns `None` for [`NO_WAY`] (end-of-list sentinel).
#[inline]
pub fn way_index_from_ptr(ptr: u64) -> Option<usize> {
    if ptr == NO_WAY {
        None
    } else {
        Some(ptr as usize)
    }
}
