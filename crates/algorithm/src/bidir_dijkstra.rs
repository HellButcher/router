use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
};

use crate::Graph;

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
pub fn bidir_dijkstra(graph: impl Graph, start: usize, goal: usize) -> Option<(Vec<usize>, usize)> {
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

    let relax = |heap: &mut BinaryHeap<HeapState>,
                 dist: &mut HashMap<usize, usize>,
                 pred: &mut HashMap<usize, usize>,
                 from: usize,
                 to: usize,
                 cost: usize| {
        match dist.entry(to) {
            Entry::Occupied(mut e) if cost < *e.get() => {
                e.insert(cost);
                pred.insert(to, from);
                heap.push(HeapState { cost, position: to });
            }
            Entry::Vacant(e) => {
                e.insert(cost);
                pred.insert(to, from);
                heap.push(HeapState { cost, position: to });
            }
            _ => {}
        }
    };

    loop {
        let top_f = heap_f.peek().map(|s| s.cost).unwrap_or(usize::MAX);
        let top_b = heap_b.peek().map(|s| s.cost).unwrap_or(usize::MAX);

        // Termination: both frontiers' minimum costs exceed best found path.
        if top_f.saturating_add(top_b) >= best {
            break;
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
                relax(
                    &mut heap_f,
                    &mut dist_f,
                    &mut pred_f,
                    position,
                    edge.node,
                    next_cost,
                );
                // Check if this node has been reached by the backward search.
                if let Some(&back_cost) = dist_b.get(&edge.node) {
                    let total = next_cost.saturating_add(back_cost);
                    if total < best {
                        best = total;
                        meeting = Some(edge.node);
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
                relax(
                    &mut heap_b,
                    &mut dist_b,
                    &mut pred_b,
                    position,
                    edge.node,
                    next_cost,
                );
                // Check if this node has been reached by the forward search.
                if let Some(&fwd_cost) = dist_f.get(&edge.node) {
                    let total = fwd_cost.saturating_add(next_cost);
                    if total < best {
                        best = total;
                        meeting = Some(edge.node);
                    }
                }
            }
        }
    }

    let meeting = meeting?;

    // Reconstruct forward half: start → meeting.
    let mut path = Vec::new();
    let mut cur = meeting;
    loop {
        path.push(cur);
        if cur == start {
            break;
        }
        match pred_f.get(&cur) {
            Some(&prev) => cur = prev,
            None => break,
        }
    }
    path.reverse();

    // Reconstruct backward half: meeting → goal.
    // pred_b[node] = the node that was expanded when `node` was discovered,
    // i.e. the successor in the forward direction.
    cur = meeting;
    loop {
        match pred_b.get(&cur) {
            Some(&next) => {
                cur = next;
                path.push(cur);
                if cur == goal {
                    break;
                }
            }
            None => break,
        }
    }

    Some((path, best))
}
