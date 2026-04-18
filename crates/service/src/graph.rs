use router_algorithm::{Edge, Graph};
use router_storage::{
    data::{
        attrib::{HighwayClass, NodeFlags, WayFlags},
        node::{NO_WAY, Node},
        way::Way,
        way_extended::WayExtended,
        way_index_from_ptr,
    },
    tablefile::TableFile,
};
use router_types::{coordinate::LatLon, country::CountryId};

use crate::{
    profile::{Profile, VehicleType},
    speed_config::SpeedConfig,
};

// ── CostModel trait ───────────────────────────────────────────────────────────

/// Computes the traversal cost of a way segment and provides a distance-based
/// heuristic for A*.
///
/// Implementations must ensure that `heuristic` never overestimates the true
/// edge cost so that A* remains correct and optimal.
pub trait CostModel: Send + Sync {
    /// Traversal cost for `way`. Returns `None` if the way is blocked for this vehicle.
    fn edge_cost(&self, way: &Way) -> Option<usize>;

    /// Additional cost for arriving at `node` (e.g. traffic signal delay, toll
    /// booth). Returns `None` if the node physically blocks the vehicle (barrier).
    /// Returns `Some(0)` if the node is freely passable with no extra penalty.
    fn node_cost(&self, node: &Node) -> Option<usize>;

    fn heuristic(&self, from: LatLon, to: LatLon) -> usize;
}

// ── Distance cost ─────────────────────────────────────────────────────────────

/// Costs edges by straight-line distance in metres, enforcing vehicle access restrictions.
pub struct DistanceCost {
    pub vehicle_type: VehicleType,
}

impl CostModel for DistanceCost {
    fn edge_cost(&self, way: &Way) -> Option<usize> {
        if way_is_blocked(way, self.vehicle_type) {
            return None;
        }
        Some(way.dist_m as usize)
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

/// Combines a routing profile with optional country-specific speed overrides.
///
/// Speed resolution order:
/// 1. Country+profile override from `speed_config` (if loaded and country known)
/// 2. Profile built-in default for the highway class
/// 3. Way's explicit `max_speed` tag (overrides the default)
/// 4. Surface quality penalty applied as a percentage multiplier
/// 5. Capped at the vehicle's physical maximum speed
#[derive(Copy, Clone)]
pub struct SpeedMap<'p> {
    pub profile: &'p Profile,
    pub speed_config: &'p SpeedConfig,
    /// Extended way attributes (dim restrictions, etc.).
    pub way_extended: &'p TableFile<WayExtended>,
    /// When `true`, any way or node with the TOLL flag is treated as impassable
    /// rather than adding `profile.toll_penalty_ms`.
    pub avoid_toll: bool,
    /// When `true`, ferry routes (`HighwayClass::Ferry`) are treated as impassable
    /// rather than adding `profile.ferry_penalty_ms`.
    pub avoid_ferry: bool,
}

impl SpeedMap<'_> {
    /// Default speed in km/h for the given way, before max_speed and surface adjustments.
    /// Returns 0 if the highway class is forbidden for this profile.
    #[inline]
    pub fn default_speed(&self, country_id: CountryId, highway: HighwayClass) -> u8 {
        self.speed_config
            .default_speed(country_id, self.profile.vehicle_type, highway)
            .unwrap_or_else(|| self.profile.default_speed(highway))
    }

    /// Effective speed in km/h for the given way after all adjustments.
    /// Returns `None` if the way is forbidden (speed 0 or impassable surface).
    #[inline]
    pub fn effective_speed(&self, way: &Way) -> Option<u8> {
        let default = self.default_speed(way.country_id, way.highway);
        if default == 0 {
            return None;
        }
        let speed = way
            .effective_max_speed(default)
            .min(self.profile.max_speed_kmh);
        let surface_pct = self.profile.surface_pct[way.surface_quality as usize];
        if surface_pct == 0 {
            return None;
        }
        Some(((speed as u32 * surface_pct as u32) / 100).max(1) as u8)
    }

    /// Travel-time cost in milliseconds for the given way, or `None` if blocked/impassable.
    #[inline]
    pub fn way_cost_ms(&self, way: &Way) -> Option<usize> {
        if way_is_blocked(way, self.profile.vehicle_type) {
            return None;
        }
        // Check dimension restrictions (only for ways that have extended attributes).
        if way.flags.contains(WayFlags::HAS_EXTENDED) {
            let dim = self.profile.vehicle_dim;
            if let Ok(Some((_, ext))) = self.way_extended.find(&way.id) {
                if ext
                    .dim
                    .blocks_vehicle(dim.height_dm, dim.width_dm, dim.weight_250kg)
                {
                    return None;
                }
            }
        }
        // Toll handling: hard block or soft penalty.
        let toll_ms = if way.flags.contains(WayFlags::TOLL) {
            if self.avoid_toll {
                return None;
            }
            self.profile.toll_penalty_ms as usize
        } else {
            0
        };
        // Ferry handling: hard block or boarding penalty.
        let ferry_ms = if way.highway == HighwayClass::Ferry {
            if self.avoid_ferry {
                return None;
            }
            self.profile.ferry_penalty_ms as usize
        } else {
            0
        };
        let speed = self.effective_speed(way)?;
        Some((way.dist_m as f32 * 3600.0 / speed as f32) as usize + toll_ms + ferry_ms)
    }
}

impl CostModel for SpeedMap<'_> {
    fn edge_cost(&self, way: &Way) -> Option<usize> {
        self.way_cost_ms(way)
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
        // Node-level toll booths: hard block or soft penalty.
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

// ── WayIter ───────────────────────────────────────────────────────────────────

pub struct WayIter<'a, C: CostModel> {
    graph: &'a RoadGraph<'a, C>,
    current_ptr: u64,
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

            self.current_ptr = if self.reverse {
                way.next_way_reverse()
            } else {
                way.next_way()
            };

            let way_from_idx = way.from_node_idx as usize;
            let way_to_idx = way.to_node_idx as usize;
            let neighbour_idx = if self.reverse {
                way_from_idx
            } else {
                way_to_idx
            };

            let Some(way_cost) = self.graph.cost_model.edge_cost(&way) else {
                continue;
            };
            let neighbour_ref = self.graph.nodes.get(neighbour_idx).ok();
            let node_penalty = match neighbour_ref.as_deref() {
                Some(n) => match self.graph.cost_model.node_cost(n) {
                    Some(p) => p,
                    None => continue, // node blocks the vehicle
                },
                None => 0,
            };
            return Some(Edge {
                node: neighbour_idx,
                cost: way_cost + node_penalty,
            });
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

pub(crate) fn way_is_blocked(way: &Way, vehicle: VehicleType) -> bool {
    match vehicle {
        VehicleType::Car => way.flags.contains(WayFlags::NO_MOTOR),
        VehicleType::Hgv => {
            way.flags.contains(WayFlags::NO_MOTOR) || way.flags.contains(WayFlags::NO_HGV)
        }
        VehicleType::Bicycle => way.flags.contains(WayFlags::NO_BICYCLE),
        VehicleType::Foot => way.flags.contains(WayFlags::NO_FOOT),
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
