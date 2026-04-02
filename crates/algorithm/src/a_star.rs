use std::{cmp::Ordering, collections::{hash_map::Entry, BinaryHeap, HashMap}};

use crate::Graph;


#[derive(Copy, Clone, PartialEq, Eq)]
struct HeapState {
    cost: usize,
    position: usize,
}

impl Ord for HeapState {
    fn cmp(&self, other: &Self) -> Ordering {
        other.cost.cmp(&self.cost).then_with(|| self.position.cmp(&other.position))
    }
}

impl PartialOrd for HeapState {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}


pub fn a_star(graph: impl Graph, start: usize, goal: usize) {
    let mut dist = HashMap::new();
    let mut heap = BinaryHeap::new();
    dist.insert(start, 0);
    heap.push(HeapState {cost: 0, position: start});

    while let Some(HeapState{ position, .. }) = heap.pop() {
        if position == goal {
            break;
        }
        let old_cost = dist[&position];
        for edge in graph.outbound(position) {
            let mut next = HeapState { cost: old_cost + edge.cost, position: edge.node };
            match dist.entry(next.position) {
                Entry::Occupied(mut e) if next.cost < *e.get() => {
                    e.insert(next.cost);
                    next.cost += graph.heuristic(next.position, goal);
                    heap.push(next);
                }
                Entry::Vacant(e) => {
                    e.insert(next.cost);
                    next.cost += graph.heuristic(next.position, goal);
                    heap.push(next);
                }
                _ => {}
            }
        }
    }
}
