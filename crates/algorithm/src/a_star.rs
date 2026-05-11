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
pub fn a_star(
    graph: impl Graph,
    start: usize,
    goal: usize,
    construct_path: bool,
) -> Option<(Vec<usize>, usize)> {
    if start == goal {
        return Some((vec![start], 0));
    }
    let initial_cost_estimate = graph.heuristic(start, goal)?;
    let mut dist: HashMap<usize, usize> = HashMap::new();
    let mut predecessors: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(start, 0);
    heap.push(HeapState {
        f_cost: initial_cost_estimate,
        g_cost: 0,
        position: start,
    });

    while let Some(HeapState {
        g_cost, position, ..
    }) = heap.pop()
    {
        if position == goal {
            let path = if construct_path {
                reconstruct_path(&predecessors, start, goal)
            } else {
                Vec::new()
            };
            return Some((path, g_cost));
        }
        // Skip stale entries.
        if dist.get(&position).is_some_and(|&d| g_cost > d) {
            continue;
        }
        for edge in graph.outbound(position) {
            let next_g = g_cost + edge.cost;
            let improved = match dist.entry(edge.edge_node_idx) {
                Entry::Occupied(mut e) if next_g < *e.get() => {
                    e.insert(next_g);
                    true
                }
                Entry::Vacant(e) => {
                    e.insert(next_g);
                    true
                }
                _ => false,
            };
            if improved {
                predecessors.insert(edge.edge_node_idx, position);
                if let Some(remaining_cost_estimate) = graph.heuristic(edge.edge_node_idx, goal) {
                    heap.push(HeapState {
                        f_cost: next_g + remaining_cost_estimate,
                        g_cost: next_g,
                        position: edge.edge_node_idx,
                    });
                };
            }
        }
    }

    None
}
