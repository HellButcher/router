use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rayon::prelude::*;
use router_algorithm::dikstra::dijkstra_ssmt;
use router_algorithm::reconstruct_path;
use router_storage::data::attrib::WayFlags;
use router_storage::data::node::Node;
use router_storage::data::way::Way;
use router_storage::tablefile::TableFile;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::graph::{CostModel, RoadGraph, SpeedMap, haversine_m};
use crate::profile::VehicleType;
use crate::snap::{EdgeSnap, EdgeSnapper, Snap};
use crate::virtual_graph::{VIRTUAL_START, VirtualGraph};

use super::{
    Service,
    common::{Location, Locations, Unit},
};

// ── Request / Response types ──────────────────────────────────────────────────

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[cfg_attr(feature = "serde", serde(untagged))]
pub enum MatrixRequestLocations {
    Symetric { locations: Locations },
    Asymetric { from: Locations, to: Locations },
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MatrixRequest {
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub locations: MatrixRequestLocations,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub pairs: Vec<(usize, usize)>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MatrixSummary {
    pub duration: Duration,
    pub length: u32,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MatrixResponseEntry {
    pub from: usize,
    pub to: usize,
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub summary: MatrixSummary,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct MatrixResponse {
    pub profile: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    pub from: Vec<Location>,
    pub to: Vec<Location>,
    pub result: Vec<MatrixResponseEntry>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

// ── Service impl ──────────────────────────────────────────────────────────────

impl Service {
    pub async fn calculate_matrix(&self, request: MatrixRequest) -> Result<MatrixResponse> {
        let profile = self.get_opt_profile(request.profile.as_deref())?;
        let units = request.units;

        let (from_locs, to_locs) = match request.locations {
            MatrixRequestLocations::Symetric { locations } => {
                let locations: Vec<Location> = locations.try_into()?;
                (locations.clone(), locations)
            }
            MatrixRequestLocations::Asymetric { from, to } => (from.try_into()?, to.try_into()?),
        };

        if from_locs.is_empty() || to_locs.is_empty() {
            return Err(Error::InvalidRequest(
                "from and to must not be empty".into(),
            ));
        }

        let snapper = EdgeSnapper {
            nodes: &self.nodes,
            ways: &self.ways,
            edge_spatial: &self.edge_spatial,
        };

        let from_snaps = snap_all(
            &from_locs,
            &snapper,
            profile.vehicle_type,
            self.max_radius_m,
        )?;
        let to_snaps = snap_all(&to_locs, &snapper, profile.vehicle_type, self.max_radius_m)?;

        let pairs: Vec<(usize, usize)> = if request.pairs.is_empty() {
            (0..from_snaps.len())
                .flat_map(|i| (0..to_snaps.len()).map(move |j| (i, j)))
                .collect()
        } else {
            for &(i, j) in &request.pairs {
                if i >= from_snaps.len() || j >= to_snaps.len() {
                    return Err(Error::InvalidRequest(format!(
                        "pair ({i}, {j}) out of bounds (from={}, to={})",
                        from_snaps.len(),
                        to_snaps.len()
                    )));
                }
            }
            request.pairs
        };

        // Assign each pair its original index so we can restore order after
        // parallel execution.  Identity pairs are short-circuited immediately.
        // Non-identity pairs are grouped by origin for batched SSMT runs.
        let mut ordered: Vec<Option<MatrixResponseEntry>> =
            (0..pairs.len()).map(|_| None).collect();
        let mut by_origin: HashMap<usize, Vec<(usize, usize)>> = HashMap::new(); // origin → [(pair_idx, dest_idx)]

        for (pair_idx, &(i, j)) in pairs.iter().enumerate() {
            if snaps_identical(&from_snaps[i], &to_snaps[j]) {
                ordered[pair_idx] = Some(MatrixResponseEntry {
                    from: i,
                    to: j,
                    summary: MatrixSummary {
                        duration: Duration::ZERO,
                        length: 0,
                    },
                });
            } else {
                by_origin.entry(i).or_default().push((pair_idx, j));
            }
        }

        // Run one SSMT per unique origin, parallelised across origins.
        // SpeedMap is Copy (two shared refs) and Send + Sync.
        let speed_map = SpeedMap {
            profile,
            speed_config: &self.speed_config,
        };
        let routed: Vec<(usize, MatrixResponseEntry)> = by_origin
            .into_par_iter()
            .flat_map(|(from_idx, dest_pairs)| {
                compute_origin(
                    from_idx,
                    &dest_pairs,
                    &from_snaps,
                    &to_snaps,
                    &self.nodes,
                    &self.ways,
                    speed_map,
                )
            })
            .collect();

        for (pair_idx, entry) in routed {
            ordered[pair_idx] = Some(entry);
        }

        let result: Vec<MatrixResponseEntry> = ordered.into_iter().flatten().collect();

        Ok(MatrixResponse {
            id: request.id,
            profile: profile.name.to_owned(),
            units,
            from: from_locs,
            to: to_locs,
            result,
        })
    }
}

// ── Per-origin SSMT computation ───────────────────────────────────────────────

fn compute_origin<C: CostModel + Copy>(
    from_idx: usize,
    dest_pairs: &[(usize, usize)], // (pair_idx, dest_idx)
    from_snaps: &[Snap],
    to_snaps: &[Snap],
    nodes: &TableFile<Node>,
    ways: &TableFile<Way>,
    cost_model: C,
) -> Vec<(usize, MatrixResponseEntry)> {
    let from_snap = &from_snaps[from_idx];

    // Collect the real graph nodes adjacent to every destination.
    let mut target_nodes: HashSet<usize> = HashSet::new();
    for &(_, to_idx) in dest_pairs {
        match &to_snaps[to_idx] {
            Snap::Node { node_idx, .. } => {
                target_nodes.insert(*node_idx);
            }
            Snap::Edge(e) => {
                target_nodes.insert(e.from_node_idx);
                target_nodes.insert(e.to_node_idx);
            }
        }
    }

    let inner = RoadGraph {
        nodes,
        ways,
        cost_model,
        goal_pos: from_snap.pos(),
    };
    let (graph, start_idx) = VirtualGraph::new_from_start(inner, from_snap);

    let (time_costs, predecessors) = dijkstra_ssmt(&graph, start_idx, &target_nodes);

    let mut entries = Vec::new();
    for &(pair_idx, to_idx) in dest_pairs {
        let to_snap = &to_snaps[to_idx];

        let Some((cost_ms, end_node)) = destination_cost(to_snap, &time_costs, ways, cost_model)
        else {
            continue; // unreachable — omit
        };

        let path = reconstruct_path(&predecessors, start_idx, end_node);
        let length_m = path_length(&path, from_snap, to_snap, end_node, nodes);

        entries.push((
            pair_idx,
            MatrixResponseEntry {
                from: from_idx,
                to: to_idx,
                summary: MatrixSummary {
                    duration: Duration::from_millis(cost_ms as u64),
                    length: length_m,
                },
            },
        ));
    }
    entries
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn snap_all(
    locations: &[Location],
    snapper: &EdgeSnapper<'_>,
    vehicle_type: VehicleType,
    max_radius_m: f32,
) -> Result<Vec<Snap>> {
    locations
        .iter()
        .map(|loc| {
            snapper
                .snap_to_edge(loc.lat, loc.lon, max_radius_m, Some(vehicle_type))
                .map(Snap::Edge)
                .ok_or_else(|| {
                    Error::InvalidRequest(format!(
                        "no routable position found near ({}, {})",
                        loc.lat, loc.lon
                    ))
                })
        })
        .collect()
}

/// Returns `true` when two snaps represent the same graph location.
fn snaps_identical(a: &Snap, b: &Snap) -> bool {
    match (a, b) {
        (Snap::Node { node_idx: ai, .. }, Snap::Node { node_idx: bi, .. }) => ai == bi,
        (Snap::Edge(a), Snap::Edge(b)) => {
            a.way_idx == b.way_idx && (a.fraction - b.fraction).abs() < f32::EPSILON
        }
        _ => false,
    }
}

/// Compute the travel-time cost (ms) and the real graph node used to reach an
/// edge-snapped or node-snapped destination, given settled node costs from an
/// SSMT run.  Returns `None` if the destination is unreachable.
fn destination_cost<C: CostModel>(
    to_snap: &Snap,
    time_costs: &HashMap<usize, usize>,
    ways: &TableFile<Way>,
    cost_model: C,
) -> Option<(usize, usize)> {
    match to_snap {
        Snap::Node { node_idx, .. } => time_costs.get(node_idx).map(|&c| (c, *node_idx)),
        Snap::Edge(e) => edge_destination_cost(e, time_costs, ways, cost_model),
    }
}

fn edge_destination_cost<C: CostModel>(
    e: &EdgeSnap,
    time_costs: &HashMap<usize, usize>,
    ways: &TableFile<Way>,
    cost_model: C,
) -> Option<(usize, usize)> {
    let way = ways.get(e.way_idx).ok()?;
    let full_cost = cost_model.edge_cost(&way)?;

    // Approaching from the way's from-node (forward direction).
    let via_from = time_costs.get(&e.from_node_idx).map(|&base| {
        (
            base + (e.fraction * full_cost as f32) as usize,
            e.from_node_idx,
        )
    });

    // Approaching from the way's to-node (reverse, only on bidirectional ways).
    let via_to = if !way.flags.contains(WayFlags::ONEWAY) {
        time_costs.get(&e.to_node_idx).map(|&base| {
            (
                base + ((1.0 - e.fraction) * full_cost as f32) as usize,
                e.to_node_idx,
            )
        })
    } else {
        None
    };

    match (via_from, via_to) {
        (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// Sum haversine distances along the reconstructed node path, adding fractional
/// edge segments at origin/destination when those are edge-snapped.
fn path_length(
    path: &[usize],
    from_snap: &Snap,
    to_snap: &Snap,
    end_node: usize,
    nodes: &TableFile<Node>,
) -> u32 {
    let resolve = |idx: usize| -> router_types::coordinate::LatLon {
        if idx == VIRTUAL_START {
            from_snap.pos()
        } else {
            nodes.get(idx).map(|n| n.pos).unwrap_or(from_snap.pos())
        }
    };

    let mut length_m: u32 = 0;
    for i in 1..path.len() {
        length_m =
            length_m.saturating_add(haversine_m(resolve(path[i - 1]), resolve(path[i])) as u32);
    }

    // Add fractional segment from the settled end_node to the actual snap point.
    if let Snap::Edge(e) = to_snap {
        let end_pos = nodes.get(end_node).map(|n| n.pos).unwrap_or(e.pos);
        length_m = length_m.saturating_add(haversine_m(end_pos, e.pos) as u32);
    }

    length_m
}
