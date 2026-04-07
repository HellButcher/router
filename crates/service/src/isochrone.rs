use std::collections::HashMap;

use router_algorithm::convex_hull::convex_hull;
use router_algorithm::dikstra::dijkstra_within_budget;
use router_storage::data::node::Node;
use router_storage::data::way::Way;
use router_storage::tablefile::TableFile;
use router_types::coordinate::LatLon;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::common::Points;
use crate::error::{Error, Result};
use crate::graph::{DistanceCost, RoadGraph, TravelTimeCost};
use crate::profile::Profile;
use crate::snap::{EdgeSnapper, Snap};
use crate::virtual_graph::{VIRTUAL_START, VirtualGraph};

use super::Service;

// ── Request / Response types ──────────────────────────────────────────────────

#[derive(Clone, Copy, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum IsochroneUnit {
    /// Distance in kilometres.
    #[default]
    #[cfg_attr(feature = "serde", serde(rename = "km"))]
    Km,
    /// Distance in miles.
    #[cfg_attr(feature = "serde", serde(rename = "mi"))]
    Mi,
    /// Travel time in minutes.
    #[cfg_attr(feature = "serde", serde(rename = "min"))]
    Min,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IsochroneRequest {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub origin: LatLon,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub unit: IsochroneUnit,
    /// Threshold values in the chosen unit. Need not be sorted.
    pub ranges: Vec<f64>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IsochroneRange {
    pub value: f64,
    /// Convex-hull polygon encoded as polyline, in [lat, lon] order.
    /// Closed: first point equals last.
    pub polygon: Points,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IsochroneResponse {
    pub profile: String,
    pub unit: IsochroneUnit,
    pub ranges: Vec<IsochroneRange>,
}

// ── Service impl ──────────────────────────────────────────────────────────────

impl Service {
    pub async fn calculate_isochrone(
        &self,
        request: IsochroneRequest,
    ) -> Result<IsochroneResponse> {
        let profile = self.get_opt_profile(request.profile.as_deref())?;

        if request.ranges.is_empty() {
            return Err(Error::InvalidRequest("ranges must not be empty".into()));
        }

        let mut ranges = request.ranges;
        ranges.retain(|v| v.is_finite() && *v > 0.0);
        if ranges.is_empty() {
            return Err(Error::InvalidRequest(
                "ranges must contain at least one positive finite value".into(),
            ));
        }
        ranges.sort_by(|a, b| a.total_cmp(b));
        ranges.dedup_by(|a, b| (*a - *b).abs() < f64::EPSILON);

        let max_cost = to_cost(*ranges.last().unwrap(), request.unit);

        let snapper = EdgeSnapper {
            nodes: &self.nodes,
            ways: &self.ways,
            edge_spatial: &self.edge_spatial,
        };
        let origin_snap = snapper
            .snap_to_edge(
                request.origin.lat,
                request.origin.lon,
                self.max_radius_m,
                Some(profile.vehicle_type),
            )
            .map(Snap::Edge)
            .ok_or_else(|| {
                Error::InvalidRequest(format!(
                    "no routable position found near ({}, {})",
                    request.origin.lat, request.origin.lon
                ))
            })?;

        let origin_pos = origin_snap.pos();
        let dist_map = run_isochrone(
            &origin_snap,
            max_cost,
            &self.nodes,
            &self.ways,
            profile,
            request.unit,
        );

        let result_ranges = ranges
            .iter()
            .map(|&range_val| {
                let threshold = to_cost(range_val, request.unit);
                let mut pts: Vec<[f32; 2]> = dist_map
                    .iter()
                    .filter_map(|(&node_idx, &cost)| {
                        if cost <= threshold && node_idx != VIRTUAL_START {
                            self.nodes
                                .get(node_idx)
                                .ok()
                                .map(|n| [n.pos.lat, n.pos.lon])
                        } else {
                            None
                        }
                    })
                    .collect();
                // Always include the origin so the hull is non-empty.
                pts.push([origin_pos.lat, origin_pos.lon]);
                let hull = convex_hull(pts);
                IsochroneRange {
                    value: range_val,
                    polygon: Points::encoded_from(hull),
                }
            })
            .collect();

        Ok(IsochroneResponse {
            profile: profile.name.to_owned(),
            unit: request.unit,
            ranges: result_ranges,
        })
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Convert a user-facing range value to a graph cost unit.
fn to_cost(value: f64, unit: IsochroneUnit) -> usize {
    match unit {
        IsochroneUnit::Km => (value * 1_000.0) as usize, // metres
        IsochroneUnit::Mi => (value * 1_609.344) as usize, // metres
        IsochroneUnit::Min => (value * 60_000.0) as usize, // milliseconds
    }
}

fn run_isochrone(
    origin_snap: &Snap,
    max_cost: usize,
    nodes: &TableFile<Node>,
    ways: &TableFile<Way>,
    profile: &Profile,
    unit: IsochroneUnit,
) -> HashMap<usize, usize> {
    // A dummy goal is required to build the VirtualGraph; it is never reached.
    let dummy_goal = Snap::Node {
        node_idx: 0,
        pos: origin_snap.pos(),
    };
    match unit {
        IsochroneUnit::Km | IsochroneUnit::Mi => {
            let inner = RoadGraph {
                nodes,
                ways,
                cost_model: DistanceCost {
                    vehicle_type: profile.vehicle_type,
                },
                goal_pos: origin_snap.pos(),
            };
            let (graph, start_idx, _) = VirtualGraph::new(inner, origin_snap, &dummy_goal);
            dijkstra_within_budget(&graph, start_idx, max_cost)
        }
        IsochroneUnit::Min => {
            let inner = RoadGraph {
                nodes,
                ways,
                cost_model: TravelTimeCost { profile },
                goal_pos: origin_snap.pos(),
            };
            let (graph, start_idx, _) = VirtualGraph::new(inner, origin_snap, &dummy_goal);
            dijkstra_within_budget(&graph, start_idx, max_cost)
        }
    }
}
