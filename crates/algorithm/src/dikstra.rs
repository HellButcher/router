use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, HashSet, hash_map::Entry},
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

/// Core Dijkstra loop with a caller-supplied stop condition.
///
/// The `stop` closure is called each time a node is settled (with its optimal
/// cost confirmed).  When it returns `true` the search terminates immediately.
///
/// Returns the settled distances and predecessor map for all visited nodes.
pub fn dijkstra_with_stop_condition(
    graph: impl Graph,
    start: usize,
    mut stop: impl FnMut(usize) -> bool,
) -> (HashMap<usize, usize>, HashMap<usize, usize>) {
    let mut dist: HashMap<usize, usize> = HashMap::new();
    let mut predecessors: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(start, 0);
    heap.push(HeapState {
        cost: 0,
        position: start,
    });

    while let Some(HeapState { cost, position }) = heap.pop() {
        let old_cost = dist[&position];
        // Stale-entry check must come before the stop condition: if `stop` has
        // side effects (e.g. counting settled targets for SSMT), calling it on
        // a stale entry would act on a cost that is not yet optimal.
        if cost > old_cost {
            continue;
        }
        if stop(position) {
            break;
        }
        for edge in graph.outbound(position) {
            let next_cost = cost + edge.cost;
            match dist.entry(edge.node) {
                Entry::Occupied(mut e) if next_cost < *e.get() => {
                    e.insert(next_cost);
                    predecessors.insert(edge.node, position);
                    heap.push(HeapState {
                        cost: next_cost,
                        position: edge.node,
                    });
                }
                Entry::Vacant(e) => {
                    e.insert(next_cost);
                    predecessors.insert(edge.node, position);
                    heap.push(HeapState {
                        cost: next_cost,
                        position: edge.node,
                    });
                }
                _ => {}
            }
        }
    }

    (dist, predecessors)
}

/// Run Dijkstra's algorithm from `start` to `goal` on `graph`.
///
/// Returns `Some((path, cost))` where `path` is the sequence of node indices
/// from `start` to `goal` (inclusive) and `cost` is the total edge cost.
/// Returns `None` if no path exists.
pub fn dikstra(graph: impl Graph, start: usize, goal: usize) -> Option<(Vec<usize>, usize)> {
    let (dist, predecessors) = dijkstra_with_stop_condition(graph, start, |pos| pos == goal);
    let cost = *dist.get(&goal)?;
    let path = reconstruct_path(&predecessors, start, goal);
    Some((path, cost))
}

/// Run Dijkstra from `start`, exploring only nodes reachable within `budget`
/// cost units.  Returns the settled distance map for all such nodes.
///
/// Since the heap is a min-heap, the first settled node whose cost exceeds
/// `budget` means all remaining entries also exceed it, so the search
/// terminates early.  Edges whose total cost would exceed the budget are
/// also pruned from the heap to keep it small.
pub fn dijkstra_within_budget(
    graph: impl Graph,
    start: usize,
    budget: usize,
) -> HashMap<usize, usize> {
    let mut dist: HashMap<usize, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();

    dist.insert(start, 0);
    heap.push(HeapState {
        cost: 0,
        position: start,
    });

    while let Some(HeapState { cost, position }) = heap.pop() {
        if cost > *dist.get(&position).unwrap_or(&usize::MAX) {
            continue; // stale entry
        }
        if cost > budget {
            break; // min-heap: all remaining entries exceed budget too
        }
        for edge in graph.outbound(position) {
            let next_cost = cost + edge.cost;
            if next_cost > budget {
                continue; // prune: settling this node would exceed budget
            }
            match dist.entry(edge.node) {
                Entry::Occupied(mut e) if next_cost < *e.get() => {
                    e.insert(next_cost);
                    heap.push(HeapState {
                        cost: next_cost,
                        position: edge.node,
                    });
                }
                Entry::Vacant(e) => {
                    e.insert(next_cost);
                    heap.push(HeapState {
                        cost: next_cost,
                        position: edge.node,
                    });
                }
                _ => {}
            }
        }
    }

    dist
}

/// Run single-source, multi-target Dijkstra from `start` on `graph`.
///
/// Explores outbound edges until all `targets` are settled (or the graph is
/// exhausted).  Returns the settled cost for every reachable node and the
/// predecessor map needed for path reconstruction.
pub fn dijkstra_ssmt(
    graph: impl Graph,
    start: usize,
    targets: &HashSet<usize>,
) -> (HashMap<usize, usize>, HashMap<usize, usize>) {
    if targets.is_empty() {
        return (HashMap::new(), HashMap::new());
    }
    let mut remaining = targets.len();
    dijkstra_with_stop_condition(graph, start, |pos| {
        if targets.contains(&pos) {
            remaining -= 1;
            remaining == 0
        } else {
            false
        }
    })
}
