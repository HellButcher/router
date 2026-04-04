use std::{hash::Hasher, marker::PhantomData, mem::size_of};

use crate::pod::TablePod;

use self::{
    node::{NO_WAY, Node, NodeId},
    way::Way,
};

pub mod attrib;
pub mod node;
pub mod way;

#[repr(C)]
pub struct SimpleHeader<I> {
    hash: u64,
    header_size: u64,
    data_size: u64,
    name: [u8; 128],
    _phantom: PhantomData<fn(&I)>,
}

unsafe impl<I> TablePod for SimpleHeader<I> {}

impl<I: 'static> SimpleHeader<I> {
    fn name_hash() -> u64 {
        let mut hash = std::hash::DefaultHasher::new();
        hash.write(std::any::type_name::<I>().as_bytes());
        hash.finish()
    }

    pub fn verify(&self) -> bool {
        self.hash == Self::name_hash()
            && self.header_size == size_of::<Self>() as u64
            && self.data_size == size_of::<I>() as u64
    }
}

impl<I: 'static> Default for SimpleHeader<I> {
    fn default() -> Self {
        let mut r = Self {
            hash: Self::name_hash(),
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

    if let Ok(node_to_index) =
        nodes.binary_search_by_key(&NodeId(way.to_node_idx as i64), |n| n.id)
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
