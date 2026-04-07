use std::{
    cmp::Ordering,
    collections::{BinaryHeap, HashMap, hash_map::Entry},
};

use crate::Graph;

#[derive(Copy, Clone, PartialEq, Eq)]
struct HeapState {
    /// f = g + h (priority for the heap).
    f_cost: usize,
    /// g = true cost from the search origin to this node.
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

/// Run bidirectional A* from `start` to `goal` on `graph`.
///
/// Runs a forward A* search from `start` and a backward A* search from `goal`
/// simultaneously, using the graph's heuristic to guide both frontiers.
///
/// The backward search uses `graph.heuristic(node, start)` as its heuristic,
/// which must be admissible for the reverse direction (this holds when the
/// heuristic is a symmetric lower bound such as straight-line distance).
///
/// Returns `Some((path, cost))` where `path` is the sequence of node indices
/// from `start` to `goal` (inclusive) and `cost` is the total true edge cost.
/// Returns `None` if no path exists.
pub fn bidir_a_star(graph: impl Graph, start: usize, goal: usize) -> Option<(Vec<usize>, usize)> {
    if start == goal {
        return Some((vec![start], 0));
    }

    // Forward search state (from start toward goal).
    let mut dist_f: HashMap<usize, usize> = HashMap::new();
    let mut pred_f: HashMap<usize, usize> = HashMap::new();
    let mut heap_f: BinaryHeap<HeapState> = BinaryHeap::new();

    // Backward search state (from goal toward start, following inbound edges).
    let mut dist_b: HashMap<usize, usize> = HashMap::new();
    let mut pred_b: HashMap<usize, usize> = HashMap::new();
    let mut heap_b: BinaryHeap<HeapState> = BinaryHeap::new();

    dist_f.insert(start, 0);
    heap_f.push(HeapState {
        f_cost: graph.heuristic(start, goal),
        g_cost: 0,
        position: start,
    });

    dist_b.insert(goal, 0);
    heap_b.push(HeapState {
        f_cost: graph.heuristic(goal, start),
        g_cost: 0,
        position: goal,
    });

    let mut best = usize::MAX;
    let mut meeting = None;

    loop {
        let top_f = heap_f.peek().map(|s| s.f_cost).unwrap_or(usize::MAX);
        let top_b = heap_b.peek().map(|s| s.f_cost).unwrap_or(usize::MAX);

        // Pohl's termination: stop when min priority of both heaps ≥ best.
        if top_f >= best && top_b >= best {
            break;
        }
        if heap_f.is_empty() && heap_b.is_empty() {
            break;
        }

        if top_f <= top_b {
            // Expand forward frontier.
            let Some(HeapState {
                g_cost, position, ..
            }) = heap_f.pop()
            else {
                break;
            };
            if g_cost > dist_f.get(&position).copied().unwrap_or(usize::MAX) {
                continue;
            }
            for edge in graph.outbound(position) {
                let next_g = g_cost + edge.cost;
                let improved = match dist_f.entry(edge.node) {
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
                    pred_f.insert(edge.node, position);
                    heap_f.push(HeapState {
                        f_cost: next_g + graph.heuristic(edge.node, goal),
                        g_cost: next_g,
                        position: edge.node,
                    });
                    if let Some(&back_g) = dist_b.get(&edge.node) {
                        let total = next_g.saturating_add(back_g);
                        if total < best {
                            best = total;
                            meeting = Some(edge.node);
                        }
                    }
                }
            }
        } else {
            // Expand backward frontier.
            let Some(HeapState {
                g_cost, position, ..
            }) = heap_b.pop()
            else {
                break;
            };
            if g_cost > dist_b.get(&position).copied().unwrap_or(usize::MAX) {
                continue;
            }
            for edge in graph.inbound(position) {
                let next_g = g_cost + edge.cost;
                let improved = match dist_b.entry(edge.node) {
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
                    pred_b.insert(edge.node, position);
                    heap_b.push(HeapState {
                        f_cost: next_g + graph.heuristic(edge.node, start),
                        g_cost: next_g,
                        position: edge.node,
                    });
                    if let Some(&fwd_g) = dist_f.get(&edge.node) {
                        let total = fwd_g.saturating_add(next_g);
                        if total < best {
                            best = total;
                            meeting = Some(edge.node);
                        }
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
