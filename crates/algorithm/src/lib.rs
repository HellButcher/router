pub mod a_star;
pub mod bidir_a_star;
pub mod bidir_dijkstra;
pub mod convex_hull;
pub mod dikstra;

/// A neighbouring node reachable from the current position, with the cost to traverse to it.
#[derive(Copy, Clone)]
pub struct Neighbour {
    pub edge_node_idx: usize,
    pub cost: usize,
}

pub trait Graph {
    type Iter<'a>: Iterator<Item = Neighbour>
    where
        Self: 'a;

    type ReverseIter<'a>: Iterator<Item = Neighbour>
    where
        Self: 'a;

    fn inbound(&self, node: usize) -> Self::ReverseIter<'_>;
    fn outbound(&self, node: usize) -> Self::Iter<'_>;

    /// Admissible heuristic: estimated cost from `from` to `to`.
    /// Must never overestimate the true cost (required for A* correctness).
    fn heuristic(&self, from: usize, to: usize) -> Option<usize>;
}

/// Blanket impl so algorithms can accept `&G` when `G: Graph`.
impl<G: Graph> Graph for &G {
    type Iter<'a>
        = G::Iter<'a>
    where
        Self: 'a;
    type ReverseIter<'a>
        = G::ReverseIter<'a>
    where
        Self: 'a;

    fn outbound(&self, node: usize) -> Self::Iter<'_> {
        (**self).outbound(node)
    }
    fn inbound(&self, node: usize) -> Self::ReverseIter<'_> {
        (**self).inbound(node)
    }
    fn heuristic(&self, from: usize, to: usize) -> Option<usize> {
        (**self).heuristic(from, to)
    }
}

/// Reconstruct a path from a predecessor map produced by a search algorithm.
///
/// Returns the node indices in order from `start` to `goal`.
pub fn reconstruct_path(
    predecessors: &std::collections::HashMap<usize, usize>,
    start: usize,
    goal: usize,
) -> Vec<usize> {
    let mut path = Vec::new();
    let mut current = goal;
    loop {
        path.push(current);
        if current == start {
            break;
        }
        match predecessors.get(&current) {
            Some(&prev) => current = prev,
            None => break, // disconnected — should not happen if goal was reached
        }
    }
    path.reverse();
    path
}

/// Reconstruct a path from predecessor maps produced by a search algorithm.
///
/// Returns the node indices in order from `start` to `goal`.
pub fn reconstruct_path_bidir(
    pred_forward: &std::collections::HashMap<usize, usize>,
    pred_backward: &std::collections::HashMap<usize, usize>,
    start: usize,
    meeting: usize,
    goal: usize,
) -> Vec<usize> {
    // Reconstruct forward half: start → meeting.
    let mut path = reconstruct_path(pred_forward, start, meeting);

    // Reconstruct backward half: meeting → goal.
    // pred_b[node] = the node that was expanded when `node` was discovered,
    // i.e. the successor in the forward direction.
    let mut cur = meeting;
    while let Some(&next) = pred_backward.get(&cur) {
        cur = next;
        path.push(cur);
        if cur == goal {
            break;
        }
    }

    path
}
