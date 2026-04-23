use router_algorithm::{Graph, Neighbour};
use router_storage::{
    data::{
        attrib::{HighwayClass, NodeFlags, WayFlags},
        edge::{Edge, EdgeFlags},
        node::Node,
        way::Way,
    },
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;

use crate::{
    profile::{Profile, VehicleType},
    speed_config::SpeedConfig,
};

// ── CostModel trait ───────────────────────────────────────────────────────────

pub trait CostModel: Send + Sync {
    /// Traversal cost for `edge`. Returns `None` if the edge is blocked for this vehicle.
    fn edge_cost(&self, edge: &Edge, way: &Way) -> Option<usize>;

    /// Additional cost for arriving at `node`. Returns `None` if the node physically
    /// blocks the vehicle. Returns `Some(0)` if freely passable.
    fn node_cost(&self, node: &Node) -> Option<usize>;

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize;
}

// ── Distance cost ─────────────────────────────────────────────────────────────

pub struct DistanceCost {
    pub vehicle_type: VehicleType,
}

impl CostModel for DistanceCost {
    fn edge_cost(&self, edge: &Edge, _way: &Way) -> Option<usize> {
        if edge_is_blocked(edge, self.vehicle_type) {
            return None;
        }
        Some(edge.dist_m as usize)
    }

    fn node_cost(&self, node: &Node) -> Option<usize> {
        if node_is_blocked(node, self.vehicle_type) {
            None
        } else {
            Some(0)
        }
    }

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize {
        haversine_m(from, to) as usize
    }
}

// ── SpeedMap ──────────────────────────────────────────────────────────────────

#[derive(Copy, Clone)]
pub struct SpeedMap<'p> {
    pub profile: &'p Profile,
    pub speed_config: &'p SpeedConfig,
    pub avoid_toll: bool,
    pub avoid_ferry: bool,
}

impl SpeedMap<'_> {
    /// Effective speed in km/h for the given edge/way, or `None` if forbidden.
    #[inline]
    pub fn effective_speed(&self, edge: &Edge, way: &Way) -> Option<u8> {
        let country_id = edge.country_id;
        let default = self
            .speed_config
            .default_speed(country_id, self.profile.vehicle_type, way.highway)
            .unwrap_or_else(|| self.profile.default_speed(way.highway));
        if default == 0 {
            return None;
        }
        // Edge max_speed overrides Way max_speed when non-zero.
        let tag_speed = edge.max_speed;
        let speed =
            (if tag_speed > 0 { tag_speed } else { default }).min(self.profile.max_speed_kmh);
        let surface_pct = self.profile.surface_pct[way.surface_quality as usize];
        if surface_pct == 0 {
            return None;
        }
        Some(((speed as u32 * surface_pct as u32) / 100).max(1) as u8)
    }

    #[inline]
    pub fn edge_cost_ms(&self, edge: &Edge, way: &Way) -> Option<usize> {
        if edge_is_blocked(edge, self.profile.vehicle_type) {
            return None;
        }
        if way.dim.blocks_vehicle(
            self.profile.vehicle_dim.height_dm,
            self.profile.vehicle_dim.width_dm,
            self.profile.vehicle_dim.weight_250kg,
        ) {
            return None;
        }
        let toll_ms = if way.flags.contains(WayFlags::TOLL) {
            if self.avoid_toll {
                return None;
            }
            self.profile.toll_penalty_ms as usize
        } else {
            0
        };
        let ferry_ms = if way.highway == HighwayClass::Ferry {
            if self.avoid_ferry {
                return None;
            }
            self.profile.ferry_penalty_ms as usize
        } else {
            0
        };
        let speed = self.effective_speed(edge, way)?;
        Some((edge.dist_m as f32 * 3600.0 / speed as f32) as usize + toll_ms + ferry_ms)
    }
}

impl CostModel for SpeedMap<'_> {
    fn edge_cost(&self, edge: &Edge, way: &Way) -> Option<usize> {
        self.edge_cost_ms(edge, way)
    }

    fn node_cost(&self, node: &Node) -> Option<usize> {
        if node_is_blocked(node, self.profile.vehicle_type) {
            return None;
        }
        let mut penalty = if node.flags.contains(NodeFlags::TRAFFIC_SIGNALS) {
            self.profile.traffic_signal_penalty_ms as usize
        } else {
            0
        };
        if node.flags.contains(NodeFlags::TOLL) {
            if self.avoid_toll {
                return None;
            }
            penalty += self.profile.toll_penalty_ms as usize;
        }
        Some(penalty)
    }

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize {
        (haversine_m(from, to) * 3600.0 / self.profile.max_speed_kmh as f32) as usize
    }
}

// ── RoadGraph ─────────────────────────────────────────────────────────────────

pub struct RoadGraph<'a, C: CostModel> {
    pub nodes: &'a TableFile<Node>,
    pub edges: &'a TableFile<Edge>,
    pub ways: &'a TableFile<Way>,
    pub cost_model: C,
    pub goal_pos: LatLon,
}

impl<C: CostModel> Graph for RoadGraph<'_, C> {
    type Iter<'a>
        = EdgeIter<'a, C>
    where
        Self: 'a;

    fn outbound(&self, node_idx: usize) -> Self::Iter<'_> {
        let first_ptr = self
            .nodes
            .get(node_idx)
            .ok()
            .map(|n| n.first_edge_idx_outbound())
            .unwrap_or(usize::MAX);
        EdgeIter {
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
            .map(|n| n.first_edge_idx_inbound())
            .unwrap_or(usize::MAX);
        EdgeIter {
            graph: self,
            current_ptr: first_ptr,
            reverse: true,
        }
    }

    fn heuristic(&self, from_idx: usize, to_idx: usize) -> usize {
        let from_pos = self
            .nodes
            .get(from_idx)
            .map(|n| n.pos)
            .unwrap_or(self.goal_pos);
        let to_pos = self
            .nodes
            .get(to_idx)
            .map(|n| n.pos)
            .unwrap_or(self.goal_pos);
        self.cost_model.heuristic(from_pos, to_pos)
    }
}

// ── EdgeIter ──────────────────────────────────────────────────────────────────

pub struct EdgeIter<'a, C: CostModel> {
    graph: &'a RoadGraph<'a, C>,
    current_ptr: usize,
    reverse: bool,
}

impl<C: CostModel> Iterator for EdgeIter<'_, C> {
    type Item = Neighbour;

    fn next(&mut self) -> Option<Neighbour> {
        loop {
            let edge_idx = self.current_ptr;
            if edge_idx == usize::MAX {
                return None;
            }
            let edge = match self.graph.edges.get(edge_idx) {
                Ok(e) => e,
                Err(err) => {
                    tracing::warn!(edge_idx, error = %err, "edges.get failed");
                    return None;
                }
            };

            self.current_ptr = if self.reverse {
                edge.next_edge_reverse()
            } else {
                edge.next_edge()
            };

            let neighbour_idx = if self.reverse {
                edge.from_node_idx()
            } else {
                edge.to_node_idx()
            };

            let way = match self.graph.ways.get(edge.way_idx()) {
                Ok(w) => w,
                Err(err) => {
                    tracing::warn!(edge_idx, error = %err, "ways.get failed");
                    continue;
                }
            };

            let Some(edge_cost) = self.graph.cost_model.edge_cost(&edge, &way) else {
                continue;
            };

            let node_penalty = match self.graph.nodes.get(neighbour_idx).ok() {
                Some(n) => match self.graph.cost_model.node_cost(&n) {
                    Some(p) => p,
                    None => continue,
                },
                None => 0,
            };
            return Some(Neighbour {
                node: neighbour_idx,
                cost: edge_cost + node_penalty,
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn edge_is_blocked(edge: &Edge, vehicle: VehicleType) -> bool {
    match vehicle {
        VehicleType::Car => edge.flags.contains(EdgeFlags::NO_MOTOR),
        VehicleType::Hgv => {
            edge.flags.contains(EdgeFlags::NO_MOTOR) || edge.flags.contains(EdgeFlags::NO_HGV)
        }
        VehicleType::Bicycle => edge.flags.contains(EdgeFlags::NO_BICYCLE),
        VehicleType::Foot => edge.flags.contains(EdgeFlags::NO_FOOT),
    }
}

pub(crate) fn node_is_blocked(node: &Node, vehicle: VehicleType) -> bool {
    match vehicle {
        VehicleType::Car => node.flags.contains(NodeFlags::NO_MOTOR),
        VehicleType::Hgv => {
            node.flags.contains(NodeFlags::NO_MOTOR) || node.flags.contains(NodeFlags::NO_HGV)
        }
        VehicleType::Bicycle => node.flags.contains(NodeFlags::NO_BICYCLE),
        VehicleType::Foot => node.flags.contains(NodeFlags::NO_FOOT),
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
    2.0 * R * s.clamp(0.0, 1.0).sqrt().asin()
}
