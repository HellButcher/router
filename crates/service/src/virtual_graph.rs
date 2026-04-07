use router_algorithm::{Edge, Graph};
use router_storage::data::attrib::WayFlags;
use router_types::coordinate::LatLon;

use crate::graph::{CostModel, RoadGraph, WayIter};
use crate::snap::{EdgeSnap, Snap};

// ── sentinel indices ──────────────────────────────────────────────────────────

pub const VIRTUAL_START: usize = usize::MAX;
pub const VIRTUAL_GOAL: usize = usize::MAX - 1;

// ── VirtualGraph ──────────────────────────────────────────────────────────────

/// Data for one virtual endpoint (start or goal) derived from an [`EdgeSnap`].
struct VirtualEndpoint {
    pos: LatLon,
    /// Nodes adjacent to the snapped point and the cost to reach them from
    /// (or to reach the virtual node from them).
    ///
    /// For the virtual start these are outbound edges: `(real_node_idx, cost)`.
    /// For the virtual goal these are the real nodes that have an edge *to* it.
    adjacent: Vec<(usize, usize)>,
}

/// Wraps a [`RoadGraph`] and injects up to two virtual nodes (`VIRTUAL_START`
/// / `VIRTUAL_GOAL`) for waypoints that were snapped to a point mid-edge.
///
/// When both endpoints are node snaps the virtual nodes are absent and this
/// behaves identically to the inner [`RoadGraph`].
pub struct VirtualGraph<'a, C: CostModel> {
    inner: RoadGraph<'a, C>,
    start: Option<VirtualEndpoint>,
    goal: Option<VirtualEndpoint>,
}

impl<'a, C: CostModel> VirtualGraph<'a, C> {
    /// Build a `VirtualGraph` from two snapped waypoints.
    ///
    /// Returns `(graph, effective_start_idx, effective_goal_idx)` — the
    /// effective indices are either a real node index (for node snaps) or
    /// `VIRTUAL_START` / `VIRTUAL_GOAL` (for edge snaps).
    pub fn new(
        inner: RoadGraph<'a, C>,
        start_snap: &Snap,
        goal_snap: &Snap,
    ) -> (Self, usize, usize) {
        let (vstart, start_idx) = match start_snap {
            Snap::Node { node_idx, .. } => (None, *node_idx),
            Snap::Edge(e) => (Some(virtual_start_endpoint(e, &inner)), VIRTUAL_START),
        };
        let (vgoal, goal_idx) = match goal_snap {
            Snap::Node { node_idx, .. } => (None, *node_idx),
            Snap::Edge(e) => (Some(virtual_goal_endpoint(e, &inner)), VIRTUAL_GOAL),
        };
        (
            Self {
                inner,
                start: vstart,
                goal: vgoal,
            },
            start_idx,
            goal_idx,
        )
    }
}

// ── Graph impl ────────────────────────────────────────────────────────────────

pub enum VirtualIter<'a, C: CostModel> {
    /// Fixed list of edges (virtual start/goal node).
    Fixed(std::vec::IntoIter<Edge>),
    /// Real node edges, with an optional extra edge injected to VIRTUAL_GOAL.
    Real {
        inner: WayIter<'a, C>,
        extra: Option<Edge>,
    },
}

impl<C: CostModel> Iterator for VirtualIter<'_, C> {
    type Item = Edge;

    fn next(&mut self) -> Option<Edge> {
        match self {
            Self::Fixed(it) => it.next(),
            Self::Real { inner, extra } => inner.next().or_else(|| extra.take()),
        }
    }
}

impl<C: CostModel> Graph for VirtualGraph<'_, C> {
    type Iter<'a>
        = VirtualIter<'a, C>
    where
        Self: 'a;

    fn outbound(&self, node_idx: usize) -> VirtualIter<'_, C> {
        if node_idx == VIRTUAL_START {
            return VirtualIter::Fixed(
                self.start
                    .as_ref()
                    .map(|v| {
                        v.adjacent
                            .iter()
                            .map(|&(n, c)| Edge { node: n, cost: c })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
                    .into_iter(),
            );
        }
        if node_idx == VIRTUAL_GOAL {
            return VirtualIter::Fixed(vec![].into_iter());
        }
        // Real node: delegate to inner, then inject edge to VIRTUAL_GOAL if
        // this node is adjacent to the goal's snapped edge.
        let extra = self.goal.as_ref().and_then(|g| {
            g.adjacent
                .iter()
                .find(|&&(n, _)| n == node_idx)
                .map(|&(_, cost)| Edge {
                    node: VIRTUAL_GOAL,
                    cost,
                })
        });
        VirtualIter::Real {
            inner: self.inner.outbound(node_idx),
            extra,
        }
    }

    fn inbound(&self, node_idx: usize) -> VirtualIter<'_, C> {
        if node_idx == VIRTUAL_GOAL {
            return VirtualIter::Fixed(
                self.goal
                    .as_ref()
                    .map(|v| {
                        v.adjacent
                            .iter()
                            .map(|&(n, c)| Edge { node: n, cost: c })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default()
                    .into_iter(),
            );
        }
        if node_idx == VIRTUAL_START {
            return VirtualIter::Fixed(vec![].into_iter());
        }
        let extra = self.start.as_ref().and_then(|s| {
            s.adjacent
                .iter()
                .find(|&&(n, _)| n == node_idx)
                .map(|&(_, cost)| Edge {
                    node: VIRTUAL_START,
                    cost,
                })
        });
        VirtualIter::Real {
            inner: self.inner.inbound(node_idx),
            extra,
        }
    }

    fn heuristic(&self, from_idx: usize, to_idx: usize) -> usize {
        let from_pos = match from_idx {
            VIRTUAL_START => self.start.as_ref().map(|v| v.pos),
            VIRTUAL_GOAL => return 0,
            _ => None,
        };
        match from_pos {
            Some(pos) => self.inner.cost_model.heuristic(pos, self.inner.goal_pos),
            None => self.inner.heuristic(from_idx, to_idx),
        }
    }
}

// ── endpoint construction ─────────────────────────────────────────────────────

fn virtual_start_endpoint<C: CostModel>(
    snap: &EdgeSnap,
    graph: &RoadGraph<'_, C>,
) -> VirtualEndpoint {
    let mut adjacent = Vec::with_capacity(2);
    if let (Ok(way), Ok(from), Ok(to)) = (
        graph.ways.get(snap.way_idx),
        graph.nodes.get(snap.from_node_idx),
        graph.nodes.get(snap.to_node_idx),
    ) {
        if let Some(full_cost) = graph.cost_model.edge_cost(&way, &from, &to) {
            // Forward: virtual_start → to_node (remaining fraction of edge).
            let cost_to = ((1.0 - snap.fraction) * full_cost as f32) as usize;
            adjacent.push((snap.to_node_idx, cost_to));

            // Backward: virtual_start → from_node (only on bidirectional ways).
            if !way.flags.contains(WayFlags::ONEWAY) {
                let cost_from = (snap.fraction * full_cost as f32) as usize;
                adjacent.push((snap.from_node_idx, cost_from));
            }
        }
    }
    VirtualEndpoint {
        pos: snap.pos,
        adjacent,
    }
}

fn virtual_goal_endpoint<C: CostModel>(
    snap: &EdgeSnap,
    graph: &RoadGraph<'_, C>,
) -> VirtualEndpoint {
    let mut adjacent = Vec::with_capacity(2);
    if let (Ok(way), Ok(from), Ok(to)) = (
        graph.ways.get(snap.way_idx),
        graph.nodes.get(snap.from_node_idx),
        graph.nodes.get(snap.to_node_idx),
    ) {
        if let Some(full_cost) = graph.cost_model.edge_cost(&way, &from, &to) {
            // from_node can reach virtual_goal by travelling forward to fraction t.
            let cost_from = (snap.fraction * full_cost as f32) as usize;
            adjacent.push((snap.from_node_idx, cost_from));

            // to_node can reach virtual_goal going backward (bidirectional only).
            if !way.flags.contains(WayFlags::ONEWAY) {
                let cost_to = ((1.0 - snap.fraction) * full_cost as f32) as usize;
                adjacent.push((snap.to_node_idx, cost_to));
            }
        }
    }
    VirtualEndpoint {
        pos: snap.pos,
        adjacent,
    }
}
