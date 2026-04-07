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
    /// Edges to/from adjacent real nodes.
    /// For the start these are outbound edges; for the goal they are inbound.
    adjacent: Vec<Edge>,
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
        let (mut vstart, start_idx) = match start_snap {
            Snap::Node { node_idx, .. } => (None, *node_idx),
            Snap::Edge(e) => (Some(virtual_start_endpoint(e, &inner)), VIRTUAL_START),
        };
        let (mut vgoal, goal_idx) = match goal_snap {
            Snap::Node { node_idx, .. } => (None, *node_idx),
            Snap::Edge(e) => (Some(virtual_goal_endpoint(e, &inner)), VIRTUAL_GOAL),
        };

        // When both snaps lie on the same way, inject a direct edge between
        // the two virtual nodes so the search doesn't have to detour through
        // real nodes (which would give the wrong cost and may not even exist
        // between the two snap points).
        if let (Snap::Edge(s), Snap::Edge(g), Some(vs), Some(vg)) =
            (start_snap, goal_snap, vstart.as_mut(), vgoal.as_mut())
        {
            if s.way_idx == g.way_idx {
                if let (Ok(way), Ok(from), Ok(to)) = (
                    inner.ways.get(s.way_idx),
                    inner.nodes.get(s.from_node_idx),
                    inner.nodes.get(s.to_node_idx),
                ) {
                    if let Some(full_cost) = inner.cost_model.edge_cost(&way, &from, &to) {
                        let frac_diff = g.fraction - s.fraction;
                        if frac_diff >= 0.0 {
                            // Goal is ahead of start along the way's direction.
                            let cost = (frac_diff * full_cost as f32) as usize;
                            vs.adjacent.push(Edge {
                                node: VIRTUAL_GOAL,
                                cost,
                            });
                            vg.adjacent.push(Edge {
                                node: VIRTUAL_START,
                                cost,
                            });
                        } else if !way.flags.contains(WayFlags::ONEWAY) {
                            // Goal is behind start; only valid on bidirectional ways.
                            let cost = ((-frac_diff) * full_cost as f32) as usize;
                            vs.adjacent.push(Edge {
                                node: VIRTUAL_GOAL,
                                cost,
                            });
                            vg.adjacent.push(Edge {
                                node: VIRTUAL_START,
                                cost,
                            });
                        }
                    }
                }
            }
        }

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

    /// Resolve a node index (real or virtual sentinel) to a map position.
    fn resolve_pos(&self, idx: usize) -> Option<LatLon> {
        match idx {
            VIRTUAL_START => self.start.as_ref().map(|v| v.pos),
            VIRTUAL_GOAL => self.goal.as_ref().map(|v| v.pos),
            _ => self.inner.nodes.get(idx).ok().map(|n| n.pos),
        }
    }
}

// ── Graph impl ────────────────────────────────────────────────────────────────

pub enum VirtualIter<'a, C: CostModel> {
    /// Slice of pre-built edges from a virtual endpoint (no allocation).
    Fixed(std::slice::Iter<'a, Edge>),
    /// Real node edges, with an optional extra edge injected to a virtual node.
    Real {
        inner: WayIter<'a, C>,
        extra: Option<Edge>,
    },
}

impl<C: CostModel> Iterator for VirtualIter<'_, C> {
    type Item = Edge;

    fn next(&mut self) -> Option<Edge> {
        match self {
            Self::Fixed(it) => it.next().copied(),
            Self::Real { inner, extra } => inner.next().or_else(|| extra.take()),
        }
    }
}

static EMPTY: &[Edge] = &[];

impl<C: CostModel> Graph for VirtualGraph<'_, C> {
    type Iter<'a>
        = VirtualIter<'a, C>
    where
        Self: 'a;

    fn outbound(&self, node_idx: usize) -> VirtualIter<'_, C> {
        if node_idx == VIRTUAL_START {
            return VirtualIter::Fixed(self.start.as_ref().map_or(EMPTY, |v| &v.adjacent).iter());
        }
        if node_idx == VIRTUAL_GOAL {
            return VirtualIter::Fixed(EMPTY.iter());
        }
        let extra = self.goal.as_ref().and_then(|g| {
            g.adjacent
                .iter()
                .find(|e| e.node == node_idx)
                .map(|e| Edge {
                    node: VIRTUAL_GOAL,
                    cost: e.cost,
                })
        });
        VirtualIter::Real {
            inner: self.inner.outbound(node_idx),
            extra,
        }
    }

    fn inbound(&self, node_idx: usize) -> VirtualIter<'_, C> {
        if node_idx == VIRTUAL_GOAL {
            return VirtualIter::Fixed(self.goal.as_ref().map_or(EMPTY, |v| &v.adjacent).iter());
        }
        if node_idx == VIRTUAL_START {
            return VirtualIter::Fixed(EMPTY.iter());
        }
        let extra = self.start.as_ref().and_then(|s| {
            s.adjacent
                .iter()
                .find(|e| e.node == node_idx)
                .map(|e| Edge {
                    node: VIRTUAL_START,
                    cost: e.cost,
                })
        });
        VirtualIter::Real {
            inner: self.inner.inbound(node_idx),
            extra,
        }
    }

    fn heuristic(&self, from_idx: usize, to_idx: usize) -> usize {
        let Some(from_pos) = self.resolve_pos(from_idx) else {
            return 0;
        };
        let Some(to_pos) = self.resolve_pos(to_idx) else {
            return 0;
        };
        self.inner.cost_model.heuristic(from_pos, to_pos)
    }
}

// ── endpoint construction ─────────────────────────────────────────────────────

fn virtual_start_endpoint<C: CostModel>(
    snap: &EdgeSnap,
    graph: &RoadGraph<'_, C>,
) -> VirtualEndpoint {
    let mut adjacent = Vec::with_capacity(2);
    let (Ok(way), Ok(from), Ok(to)) = (
        graph.ways.get(snap.way_idx),
        graph.nodes.get(snap.from_node_idx),
        graph.nodes.get(snap.to_node_idx),
    ) else {
        return VirtualEndpoint {
            pos: snap.pos,
            adjacent,
        };
    };
    if let Some(full_cost) = graph.cost_model.edge_cost(&way, &from, &to) {
        let cost_to_snap = (snap.fraction * full_cost as f32) as usize;
        let cost_from_snap = ((1.0 - snap.fraction) * full_cost as f32) as usize;

        adjacent.push(Edge {
            node: snap.to_node_idx,
            cost: cost_from_snap,
        });
        if !way.flags.contains(WayFlags::ONEWAY) {
            adjacent.push(Edge {
                node: snap.from_node_idx,
                cost: cost_to_snap,
            });
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
    let (Ok(way), Ok(from), Ok(to)) = (
        graph.ways.get(snap.way_idx),
        graph.nodes.get(snap.from_node_idx),
        graph.nodes.get(snap.to_node_idx),
    ) else {
        return VirtualEndpoint {
            pos: snap.pos,
            adjacent,
        };
    };
    if let Some(full_cost) = graph.cost_model.edge_cost(&way, &from, &to) {
        let cost_to_snap = (snap.fraction * full_cost as f32) as usize;
        let cost_from_snap = ((1.0 - snap.fraction) * full_cost as f32) as usize;

        adjacent.push(Edge {
            node: snap.from_node_idx,
            cost: cost_to_snap,
        });
        if !way.flags.contains(WayFlags::ONEWAY) {
            adjacent.push(Edge {
                node: snap.to_node_idx,
                cost: cost_from_snap,
            });
        }
    }
    VirtualEndpoint {
        pos: snap.pos,
        adjacent,
    }
}
