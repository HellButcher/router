use std::ops::Deref;

use router_algorithm::{Graph, Neighbour};
use router_types::coordinate::LatLon;

use crate::graph::{CostModel, RoadGraph, TurnIter};
use crate::snap::Snap;

/// fixed index of the virtual start edge-node
///
/// The virtual start is a virtual edge-node with only outbound edges to the real graph, representing the snapped start position.
/// It also only has a `to` position (the snap point), and no `from` position.
pub const VIRTUAL_START: usize = usize::MAX;

/// fixed index of the virtual goal edge-node
///
/// The virtual goal is a virtual edge-node with only inbound edges from the real graph, representing the snapped goal position.
/// It also only has a `from` position (the snap point), and no `to` position.
pub const VIRTUAL_GOAL: usize = usize::MAX - 1;

// ── VirtualEndpoint ───────────────────────────────────────────────────────────

struct VirtualEndpoint {
    pos: LatLon,
    /// neighbours with partial-traversal cost baked in.
    /// For VIRTUAL_START, these are outbound neighbours.
    /// For VIRTUAL_GOAL, these are inbound neighbours.
    adjacent: Vec<Neighbour>,
}

// ── VirtualGraph ──────────────────────────────────────────────────────────────

pub struct VirtualGraph<'a, C: CostModel> {
    inner: RoadGraph<'a, C>,
    start: Option<VirtualEndpoint>,
    goal: Option<VirtualEndpoint>,
}

impl<'a, C: CostModel> VirtualGraph<'a, C> {
    pub fn new_from_start(inner: RoadGraph<'a, C>, start_snaps: &[Snap]) -> (Self, usize) {
        let (start, start_idx) = build_start(&inner, start_snaps);
        (
            Self {
                inner,
                start,
                goal: None,
            },
            start_idx,
        )
    }

    pub fn new(
        inner: RoadGraph<'a, C>,
        start_snaps: &[Snap],
        goal_snaps: &[Snap],
    ) -> (Self, usize, usize) {
        let (mut start, start_idx) = build_start(&inner, start_snaps);
        let (mut goal, goal_idx) = build_goal(&inner, goal_snaps);

        // Direct VIRTUAL_START → VIRTUAL_GOAL edge when both snaps are on the
        // same directed EdgeNode and the start is before the goal.
        if let (Some(vs), Some(vg)) = (start.as_mut(), goal.as_mut())
            && let Some((start_snap, goal_snap)) = find_shared_snap(start_snaps, goal_snaps)
            && let Some(en) = inner.edge_nodes.get(start_snap.edge_node_idx)
            && let Some(way) = inner.ways.get(en.way_idx as usize)
            && let Some(cost) = inner.cost_model.traversal_cost(
                goal_snap.distance_from_start_m - start_snap.distance_from_start_m,
                en,
                way,
            )
        {
            vs.adjacent.push(Neighbour {
                edge_node_idx: VIRTUAL_GOAL,
                cost,
            });
            vg.adjacent.push(Neighbour {
                edge_node_idx: VIRTUAL_START,
                cost,
            });
        }

        (Self { inner, start, goal }, start_idx, goal_idx)
    }

    pub fn get_edge_from_pos(&self, edge_node_idx: usize) -> Option<LatLon> {
        if edge_node_idx == VIRTUAL_GOAL
            && let Some(end) = &self.goal
        {
            return Some(end.pos);
        }
        self.inner.get_edge_from_pos(edge_node_idx)
    }

    pub fn get_edge_to_pos(&self, edge_node_idx: usize) -> Option<LatLon> {
        if edge_node_idx == VIRTUAL_START
            && let Some(start) = &self.start
        {
            return Some(start.pos);
        }
        self.inner.get_edge_to_pos(edge_node_idx)
    }
}

// ── Graph impl ────────────────────────────────────────────────────────────────

pub enum VirtualIter<'a, C: CostModel, const REVERSE: bool> {
    Fixed(std::slice::Iter<'a, Neighbour>),
    Real {
        inner: TurnIter<'a, C, REVERSE>,
        extra: Option<Neighbour>,
    },
}

static EMPTY: &[Neighbour] = &[];

impl<C: CostModel, const REVERSE: bool> Iterator for VirtualIter<'_, C, REVERSE> {
    type Item = Neighbour;

    fn next(&mut self) -> Option<Neighbour> {
        match self {
            Self::Fixed(it) => it.next().copied(),
            Self::Real { inner, extra } => inner.next().or_else(|| extra.take()),
        }
    }
}

impl<C: CostModel> Graph for VirtualGraph<'_, C> {
    type Iter<'a>
        = VirtualIter<'a, C, false>
    where
        Self: 'a;
    type ReverseIter<'a>
        = VirtualIter<'a, C, true>
    where
        Self: 'a;

    fn outbound(&self, node_idx: usize) -> VirtualIter<'_, C, false> {
        match node_idx {
            VIRTUAL_START => VirtualIter::Fixed(
                self.start
                    .as_ref()
                    .map_or(EMPTY, |v| v.adjacent.as_slice())
                    .iter(),
            ),
            VIRTUAL_GOAL => VirtualIter::Fixed(EMPTY.iter()),
            _ => {
                let extra = self.goal.as_ref().and_then(|g| {
                    g.adjacent
                        .binary_search_by_key(&node_idx, |e| e.edge_node_idx)
                        .ok()
                        .and_then(|i| g.adjacent.get(i))
                        .map(|e| Neighbour {
                            edge_node_idx: VIRTUAL_GOAL,
                            cost: e.cost,
                        })
                });
                VirtualIter::Real {
                    inner: self.inner.outbound(node_idx),
                    extra,
                }
            }
        }
    }

    fn inbound(&self, node_idx: usize) -> VirtualIter<'_, C, true> {
        match node_idx {
            VIRTUAL_GOAL => VirtualIter::Fixed(
                self.goal
                    .as_ref()
                    .map_or(EMPTY, |v| v.adjacent.as_slice())
                    .iter(),
            ),
            VIRTUAL_START => VirtualIter::Fixed(EMPTY.iter()),
            _ => {
                let extra = self.start.as_ref().and_then(|s| {
                    s.adjacent
                        .binary_search_by_key(&node_idx, |e| e.edge_node_idx)
                        .ok()
                        .and_then(|i| s.adjacent.get(i))
                        .map(|e| Neighbour {
                            edge_node_idx: VIRTUAL_START,
                            cost: e.cost,
                        })
                });
                VirtualIter::Real {
                    inner: self.inner.inbound(node_idx),
                    extra,
                }
            }
        }
    }

    fn heuristic(&self, from_idx: usize, to_idx: usize) -> Option<usize> {
        let from_pos = self.get_edge_to_pos(from_idx)?;
        let to_pos = self.get_edge_from_pos(to_idx)?;
        Some(self.inner.cost_model.heuristic(from_pos, to_pos))
    }
}

impl<'a, C: CostModel> Deref for VirtualGraph<'a, C> {
    type Target = RoadGraph<'a, C>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

// ── Endpoint builders ─────────────────────────────────────────────────────────

/// Build outbound adjacency for VIRTUAL_START.
///
/// For each EdgeNode direction at the snap point (forward + optional reverse),
/// the remaining traversal cost is baked into every outbound TurnEdge's cost.
fn build_start<C: CostModel>(
    graph: &RoadGraph<'_, C>,
    snaps: &[Snap],
) -> (Option<VirtualEndpoint>, usize) {
    if snaps.is_empty() {
        return (None, VIRTUAL_START);
    }
    let mut adjacent = Vec::new();
    let pos = snaps.first().unwrap().pos;
    for snap in snaps {
        if snap.distance_to_end_m == 0 {
            // Snap is exactly at the end of an edge → no virtual edge needed
            return (None, snap.edge_node_idx);
        }
        if pos.is_quasi_equal(snap.pos) {
            collect_turns_for_snap(graph, snap, snap.distance_to_end_m, &mut adjacent, |en| {
                graph.outbound(en)
            });
        }
    }
    adjacent.sort_by_key(|e| e.edge_node_idx);

    (Some(VirtualEndpoint { pos, adjacent }), VIRTUAL_START)
}

/// Build inbound adjacency for VIRTUAL_GOAL.
///
/// For each EdgeNode direction at the snap point (forward + optional reverse),
/// the partial traversal cost is baked into every inbound TurnEdge's cost.
fn build_goal<C: CostModel>(
    graph: &RoadGraph<'_, C>,
    snaps: &[Snap],
) -> (Option<VirtualEndpoint>, usize) {
    if snaps.is_empty() {
        return (None, VIRTUAL_GOAL);
    }
    let mut adjacent = Vec::new();
    let pos = snaps.first().unwrap().pos;
    for snap in snaps {
        if snap.distance_from_start_m == 0 {
            // Snap is exactly at the start of an edge → no virtual edge needed
            return (None, snap.edge_node_idx);
        }
        if pos.is_quasi_equal(snap.pos) {
            collect_turns_for_snap(
                graph,
                snap,
                snap.distance_from_start_m,
                &mut adjacent,
                |en| graph.inbound(en),
            );
        }
    }
    adjacent.sort_by_key(|e| e.edge_node_idx);

    (Some(VirtualEndpoint { pos, adjacent }), VIRTUAL_GOAL)
}

fn collect_turns_for_snap<C: CostModel, F: Fn(usize) -> I, I: Iterator<Item = Neighbour>>(
    graph: &RoadGraph<'_, C>,
    snap: &Snap,
    dist_m: usize,
    out: &mut Vec<Neighbour>,
    get_turns: F,
) {
    let Some(en) = graph.edge_nodes.get(snap.edge_node_idx) else {
        return;
    };
    let Some(way) = graph.ways.get(en.way_idx as usize) else {
        return;
    };
    let Some(remaining_cost) = graph.cost_model.traversal_cost(dist_m, en, way) else {
        return;
    };

    for mut neighbor in get_turns(snap.edge_node_idx) {
        neighbor.cost += remaining_cost;
        out.push(neighbor);
    }
}

fn find_shared_snap<'a, 'b>(
    start_snaps: &'a [Snap],
    goal_snaps: &'b [Snap],
) -> Option<(&'a Snap, &'b Snap)> {
    if start_snaps.is_empty() || goal_snaps.is_empty() {
        return None;
    }
    let first_start_pos = start_snaps[0].pos;
    let first_goal_pos = goal_snaps[0].pos;
    for start_snap in start_snaps {
        if !start_snap.pos.is_quasi_equal(first_start_pos) {
            continue;
        }
        for goal_snap in goal_snaps {
            if !goal_snap.pos.is_quasi_equal(first_goal_pos) {
                continue;
            }
            if start_snap.edge_node_idx == goal_snap.edge_node_idx
                && start_snap.distance_from_start_m <= goal_snap.distance_from_start_m
            {
                return Some((start_snap, goal_snap));
            }
        }
    }
    None
}
