use router_storage::{
    data::{node::Node, way::Way},
    spatial::SpatialIndex,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;

use crate::{
    graph::{haversine_m, way_is_blocked},
    profile::VehicleType,
};

// ── Snap ──────────────────────────────────────────────────────────────────────

/// Result of snapping a waypoint coordinate — either to a node or to a point
/// on a way segment.
pub enum Snap {
    Node { node_idx: usize, pos: LatLon },
    Edge(EdgeSnap),
}

impl Snap {
    pub fn pos(&self) -> LatLon {
        match self {
            Self::Node { pos, .. } => *pos,
            Self::Edge(e) => e.pos,
        }
    }
}

// ── EdgeSnap ──────────────────────────────────────────────────────────────────

/// Result of snapping a point to the nearest way segment.
pub struct EdgeSnap {
    pub way_idx: usize,
    /// OSM way ID for use in responses.
    pub way_id: u64,
    pub pos: LatLon,
    /// 0.0 = from-node end, 1.0 = to-node end.
    pub fraction: f32,
    pub distance_m: f32,
    pub from_node_idx: usize,
    pub to_node_idx: usize,
}

// ── Snapper ───────────────────────────────────────────────────────────────────

/// Performs edge snapping against the storage layer.
///
/// Holds borrowed references so it can be constructed cheaply per-request.
pub struct EdgeSnapper<'a> {
    pub nodes: &'a TableFile<Node>,
    pub ways: &'a TableFile<Way>,
    pub edge_spatial: &'a SpatialIndex,
}

impl<'a> EdgeSnapper<'a> {
    /// Snap `(lat, lon)` to the nearest point on any way segment within
    /// `max_radius_m` metres.
    ///
    /// When `restrict_to` is `Some(vehicle)`, ways that are inaccessible for
    /// that vehicle type are skipped.
    pub fn snap_to_edge(
        &self,
        lat: f32,
        lon: f32,
        max_radius_m: f32,
        restrict_to: Option<VehicleType>,
    ) -> Option<EdgeSnap> {
        let p = LatLon(lat, lon);
        self.edge_spatial
            .nearest_refined(lat, lon, max_radius_m, |entry, _bbox_dist| {
                let way_idx = entry.index as usize;
                let way = self.ways.get(way_idx).ok()?;
                if restrict_to.is_some_and(|v| way_is_blocked(&way, v)) {
                    return None;
                }
                let from = self.nodes.get(way.from_node_idx as usize).ok()?;
                let to = self.nodes.get(way.to_node_idx as usize).ok()?;
                let (snapped, fraction) = project_onto_segment(p, from.pos, to.pos);
                let distance_m = haversine_m(p, snapped);
                Some((
                    distance_m,
                    EdgeSnap {
                        way_idx,
                        way_id: way.id.0 as u64,
                        pos: snapped,
                        fraction,
                        distance_m,
                        from_node_idx: way.from_node_idx as usize,
                        to_node_idx: way.to_node_idx as usize,
                    },
                ))
            })
    }
}

// ── geometry ──────────────────────────────────────────────────────────────────

/// Project point `p` onto segment `a → b`.
///
/// Returns `(projected_point, t)` where `t ∈ [0, 1]` is the fraction along
/// the segment (0 = `a`, 1 = `b`).  Uses equirectangular projection scaled by
/// `cos(mid_lat)` — accurate for segments up to a few hundred kilometres.
pub fn project_onto_segment(p: LatLon, a: LatLon, b: LatLon) -> (LatLon, f32) {
    let cos_lat = (((a.lat + b.lat) * 0.5) as f64).to_radians().cos() as f32;

    let ax = a.lat;
    let ay = a.lon * cos_lat;
    let bx = b.lat;
    let by = b.lon * cos_lat;
    let px = p.lat;
    let py = p.lon * cos_lat;

    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;

    let t = if len_sq > 0.0 {
        ((px - ax) * dx + (py - ay) * dy) / len_sq
    } else {
        0.0
    }
    .clamp(0.0, 1.0);

    let proj = LatLon(a.lat + t * (b.lat - a.lat), a.lon + t * (b.lon - a.lon));
    (proj, t)
}
