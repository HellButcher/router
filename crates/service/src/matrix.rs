use std::collections::{HashMap, HashSet};
use std::time::Duration;

use rayon::prelude::*;
use router_algorithm::dikstra::dijkstra_ssmt;
use router_algorithm::reconstruct_path;
use router_storage::data::edge_node::EdgeNode;
use router_storage::data::way::Way;
use router_types::coordinate::LatLon;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::graph::{CostModel, SpeedMap, haversine_m};
use crate::snap::Snap;
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
pub struct MatrixSummary {
    pub duration: Duration,
    pub length: f32,
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

        let (mut from_locs, mut to_locs) = match request.locations {
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

        let from_snaps = self.snap_all_mut(units, Some(profile.vehicle_type), &mut from_locs)?;
        let to_snaps = self.snap_all_mut(units, Some(profile.vehicle_type), &mut to_locs)?;

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
                        length: 0.0,
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
            avoid_toll: request.avoid_toll,
            avoid_ferry: request.avoid_ferry,
        };
        let routed: Vec<(usize, MatrixResponseEntry)> = by_origin
            .into_par_iter()
            .flat_map(|(from_idx, dest_pairs)| {
                self.compute_origin(
                    units,
                    from_idx,
                    &dest_pairs,
                    &from_snaps,
                    &to_snaps,
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

    // ── Per-origin SSMT computation ───────────────────────────────────────────────

    fn compute_origin<C: CostModel + Copy>(
        &self,
        units: Unit,
        from_idx: usize,
        dest_pairs: &[(usize, usize)], // (pair_idx, dest_idx)
        from_snaps: &[Vec<Snap>],
        to_snaps: &[Vec<Snap>],
        cost_model: C,
    ) -> Vec<(usize, MatrixResponseEntry)> {
        let from_snap = &from_snaps[from_idx];

        // Collect the real graph nodes adjacent to every destination.
        let mut target_edge_nodes: HashSet<usize> = HashSet::new();
        for &(_, to_idx) in dest_pairs {
            for snap in &to_snaps[to_idx] {
                target_edge_nodes.insert(snap.edge_node_idx);
            }
        }

        let Ok(inner) = self.road_graph(cost_model) else {
            return Vec::new();
        };
        let (graph, start_idx) = VirtualGraph::new_from_start(inner, from_snap);
        let (time_costs, predecessors) = dijkstra_ssmt(&graph, start_idx, &target_edge_nodes);

        let mut entries = Vec::new();
        for &(pair_idx, to_idx) in dest_pairs {
            let to_snap = &to_snaps[to_idx];

            let Some((cost_ms, end_snap_idx)) = destination_cost(
                to_snap,
                &time_costs,
                &graph.edge_nodes,
                &graph.ways,
                cost_model,
            ) else {
                continue; // unreachable — omit
            };
            let end_snap = &to_snap[end_snap_idx];
            let path = reconstruct_path(&predecessors, start_idx, end_snap.edge_node_idx);
            let length_m = path_length(
                &path,
                from_snap[0].pos,
                &to_snap[end_snap_idx],
                &graph.edge_nodes,
                &graph.geometry,
            );

            entries.push((
                pair_idx,
                MatrixResponseEntry {
                    from: from_idx,
                    to: to_idx,
                    summary: MatrixSummary {
                        duration: Duration::from_millis(cost_ms as u64),
                        length: units.from_meters(length_m),
                    },
                },
            ));
        }
        entries
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Returns `true` when two snaps represent the same graph location.
fn snaps_identical(a: &[Snap], b: &[Snap]) -> bool {
    if let Some(a_first) = a.first()
        && let Some(b_first) = b.first()
    {
        a_first.pos.is_quasi_equal(b_first.pos)
    } else {
        false
    }
}

/// Compute the travel-time cost (ms) and the real graph node used to reach an
/// edge-snapped or node-snapped destination, given settled node costs from an
/// SSMT run.  Returns `None` if the destination is unreachable.
fn destination_cost<C: CostModel>(
    to_snap: &[Snap],
    time_costs: &HashMap<usize, usize>,
    edges: &[EdgeNode],
    ways: &[Way],
    cost_model: C,
) -> Option<(usize, usize)> {
    let mut best_cost = usize::MAX;
    let mut best_snap = usize::MAX;
    for (i, snap) in to_snap.iter().enumerate() {
        if let Some(cost) = destination_cost_single(snap, time_costs, edges, ways, &cost_model)
            && cost < best_cost
        {
            best_cost = cost;
            best_snap = i;
        }
    }
    let best_snap = to_snap.get(best_snap)?;
    Some((best_cost, best_snap.edge_node_idx))
}

fn destination_cost_single<C: CostModel>(
    snap: &Snap,
    time_costs: &HashMap<usize, usize>,
    edges: &[EdgeNode],
    ways: &[Way],
    cost_model: &C,
) -> Option<usize> {
    let base_cost = time_costs.get(&snap.edge_node_idx)?;
    let edge = edges.get(snap.edge_node_idx)?;
    let way = ways.get(edge.way_idx())?;
    let snap_edge_cost = cost_model.traversal_cost(snap.distance_from_start_m, edge, way)?;

    Some(base_cost + snap_edge_cost)
}

/// Sum haversine distances along the reconstructed node path, adding fractional
/// edge segments at origin/destination when those are edge-snapped.
fn path_length(
    path: &[usize],
    from_snap_pos: LatLon,
    to_snap: &Snap,
    edges: &[EdgeNode],
    geometry: &[LatLon],
) -> f32 {
    let resolve = |idx: usize| -> Option<router_types::coordinate::LatLon> {
        if idx == VIRTUAL_START {
            Some(from_snap_pos)
        } else if let Some(edge) = edges.get(idx) {
            geometry.get(edge.geometry_to_idx()).copied()
        } else {
            None
        }
    };

    let mut length_m = 0f32;
    for i in 1..path.len() {
        if let Some(a) = resolve(path[i - 1])
            && let Some(b) = resolve(path[i])
        {
            length_m += haversine_m(a, b);
        }
    }

    // Add fractional segment from the settled end_node to the actual snap point.
    length_m += to_snap.distance_from_start_m as f32;

    length_m
}
