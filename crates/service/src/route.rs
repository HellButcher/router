use std::time::Duration;

use router_algorithm::a_star::a_star;
use router_types::bbox::BoundingBox;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use router_algorithm::Graph;

use crate::{
    error::{Error, Result},
    graph::{RoadGraph, TravelTimeCost, haversine_m},
};

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct RouteRequest {
    #[cfg_attr(feature = "serde", serde(default))]
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,

    pub locations: Locations,

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
        let locations: Vec<Location> = request.locations.try_into()?;

        if locations.len() < 2 {
            return Err(Error::InvalidRequest(
                "at least two locations are required".into(),
            ));
        }

        // Snap each location to the nearest node.
        let all_nodes = self.nodes.get_all().map_err(Error::StorageError)?;
        tracing::debug!(total_nodes = all_nodes.len(), "snapping locations");
        let mut snapped: Vec<usize> = Vec::with_capacity(locations.len());
        for loc in &locations {
            let (node_idx, _lat, _lon, _dist) = self
                .spatial
                .nearest(loc.lat, loc.lon, self.max_radius_m)
                .ok_or_else(|| {
                    Error::InvalidRequest(format!(
                        "no routable node found near ({}, {})",
                        loc.lat, loc.lon
                    ))
                })?;
            snapped.push(node_idx as usize);
        }

        // Use snapped node positions as the canonical waypoint locations.
        let snapped_locations: Vec<Location> = snapped
            .iter()
            .map(|&idx| Location {
                id: Some(all_nodes[idx].id.0.to_string()),
                coordinate: all_nodes[idx].pos,
                ..Default::default()
            })
            .collect();

        // Route leg by leg (one leg per consecutive location pair).
        let mut legs: Vec<Leg> = Vec::with_capacity(snapped.len() - 1);
        let mut trip_bounds = BoundingBox::VOID;
        let mut trip_duration_ms: u64 = 0;
        let mut trip_length_m: u32 = 0;

        for window in snapped.windows(2) {
            let (start, goal) = (window[0], window[1]);
            let graph = RoadGraph {
                nodes: &self.nodes,
                ways: &self.ways,
                cost_model: TravelTimeCost { profile },
                goal_pos: all_nodes[goal].pos,
            };

            let Some((path_nodes, cost_ms)) = a_star(&graph, start, goal) else {
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

            // Build geometry and compute leg metrics.
            let mut leg_bounds = BoundingBox::VOID;
            let mut leg_length_m: u32 = 0;
            let mut coords: Vec<[f32; 2]> = Vec::with_capacity(path_nodes.len());

            for i in 0..path_nodes.len() {
                let pos = all_nodes[path_nodes[i]].pos;
                leg_bounds.add(pos);
                coords.push([pos.lat, pos.lon]);
                if i > 0 {
                    let prev = all_nodes[path_nodes[i - 1]].pos;
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
