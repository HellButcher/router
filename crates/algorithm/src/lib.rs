pub mod a_star;
pub mod bidir_a_star;
pub mod bidir_dijkstra;
pub mod dikstra;

#[derive(Copy, Clone)]
pub struct Edge {
    pub node: usize,
    pub cost: usize,
}

pub trait Graph {
    type Iter<'a>: Iterator<Item = Edge>
    where
        Self: 'a;

    fn inbound(&self, node: usize) -> Self::Iter<'_>;
    fn outbound(&self, node: usize) -> Self::Iter<'_>;

    /// Admissible heuristic: estimated cost from `from` to `to`.
    /// Must never overestimate the true cost (required for A* correctness).
    fn heuristic(&self, from: usize, to: usize) -> usize;
}

/// Blanket impl so algorithms can accept `&G` when `G: Graph`.
impl<G: Graph> Graph for &G {
    type Iter<'a>
        = G::Iter<'a>
    where
        Self: 'a;

    fn outbound(&self, node: usize) -> Self::Iter<'_> {
        (**self).outbound(node)
    }
    fn inbound(&self, node: usize) -> Self::Iter<'_> {
        (**self).inbound(node)
    }
    fn heuristic(&self, from: usize, to: usize) -> usize {
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
