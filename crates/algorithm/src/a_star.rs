use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
};

use crate::{Graph, reconstruct_path};

#[derive(Copy, Clone, PartialEq, Eq)]
struct HeapState {
    /// f = g + h (priority for the heap).
    f_cost: usize,
    /// g = true cost from start to this node.
    g_cost: usize,
    position: usize,
}

impl Ord for HeapState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .f_cost
            .cmp(&self.f_cost)
            .then_with(|| self.position.cmp(&other.position))
    }
}

impl PartialOrd for HeapState {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Run A* from `start` to `goal` on `graph`.
///
/// Returns `Some((path, cost))` where `path` is the sequence of node indices
/// from `start` to `goal` (inclusive) and `cost` is the total true edge cost.
/// Returns `None` if no path exists.
pub fn a_star(graph: impl Graph, start: usize, goal: usize) -> Option<(Vec<usize>, usize)> {
    let mut dist: HashMap<usize, usize> = HashMap::new();
    let mut predecessors: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(start, 0);
    heap.push(HeapState {
        f_cost: graph.heuristic(start, goal),
        g_cost: 0,
        position: start,
    });

    while let Some(HeapState {
        g_cost, position, ..
    }) = heap.pop()
    {
        if position == goal {
            let path = reconstruct_path(&predecessors, start, goal);
            return Some((path, g_cost));
        }
        // Skip stale entries.
        if dist.get(&position).is_some_and(|&d| g_cost > d) {
            continue;
        }
        for edge in graph.outbound(position) {
            let next_g = g_cost + edge.cost;
            match dist.entry(edge.node) {
                Entry::Occupied(mut e) if next_g < *e.get() => {
                    e.insert(next_g);
                    predecessors.insert(edge.node, position);
                    heap.push(HeapState {
                        f_cost: next_g + graph.heuristic(edge.node, goal),
                        g_cost: next_g,
                        position: edge.node,
                    });
                }
                Entry::Vacant(e) => {
                    e.insert(next_g);
                    predecessors.insert(edge.node, position);
                    heap.push(HeapState {
                        f_cost: next_g + graph.heuristic(edge.node, goal),
                        g_cost: next_g,
                        position: edge.node,
                    });
                }
                _ => {}
            }
        }
    }

    None
}
