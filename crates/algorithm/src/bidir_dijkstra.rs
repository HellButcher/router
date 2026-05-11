use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
};

use crate::{Graph, reconstruct_path_bidir};

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

/// Run bidirectional Dijkstra from `start` to `goal` on `graph`.
///
/// Runs a forward search from `start` and a backward search from `goal`
/// simultaneously, meeting in the middle. This typically explores roughly
/// half the nodes of a unidirectional search.
///
/// Returns `Some((path, cost))` where `path` is the sequence of node indices
/// from `start` to `goal` (inclusive) and `cost` is the total edge cost.
/// Returns `None` if no path exists.
pub fn bidir_dijkstra(
    graph: impl Graph,
    start: usize,
    goal: usize,
    construct_path: bool,
) -> Option<(Vec<usize>, usize)> {
    if start == goal {
        return Some((vec![start], 0));
    }

    // Forward search state (from start).
    let mut dist_f: HashMap<usize, usize> = HashMap::new();
    let mut pred_f: HashMap<usize, usize> = HashMap::new();
    let mut heap_f: BinaryHeap<HeapState> = BinaryHeap::new();

    // Backward search state (from goal, following inbound edges).
    let mut dist_b: HashMap<usize, usize> = HashMap::new();
    let mut pred_b: HashMap<usize, usize> = HashMap::new();
    let mut heap_b: BinaryHeap<HeapState> = BinaryHeap::new();

    dist_f.insert(start, 0);
    heap_f.push(HeapState {
        cost: 0,
        position: start,
    });

    dist_b.insert(goal, 0);
    heap_b.push(HeapState {
        cost: 0,
        position: goal,
    });

    // Best complete path cost found so far.
    let mut best = usize::MAX;
    // Node through which the best path passes.
    let mut meeting = None;

    loop {
        // Use Option so we can distinguish "empty heap" from "cost 0".
        let top_f = heap_f.peek().map(|s| s.cost);
        let top_b = heap_b.peek().map(|s| s.cost);

        // Termination: once both frontier minima sum to ≥ best no shorter path
        // can be found. If either heap is empty the search on that side is
        // exhausted; there is nothing left to explore.
        match (top_f, top_b) {
            (None, _) | (_, None) => break,
            (Some(f), Some(b)) if f.saturating_add(b) >= best => break,
            _ => {}
        }

        if top_f <= top_b {
            // Expand forward frontier.
            let Some(HeapState { cost, position }) = heap_f.pop() else {
                break;
            };
            if cost > dist_f.get(&position).copied().unwrap_or(usize::MAX) {
                continue;
            }
            for edge in graph.outbound(position) {
                let next_cost = cost + edge.cost;
                let improved = match dist_f.entry(edge.edge_node_idx) {
                    Entry::Occupied(mut e) if next_cost < *e.get() => {
                        e.insert(next_cost);
                        true
                    }
                    Entry::Vacant(e) => {
                        e.insert(next_cost);
                        true
                    }
                    _ => false,
                };
                if improved {
                    pred_f.insert(edge.edge_node_idx, position);
                    heap_f.push(HeapState {
                        cost: next_cost,
                        position: edge.edge_node_idx,
                    });
                    // next_cost == dist_f[edge.node] here (just committed).
                    if let Some(&back_cost) = dist_b.get(&edge.edge_node_idx) {
                        let total = next_cost.saturating_add(back_cost);
                        if total < best {
                            best = total;
                            meeting = Some(edge.edge_node_idx);
                        }
                    }
                }
            }
        } else {
            // Expand backward frontier.
            let Some(HeapState { cost, position }) = heap_b.pop() else {
                break;
            };
            if cost > dist_b.get(&position).copied().unwrap_or(usize::MAX) {
                continue;
            }
            for edge in graph.inbound(position) {
                let next_cost = cost + edge.cost;
                let improved = match dist_b.entry(edge.edge_node_idx) {
                    Entry::Occupied(mut e) if next_cost < *e.get() => {
                        e.insert(next_cost);
                        true
                    }
                    Entry::Vacant(e) => {
                        e.insert(next_cost);
                        true
                    }
                    _ => false,
                };
                if improved {
                    pred_b.insert(edge.edge_node_idx, position);
                    heap_b.push(HeapState {
                        cost: next_cost,
                        position: edge.edge_node_idx,
                    });
                    // next_cost == dist_b[edge.node] here (just committed).
                    if let Some(&fwd_cost) = dist_f.get(&edge.edge_node_idx) {
                        let total = fwd_cost.saturating_add(next_cost);
                        if total < best {
                            best = total;
                            meeting = Some(edge.edge_node_idx);
                        }
                    }
                }
            }
        }
    }

    let path = if construct_path {
        reconstruct_path_bidir(&pred_f, &pred_b, start, meeting?, goal)
    } else {
        Vec::new()
    };

    Some((path, best))
}
