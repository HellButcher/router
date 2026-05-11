use std::io;

use router_algorithm::{Graph, Neighbour};
use router_storage::{
    data::{
        attrib::{HighwayClass, TurnFlags, WayFlags},
        edge_node::EdgeNode,
        turn_edge::TurnEdge,
        way::Way,
    },
    tablefile::Ref,
};
use router_types::coordinate::LatLon;

use crate::{
    Service,
    profile::{Profile, VehicleType},
    speed_config::SpeedConfig,
};

// ── CostModel trait ───────────────────────────────────────────────────────────

pub trait CostModel: Send + Sync {
    /// Traversal cost for given length of the edge represented by `en`. Returns `None` if blocked.
    fn traversal_cost(&self, dist_m: usize, en: &EdgeNode, way: &Way) -> Option<usize>;

    /// Turn cost at the shared intersection. Returns `None` if the turn is forbidden.
    fn turn_cost(&self, te: &TurnEdge) -> Option<usize>;

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize;
}

// ── Distance cost ─────────────────────────────────────────────────────────────

pub struct DistanceCost {
    pub vehicle_type: VehicleType,
}

impl CostModel for DistanceCost {
    fn traversal_cost(&self, dist_m: usize, _en: &EdgeNode, way: &Way) -> Option<usize> {
        if way.access.blocks(self.vehicle_type) {
            return None;
        }
        Some(dist_m)
    }

    fn turn_cost(&self, te: &TurnEdge) -> Option<usize> {
        if te.restriction_mask.blocks(self.vehicle_type) {
            return None;
        }
        Some(0)
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
    #[inline]
    fn effective_speed(&self, en: &EdgeNode, way: &Way) -> Option<u8> {
        let country_id = en.country_id;
        let default = self
            .speed_config
            .default_speed(country_id, self.profile.vehicle_type, way.highway)
            .unwrap_or_else(|| self.profile.default_speed(way.highway));
        if default == 0 {
            return None;
        }
        let tag_speed = way.max_speed;
        let speed =
            (if tag_speed > 0 { tag_speed } else { default }).min(self.profile.max_speed_kmh);
        let surface_pct = self.profile.surface_pct[way.surface_quality as usize];
        if surface_pct == 0 {
            return None;
        }
        Some(((speed as u32 * surface_pct as u32) / 100).max(1) as u8)
    }
}

impl CostModel for SpeedMap<'_> {
    fn traversal_cost(&self, dist_m: usize, en: &EdgeNode, way: &Way) -> Option<usize> {
        if way.access.blocks(self.profile.vehicle_type) {
            return None;
        }
        if way.dim.blocks_vehicle(
            self.profile.vehicle_dim.height_dm,
            self.profile.vehicle_dim.width_dm,
            self.profile.vehicle_dim.length_dm,
            self.profile.vehicle_dim.weight_250kg,
        ) {
            return None;
        }
        // TODO: toll and ferry penalties should scale with distance (not fixed; currenty applied to each edge))
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
        let speed = self.effective_speed(en, way)?;
        Some((dist_m as f32 * 3600.0 / speed as f32) as usize + toll_ms + ferry_ms)
    }

    fn turn_cost(&self, te: &TurnEdge) -> Option<usize> {
        if te.restriction_mask.blocks(self.profile.vehicle_type) {
            return None;
        }
        let flags = te.turn_flags;
        let mut cost: usize = 0;
        if flags.contains(TurnFlags::TRAFFIC_SIGNALS) {
            cost += self.profile.traffic_signal_penalty_ms as usize;
        }
        if flags.contains(TurnFlags::TOLL) {
            if self.avoid_toll {
                return None;
            }
            cost += self.profile.toll_penalty_ms as usize;
        }
        cost += turn_angle_penalty(te.turn_angle, self.profile.max_turn_penalty_ms);
        Some(cost)
    }

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize {
        (haversine_m(from, to) * 3600.0 / self.profile.max_speed_kmh as f32) as usize
    }
}

// ── RoadGraph ─────────────────────────────────────────────────────────────────

pub struct RoadGraph<'a, C: CostModel> {
    pub edge_nodes: Ref<'a, [EdgeNode]>,
    pub turn_edges: Ref<'a, [TurnEdge]>,
    pub ways: Ref<'a, [Way]>,
    pub geometry: Ref<'a, [LatLon]>,
    pub cost_model: C,
}

impl Service {
    pub fn road_graph<'a, C: CostModel>(
        &'a self,
        cost_model: C,
    ) -> Result<RoadGraph<'a, C>, io::Error> {
        Ok(RoadGraph {
            edge_nodes: self.edge_nodes.get_all()?,
            turn_edges: self.turn_edges.get_all()?,
            ways: self.ways.get_all()?,
            geometry: self.geometry.get_all()?,
            cost_model,
        })
    }
}

impl<C: CostModel> RoadGraph<'_, C> {
    #[inline]
    pub fn get_edge_from_pos(&self, edge_node_idx: usize) -> Option<LatLon> {
        self.edge_nodes
            .get(edge_node_idx)
            .and_then(|en| self.geometry.get(en.geometry_from_idx()).copied())
    }
    #[inline]
    pub fn get_edge_to_pos(&self, edge_node_idx: usize) -> Option<LatLon> {
        self.edge_nodes
            .get(edge_node_idx)
            .and_then(|en| self.geometry.get(en.geometry_to_idx()).copied())
    }
}

impl<C: CostModel> Graph for RoadGraph<'_, C> {
    type Iter<'a>
        = TurnIter<'a, C, false>
    where
        Self: 'a;

    type ReverseIter<'a>
        = TurnIter<'a, C, true>
    where
        Self: 'a;

    fn outbound(&self, node_idx: usize) -> Self::Iter<'_> {
        let first = self
            .edge_nodes
            .get(node_idx)
            .map(|en| en.first_outbound_turn_idx())
            .unwrap_or(usize::MAX);
        TurnIter {
            graph: self,
            current_turn: first,
        }
    }

    fn inbound(&self, node_idx: usize) -> Self::ReverseIter<'_> {
        let first = self
            .edge_nodes
            .get(node_idx)
            .map(|en| en.first_inbound_turn_idx())
            .unwrap_or(usize::MAX);
        TurnIter {
            graph: self,
            current_turn: first,
        }
    }

    fn heuristic(&self, from_idx: usize, to_idx: usize) -> Option<usize> {
        // Traversal of `from_idx` is already paid; use the edge's endpoint as position.
        // Traversal of `to_idx` is not yet paid, so use its start point.
        let from_pos = self.get_edge_to_pos(from_idx)?;
        let to_pos = self.get_edge_from_pos(to_idx)?;
        Some(self.cost_model.heuristic(from_pos, to_pos))
    }
}

// ── TurnIter ──────────────────────────────────────────────────────────────────

pub struct TurnIter<'a, C: CostModel, const REVERSE: bool> {
    graph: &'a RoadGraph<'a, C>,
    current_turn: usize,
}

impl<C: CostModel, const REVERSE: bool> Iterator for TurnIter<'_, C, REVERSE> {
    type Item = Neighbour;

    fn next(&mut self) -> Option<Neighbour> {
        loop {
            let te_idx = self.current_turn;
            if te_idx == usize::MAX {
                return None;
            }
            let te = self.graph.turn_edges.get(te_idx)?;

            self.current_turn = if REVERSE {
                te.next_inbound_idx()
            } else {
                te.next_outbound_idx()
            };

            let Some(turn) = self.graph.cost_model.turn_cost(te) else {
                continue;
            };

            let neighbour_en_idx = if REVERSE {
                te.from_edge_node_idx()
            } else {
                te.to_edge_node_idx()
            };

            let Some(neighbour_en) = self.graph.edge_nodes.get(neighbour_en_idx) else {
                continue;
            };
            let Some(to_way) = self.graph.ways.get(neighbour_en.way_idx as usize) else {
                continue;
            };
            let Some(traversal) = self.graph.cost_model.traversal_cost(
                neighbour_en.dist_m as usize,
                neighbour_en,
                to_way,
            ) else {
                continue;
            };

            return Some(Neighbour {
                edge_node_idx: neighbour_en_idx,
                cost: turn + traversal,
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────


/// Quadratic turn-angle penalty: 0 at ≤30°, scales to `max_ms` at 180°.
#[inline]
fn turn_angle_penalty(turn_angle: i16, max_ms: u32) -> usize {
    if max_ms == 0 {
        return 0;
    }
    let abs = turn_angle.unsigned_abs() as f32;
    let t = ((abs - 30.0).max(0.0) / 150.0).min(1.0);
    (t * t * max_ms as f32) as usize
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
