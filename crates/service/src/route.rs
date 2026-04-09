use std::num::NonZeroU64;
use std::time::Duration;

use router_algorithm::{
    a_star::a_star, bidir_a_star::bidir_a_star, bidir_dijkstra::bidir_dijkstra, dikstra::dikstra,
};
use router_types::bbox::BoundingBox;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    graph::{RoadGraph, SpeedMap, haversine_m},
    locate::SnapMode,
    snap::{EdgeSnapper, Snap},
    virtual_graph::{VIRTUAL_GOAL, VIRTUAL_START, VirtualGraph},
};

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};

/// Search algorithm used to compute the route.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// Dijkstra's algorithm (unidirectional).
    Dijkstra,
    /// Bidirectional Dijkstra.
    BidirDijkstra,
    /// A* (unidirectional).
    AStar,
    /// Bidirectional A*.
    #[default]
    BidirAStar,
}

impl Algorithm {
    pub fn run(
        self,
        graph: impl router_algorithm::Graph,
        start: usize,
        goal: usize,
    ) -> Option<(Vec<usize>, usize)> {
        match self {
            Algorithm::Dijkstra => dikstra(graph, start, goal),
            Algorithm::BidirDijkstra => bidir_dijkstra(graph, start, goal),
            Algorithm::AStar => a_star(graph, start, goal),
            Algorithm::BidirAStar => bidir_a_star(graph, start, goal),
        }
    }
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RouteRequest {
    #[cfg_attr(feature = "serde", serde(default))]
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,

    pub locations: Locations,

    /// Whether to snap waypoints to the nearest node or the nearest point on a
    /// way segment. Defaults to [`SnapMode::Edge`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub snap_mode: SnapMode,

    /// Search algorithm used to find the shortest path. Defaults to [`Algorithm::AStar`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub algorithm: Algorithm,

    /// When `true`, routes avoid all toll roads and toll booths entirely.
    #[cfg_attr(feature = "serde", serde(default))]
    pub avoid_toll: bool,

    /// When `true`, routes avoid ferry connections entirely.
    #[cfg_attr(feature = "serde", serde(default))]
    pub avoid_ferry: bool,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RouteResponse {
    pub profile: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,

    pub locations: Vec<Location>,

    #[cfg_attr(feature = "serde", serde(flatten))]
    pub trip_summary: Summary,

    pub legs: Vec<Leg>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Summary {
    pub duration: Duration,
    pub length: u32,
    pub bounds: BoundingBox,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Leg {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub leg_summary: Summary,

    pub path: Points,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub maneuvers: Vec<Maneuver>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Maneuver {
    #[cfg_attr(feature = "serde", serde(rename = "maneuver"))]
    pub maneuver_type: ManeuverType,
    pub maneuver_direction: Option<ManeuverDirection>,
    pub instruction: String,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub street_names: Vec<String>,
    pub path_segment: [usize; 2],
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ManeuverType {
    Start,
    Destination,
    Continue,
    Turn,
    SlightTurn,
    SharpTurn,
    UTurn,
    Ramp,
    Exit,
    Stay,
    Merge,
    RoundaboutEnter,
    RoundaboutExit(u8),
    FerryEnter,
    FerryExit,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ManeuverDirection {
    Straight,
    Left,
    Right,
}

impl Service {
    pub async fn calculate_route(&self, request: RouteRequest) -> Result<RouteResponse> {
        let profile = self.get_opt_profile(request.profile.as_deref())?;
        let units = request.units;
        let snap_mode = request.snap_mode;
        let locations: Vec<Location> = request.locations.try_into()?;

        if locations.len() < 2 {
            return Err(Error::InvalidRequest(
                "at least two locations are required".into(),
            ));
        }

        let snapper = EdgeSnapper {
            nodes: &self.nodes,
            ways: &self.ways,
            edge_spatial: &self.edge_spatial,
        };

        // Snap each location.
        let mut snaps: Vec<Snap> = Vec::with_capacity(locations.len());
        for loc in &locations {
            let snap = match snap_mode {
                SnapMode::Node => self
                    .node_spatial
                    .nearest(loc.lat, loc.lon, self.max_radius_m)
                    .map(|(idx, lat, lon, _)| Snap::Node {
                        node_idx: idx as usize,
                        pos: router_types::coordinate::LatLon(lat, lon),
                    }),
                SnapMode::Edge => snapper
                    .snap_to_edge(
                        loc.lat,
                        loc.lon,
                        self.max_radius_m,
                        Some(profile.vehicle_type),
                    )
                    .map(Snap::Edge),
            };
            snaps.push(snap.ok_or_else(|| {
                Error::InvalidRequest(format!(
                    "no routable position found near ({}, {})",
                    loc.lat, loc.lon
                ))
            })?);
        }

        // Build canonical waypoint locations from snaps.
        let snapped_locations: Vec<Location> = snaps
            .iter()
            .map(|snap| match snap {
                Snap::Node { node_idx, pos } => Location {
                    id: self.nodes.get(*node_idx).ok().map(|n| n.id.0.to_string()),
                    coordinate: *pos,
                    ..Default::default()
                },
                Snap::Edge(e) => Location {
                    coordinate: e.pos,
                    way_id: NonZeroU64::new(e.way_id),
                    fraction: Some(e.fraction),
                    ..Default::default()
                },
            })
            .collect();

        // Route leg by leg (one leg per consecutive location pair).
        let mut legs: Vec<Leg> = Vec::with_capacity(snaps.len() - 1);
        let mut trip_bounds = BoundingBox::VOID;
        let mut trip_duration_ms: u64 = 0;
        let mut trip_length_m: u32 = 0;

        for window in snaps.windows(2) {
            let (start_snap, goal_snap) = (&window[0], &window[1]);

            let inner = RoadGraph {
                nodes: &self.nodes,
                ways: &self.ways,
                cost_model: SpeedMap {
                    profile,
                    speed_config: &self.speed_config,
                    dim_table: &self.dim_table,
                    avoid_toll: request.avoid_toll,
                    avoid_ferry: request.avoid_ferry,
                },
                goal_pos: goal_snap.pos(),
            };
            let (graph, start_idx, goal_idx) = VirtualGraph::new(inner, start_snap, goal_snap);

            let Some((path_nodes, cost_ms)) = request.algorithm.run(&graph, start_idx, goal_idx)
            else {
                return Ok(RouteResponse {
                    id: request.id,
                    profile: profile.name.to_owned(),
                    units,
                    locations: snapped_locations,
                    trip_summary: Summary {
                        duration: Duration::ZERO,
                        length: 0,
                        bounds: BoundingBox::VOID,
                    },
                    legs: Vec::new(),
                });
            };

            // Resolve a path node index to a position, handling virtual sentinels.
            let resolve_pos = |idx: usize| match idx {
                VIRTUAL_START => start_snap.pos(),
                VIRTUAL_GOAL => goal_snap.pos(),
                i => self.nodes.get(i).map(|n| n.pos).unwrap_or(start_snap.pos()),
            };

            // Build geometry and compute leg metrics.
            let mut leg_bounds = BoundingBox::VOID;
            let mut leg_length_m: u32 = 0;
            let mut coords: Vec<[f32; 2]> = Vec::with_capacity(path_nodes.len());

            for i in 0..path_nodes.len() {
                let pos = resolve_pos(path_nodes[i]);
                leg_bounds.add(pos);
                coords.push([pos.lat, pos.lon]);
                if i > 0 {
                    let prev = resolve_pos(path_nodes[i - 1]);
                    leg_length_m = leg_length_m.saturating_add(haversine_m(prev, pos) as u32);
                }
            }

            trip_bounds.expand(&leg_bounds);
            trip_duration_ms += cost_ms as u64;
            trip_length_m = trip_length_m.saturating_add(leg_length_m);

            legs.push(Leg {
                leg_summary: Summary {
                    duration: Duration::from_millis(cost_ms as u64),
                    length: leg_length_m,
                    bounds: leg_bounds,
                },
                path: Points::encoded_from(coords),
                maneuvers: Vec::new(), // TODO: maneuver generation
            });
        }

        Ok(RouteResponse {
            id: request.id,
            profile: profile.name.to_owned(),
            units,
            locations: snapped_locations,
            trip_summary: Summary {
                duration: Duration::from_millis(trip_duration_ms),
                length: trip_length_m,
                bounds: trip_bounds,
            },
            legs,
        })
    }
}
