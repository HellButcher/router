use std::collections::HashMap;
use std::io;

use router_algorithm::convex_hull::convex_hull;
use router_algorithm::dikstra::dijkstra_within_budget;

use router_types::coordinate::LatLon;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::common::Points;
use crate::error::{Error, Result};
use crate::graph::{DistanceCost, SpeedMap};
use crate::profile::Profile;
use crate::snap::Snap;
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

    /// When `true`, routes avoid all toll roads and toll booths entirely.
    #[cfg_attr(feature = "serde", serde(default))]
    pub avoid_toll: bool,

    /// When `true`, routes avoid ferry connections entirely.
    #[cfg_attr(feature = "serde", serde(default))]
    pub avoid_ferry: bool,
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

        let origin_snaps = self.snapper()?.snap(
            request.origin.lat,
            request.origin.lon,
            self.max_radius_m,
            Some(profile.vehicle_type),
        );
        if origin_snaps.is_empty() {
            return Err(Error::InvalidRequest(format!(
                "no routable position found near ({}, {})",
                request.origin.lat, request.origin.lon
            )));
        }

        let origin_pos = origin_snaps[0].pos;
        let dist_map = self.run_isochrone(
            &origin_snaps,
            max_cost,
            profile,
            request.avoid_toll,
            request.avoid_ferry,
            request.unit,
        )?;

        let edge_nodes = self.edge_nodes.get_all()?;
        let geometry = self.geometry.get_all()?;
        // TODO: optimize hull computation by iterating dist_map once and collecting points for all ranges in one pass, rather than iterating once per range
        // (also exclude points already included in smaller ranges, since they won't affect the hull
        // of larger ranges. instead include the hull-points of the smaller range as fixed points for the larger range, since they will be on the hull of the larger range too)
        let result_ranges = ranges
            .iter()
            .map(|&range_val| {
                let threshold = to_cost(range_val, request.unit);
                let mut pts: Vec<[f32; 2]> = dist_map
                    .iter()
                    .filter_map(|(&edge_node_idx, &cost)| {
                        if cost <= threshold && edge_node_idx != VIRTUAL_START {
                            let edge_node = edge_nodes.get(edge_node_idx)?;
                            geometry
                                .get(edge_node.geometry_from_idx())
                                .map(|pos| [pos.lat, pos.lon])
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

impl Service {
    fn run_isochrone(
        &self,
        origin_snaps: &[Snap],
        max_cost: usize,
        profile: &Profile,
        avoid_toll: bool,
        avoid_ferry: bool,
        unit: IsochroneUnit,
    ) -> Result<HashMap<usize, usize>, io::Error> {
        match unit {
            IsochroneUnit::Km | IsochroneUnit::Mi => {
                let inner = self.road_graph(DistanceCost {
                    vehicle_type: profile.vehicle_type,
                })?;
                let (graph, start_idx) = VirtualGraph::new_from_start(inner, origin_snaps);
                Ok(dijkstra_within_budget(&graph, start_idx, max_cost))
            }
            IsochroneUnit::Min => {
                let inner = self.road_graph(SpeedMap {
                    profile,
                    speed_config: &self.speed_config,
                    avoid_toll,
                    avoid_ferry,
                })?;
                let (graph, start_idx) = VirtualGraph::new_from_start(inner, origin_snaps);
                Ok(dijkstra_within_budget(&graph, start_idx, max_cost))
            }
        }
    }
}
