use router_algorithm::{Edge, Graph};
use router_storage::{
    data::{
        attrib::WayFlags,
        node::{NO_WAY, Node},
        way::Way,
        way_index_from_ptr,
    },
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;

use crate::profile::{Profile, VehicleType};

// ── CostModel trait ───────────────────────────────────────────────────────────

/// Computes the traversal cost of a way segment and provides a distance-based
/// heuristic for A*.
///
/// Implementations must ensure that `heuristic` never overestimates the true
/// edge cost so that A* remains correct and optimal.
pub trait CostModel: Send + Sync {
    /// Returns the cost to traverse `way` from `from` to `to`, or `None` if
    /// the way is inaccessible under this model.
    fn edge_cost(&self, way: &Way, from: &Node, to: &Node) -> Option<usize>;

    /// Admissible lower-bound estimate of the cost from `from` to `to`.
    fn heuristic(&self, from: LatLon, to: LatLon) -> usize;
}

// ── Distance cost ─────────────────────────────────────────────────────────────

/// Costs edges by straight-line distance in metres (ignores speeds).
pub struct DistanceCost;

impl CostModel for DistanceCost {
    fn edge_cost(&self, _way: &Way, from: &Node, to: &Node) -> Option<usize> {
        Some(haversine_m(from.pos, to.pos) as usize)
    }

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize {
        haversine_m(from, to) as usize
    }
}

// ── Travel-time cost ──────────────────────────────────────────────────────────

/// Costs edges by estimated travel time in milliseconds, using the effective
/// maximum speed capped at the vehicle's own maximum speed.
pub struct TravelTimeCost<'p> {
    pub profile: &'p Profile,
}

impl CostModel for TravelTimeCost<'_> {
    fn edge_cost(&self, way: &Way, from: &Node, to: &Node) -> Option<usize> {
        // Access check
        if way_is_blocked(way, self.profile.vehicle_type) {
            return None;
        }
        let default_speed = self.profile.default_speed(way.highway);
        if default_speed == 0 {
            // Highway class not permitted for this vehicle type.
            return None;
        }
        let speed = way
            .effective_max_speed(default_speed)
            .min(self.profile.max_speed_kmh);
        let dist_m = haversine_m(from.pos, to.pos);
        // cost in milliseconds: (dist_m / speed_m_per_s) * 1000
        // speed_m_per_s = speed_kmh * 1000 / 3600
        // → cost_ms = dist_m * 3600 / speed_kmh
        let cost_ms = (dist_m * 3600.0 / speed as f32) as usize;
        Some(cost_ms)
    }

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize {
        // Lower bound: travel at vehicle's maximum speed over straight-line dist.
        let dist_m = haversine_m(from, to);
        (dist_m * 3600.0 / self.profile.max_speed_kmh as f32) as usize
    }
}

// ── RoadGraph ─────────────────────────────────────────────────────────────────

/// A [`Graph`] implementation backed by mmap'd node and way tables.
pub struct RoadGraph<'a, C: CostModel> {
    pub nodes: &'a TableFile<Node>,
    pub ways: &'a TableFile<Way>,
    pub cost_model: C,
    pub goal_pos: LatLon,
}

impl<C: CostModel> Graph for RoadGraph<'_, C> {
    type Iter<'a>
        = WayIter<'a, C>
    where
        Self: 'a;

    fn outbound(&self, node_idx: usize) -> Self::Iter<'_> {
        let first_ptr = self
            .nodes
            .get(node_idx)
            .ok()
            .map(|n| n.first_way())
            .unwrap_or(NO_WAY);
        WayIter {
            graph: self,
            current_ptr: first_ptr,
            reverse: false,
        }
    }

    fn inbound(&self, node_idx: usize) -> Self::Iter<'_> {
        let first_ptr = self
            .nodes
            .get(node_idx)
            .ok()
            .map(|n| n.first_way_reverse())
            .unwrap_or(NO_WAY);
        WayIter {
            graph: self,
            current_ptr: first_ptr,
            reverse: true,
        }
    }

    fn heuristic(&self, from_idx: usize, _to_idx: usize) -> usize {
        let from_pos = self
            .nodes
            .get(from_idx)
            .map(|n| n.pos)
            .unwrap_or(self.goal_pos);
        self.cost_model.heuristic(from_pos, self.goal_pos)
    }
}

// ── WayIter ───────────────────────────────────────────────────────────────────

pub struct WayIter<'a, C: CostModel> {
    graph: &'a RoadGraph<'a, C>,
    current_ptr: u64,
    /// `false` = following `next_way` (outbound), `true` = `next_way_reverse` (inbound).
    reverse: bool,
}

impl<C: CostModel> Iterator for WayIter<'_, C> {
    type Item = Edge;

    fn next(&mut self) -> Option<Edge> {
        loop {
            let way_idx = way_index_from_ptr(self.current_ptr)?;
            let way = match self.graph.ways.get(way_idx) {
                Ok(w) => w,
                Err(e) => {
                    tracing::warn!(way_idx, error = %e, "ways.get failed");
                    return None;
                }
            };

            // Advance pointer before any early-continues.
            self.current_ptr = if self.reverse {
                way.next_way_reverse()
            } else {
                way.next_way()
            };

            // All stored ways are forward-directed; inbound traversal of a
            // oneway is not permitted.
            if self.reverse && way.flags.contains(WayFlags::ONEWAY) {
                continue;
            }

            // After node-index resolution from_node_idx / to_node_idx are direct
            // table indices — no binary search needed.
            let (from_idx, neighbour_idx) = if self.reverse {
                (way.to_node_idx as usize, way.from_node_idx as usize)
            } else {
                (way.from_node_idx as usize, way.to_node_idx as usize)
            };

            let from = match self.graph.nodes.get(from_idx) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(from_idx, error = %e, "nodes.get failed");
                    continue;
                }
            };
            let to = match self.graph.nodes.get(neighbour_idx) {
                Ok(n) => n,
                Err(e) => {
                    tracing::warn!(neighbour_idx, error = %e, "nodes.get failed");
                    continue;
                }
            };

            if let Some(cost) = self.graph.cost_model.edge_cost(&way, &from, &to) {
                return Some(Edge {
                    node: neighbour_idx,
                    cost,
                });
            }
        }
    }
}

// ── Geometry helpers ──────────────────────────────────────────────────────────

/// Returns whether a way is blocked for the given vehicle type based on its flags.
fn way_is_blocked(way: &Way, vehicle: VehicleType) -> bool {
    match vehicle {
        VehicleType::Car => way.flags.contains(WayFlags::NO_MOTOR),
        VehicleType::Hgv => {
            way.flags.contains(WayFlags::NO_MOTOR) || way.flags.contains(WayFlags::NO_HGV)
        }
    }
}

/// Haversine distance in metres between two `LatLon` positions.
pub fn haversine_m(a: LatLon, b: LatLon) -> f32 {
    const R: f32 = 6_371_000.0;
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();
    let dlat = (b.lat - a.lat).to_radians();
    let dlon = (b.lon - a.lon).to_radians();
    let s = (dlat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (dlon / 2.0).sin().powi(2);
    2.0 * R * s.sqrt().asin()
}
