use router_storage::{
    data::{edge::Edge, node::Node, way::Way},
    spatial::SpatialIndex,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;

use crate::{
    graph::{edge_is_blocked, haversine_m},
    profile::VehicleType,
};

// ── Snap ──────────────────────────────────────────────────────────────────────

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

pub struct EdgeSnap {
    /// Index into the edge table (`edges.bin`).
    pub edge_idx: usize,
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

pub struct EdgeSnapper<'a> {
    pub nodes: &'a TableFile<Node>,
    pub edges: &'a TableFile<Edge>,
    pub ways: &'a TableFile<Way>,
    pub edge_spatial: &'a SpatialIndex,
}

impl<'a> EdgeSnapper<'a> {
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
                let edge_idx = entry.index as usize;
                let edge = self.edges.get(edge_idx).ok()?;
                if restrict_to.is_some_and(|v| edge_is_blocked(&edge, v)) {
                    return None;
                }
                let from = self.nodes.get(edge.from_node_idx as usize).ok()?;
                let to = self.nodes.get(edge.to_node_idx as usize).ok()?;
                let (snapped, fraction) = project_onto_segment(p, from.pos, to.pos);
                let distance_m = haversine_m(p, snapped);
                let way_id = self
                    .ways
                    .get(edge.way_idx())
                    .map(|w| w.id.0 as u64)
                    .unwrap_or(0);
                Some((
                    distance_m,
                    EdgeSnap {
                        edge_idx,
                        way_id,
                        pos: snapped,
                        fraction,
                        distance_m,
                        from_node_idx: edge.from_node_idx as usize,
                        to_node_idx: edge.to_node_idx as usize,
                    },
                ))
            })
    }
}

// ── geometry ──────────────────────────────────────────────────────────────────

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
