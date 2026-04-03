use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
};

use crate::{Graph, reconstruct_path};

#[derive(Copy, Clone, PartialEq, Eq)]
struct HeapState {
    cost: usize,
    position: usize,
}

impl Ord for HeapState {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| self.position.cmp(&other.position))
    }
}

impl PartialOrd for HeapState {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Run Dijkstra's algorithm from `start` to `goal` on `graph`.
///
/// Returns `Some((path, cost))` where `path` is the sequence of node indices
/// from `start` to `goal` (inclusive) and `cost` is the total edge cost.
/// Returns `None` if no path exists.
pub fn dikstra(graph: impl Graph, start: usize, goal: usize) -> Option<(Vec<usize>, usize)> {
    let mut dist: HashMap<usize, usize> = HashMap::new();
    let mut predecessors: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(start, 0);
    heap.push(HeapState {
        cost: 0,
        position: start,
    });

    while let Some(HeapState { cost, position }) = heap.pop() {
        if position == goal {
            let path = reconstruct_path(&predecessors, start, goal);
            return Some((path, cost));
        }
        let old_cost = dist[&position];
        if cost > old_cost {
            continue;
        }
        for edge in graph.outbound(position) {
            let next_cost = cost + edge.cost;
            let next = HeapState {
                cost: next_cost,
                position: edge.node,
            };
            match dist.entry(next.position) {
                Entry::Occupied(mut e) if next_cost < *e.get() => {
                    e.insert(next_cost);
                    predecessors.insert(next.position, position);
                    heap.push(next);
                }
                Entry::Vacant(e) => {
                    e.insert(next_cost);
                    predecessors.insert(next.position, position);
                    heap.push(next);
                }
                _ => {}
            }
        }
    }

    None
}
