use std::io;

use rayon::prelude::*;
use router_storage::{
    data::{edge_node::EdgeNode, way::Way},
    spatial::SpatialIndex,
    tablefile::Ref,
};
use router_types::coordinate::LatLon;

use crate::{
    Service,
    common::{Location, Unit},
    error::Error,
    profile::VehicleType,
};

// ── Snap ──────────────────────────────────────────────────────────────────────

pub struct Snap {
    pub edge_node_idx: usize,
    /// Projected position on the edges geometry.
    pub pos: LatLon,
    /// Segment index within the EdgeNode geometry (in traversal direction).
    pub seg_idx: usize,
    /// 0.0 = geometry start, 1.0 = geometry end (forward EdgeNode direction).
    pub distance_from_start_m: usize,
    pub distance_to_end_m: usize,
    pub distance_m: f32,
}

impl Snap {
    #[inline]
    pub fn edge_distance_m(&self) -> usize {
        self.distance_from_start_m + self.distance_to_end_m
    }
    #[inline]
    pub fn fraction(&self) -> f32 {
        let total_distance = self.edge_distance_m();
        if total_distance > 0 {
            self.distance_from_start_m as f32 / total_distance as f32
        } else {
            0.0
        }
    }
}

// ── EdgeNodeSnapper ───────────────────────────────────────────────────────────

pub struct Snapper<'a> {
    pub edge_nodes: Ref<'a, [EdgeNode]>,
    pub ways: Ref<'a, [Way]>,
    pub geometry: Ref<'a, [LatLon]>,
    pub edge_node_spatial: &'a SpatialIndex,
}

impl Service {
    pub fn snapper(&self) -> Result<Snapper<'_>, io::Error> {
        Ok(Snapper {
            edge_nodes: self.edge_nodes.get_all()?,
            ways: self.ways.get_all()?,
            geometry: self.geometry.get_all()?,
            edge_node_spatial: &self.edge_node_spatial,
        })
    }

    pub fn snap_all_mut(
        &self,
        radius_unit: Unit,
        vehicle_type: Option<VehicleType>,
        locations: &mut [Location],
    ) -> Result<Vec<Vec<Snap>>, Error> {
        let max_radius_m = self.max_radius_m;
        locations
            .par_iter_mut()
            .map(|loc| {
                let snaps = self.snapper()?.snap(
                    loc.lat,
                    loc.lon,
                    loc.radius
                        .map_or(max_radius_m, |r| radius_unit.to_meters(r).min(max_radius_m)),
                    vehicle_type,
                );
                if let Some(first_pos) = snaps.first().map(|s| s.pos) {
                    loc.coordinate = first_pos;
                    Ok(snaps)
                } else {
                    Err(Error::LocationNotFound(loc.coordinate))
                }
            })
            .collect::<Result<Vec<_>, _>>()
    }
}

impl Snapper<'_> {
    pub fn snap(
        &self,
        lat: f32,
        lon: f32,
        max_radius_m: f32,
        vehicle_type: Option<VehicleType>,
    ) -> Vec<Snap> {
        let p = LatLon(lat, lon);

        // First pass: best accessible EdgeNode.
        // (en_idx, way_idx, way_id, way_flags, pos, fraction, distance_m)
        let (distance_m, results) =
            self.edge_node_spatial
                .nearest_refined(lat, lon, max_radius_m, |entry, _bbox_dist| {
                    let idx = entry.idx();
                    let en = self.edge_nodes.get(idx)?;
                    let way = self.ways.get(en.way_idx as usize)?;
                    if vehicle_type.is_some_and(|v| way.access.blocks(v)) {
                        return None;
                    }
                    let (pos, seg_idx, frac, dist) = self.project(p, en);
                    Some((dist, (idx, pos, seg_idx, frac)))
                });

        let mut first_point: Option<LatLon> = None;
        results
            .into_iter()
            .filter_map(|(en_idx, pos, seg_idx, seg_frac)| {
                if let Some(fp) = first_point {
                    if !fp.is_quasi_equal(pos) {
                        return None;
                    }
                } else {
                    first_point = Some(pos);
                }
                let en = self.edge_nodes.get(en_idx)?;
                let frac_dist_m = fractional_distance_on_geometry(
                    &self.geometry[en.geometry_range()],
                    seg_idx,
                    seg_frac,
                );
                let total_dist_m = en.dist_m as usize;
                let (distance_from_start_m, distance_to_end_m, snap_seg_idx) = if en.is_backward() {
                    let nseg = en.geometry_count() - 1;
                    (total_dist_m - frac_dist_m, frac_dist_m, nseg - seg_idx)
                } else {
                    (frac_dist_m, total_dist_m - frac_dist_m, seg_idx)
                };
                Some(Snap {
                    edge_node_idx: en_idx,
                    pos,
                    seg_idx: snap_seg_idx,
                    distance_from_start_m,
                    distance_to_end_m,
                    distance_m,
                })
            })
            .collect()
    }

    fn project(&self, p: LatLon, en: &EdgeNode) -> (LatLon, usize, f32, f32) {
        let (pos, seg_idx, frac, dist) =
            project_onto_geometry(p, &self.geometry[en.geometry_range()]);
        if en.is_backward() {
            let nseg = en.geometry_count() - 1;
            (pos, nseg - seg_idx, 1.0 - frac, dist)
        } else {
            (pos, seg_idx, frac, dist)
        }
    }
}

// ── geometry helpers ──────────────────────────────────────────────────────────

fn project_onto_geometry(p: LatLon, geom: &[LatLon]) -> (LatLon, usize, f32, f32) {
    if geom.is_empty() {
        return (p, 0, 0.0, 0.0);
    }
    if geom.len() == 1 {
        let d = crate::graph::haversine_m(p, geom[0]);
        return (geom[0], 0, 0.0, d);
    }
    let mut best_dist = f32::MAX;
    let mut best_pos = geom[0];
    let mut best_seg_idx = 0;
    let mut best_frag = 0.0f32;
    for i in 0..geom.len() - 1 {
        let seg_from = geom[i];
        let seg_to = geom[i + 1];
        let (proj, t) = project_onto_segment(p, seg_from, seg_to);
        let d = crate::graph::haversine_m(p, proj);
        if d < best_dist {
            best_dist = d;
            best_pos = proj;
            best_seg_idx = i;
            best_frag = t;
        }
    }
    (best_pos, best_seg_idx, best_frag, best_dist)
}

pub fn project_onto_segment(p: LatLon, a: LatLon, b: LatLon) -> (LatLon, f32) {
    let cos_lat = (((a.lat + b.lat) * 0.5) as f64).to_radians().cos() as f32;
    let dx = b.lat - a.lat;
    let dy = (b.lon - a.lon) * cos_lat;
    let len_sq = dx * dx + dy * dy;
    let t = if len_sq > 0.0 {
        ((p.lat - a.lat) * dx + (p.lon - a.lon) * cos_lat * dy) / len_sq
    } else {
        0.0
    }
    .clamp(0.0, 1.0);
    let proj = LatLon(a.lat + t * (b.lat - a.lat), a.lon + t * (b.lon - a.lon));
    (proj, t)
}

#[inline]
pub fn fractional_distance_on_geometry_f32(geom: &[LatLon], frac: f32) -> usize {
    fractional_distance_on_geometry(geom, frac.trunc() as usize, frac.fract())
}

pub fn fractional_distance_on_geometry(
    geom: &[LatLon],
    mut seg_idx: usize,
    mut seg_frac: f32,
) -> usize {
    if seg_frac >= 1.0 {
        seg_idx += 1;
        seg_frac = 0.0;
    }
    if seg_idx >= geom.len() {
        seg_idx = geom.len() - 1;
    }
    let mut dist = 0f32;
    for i in 0..seg_idx {
        dist += crate::graph::haversine_m(geom[i], geom[i + 1]);
    }
    if seg_idx < geom.len() - 1 && seg_frac > 0.0 {
        let seg_from = geom[seg_idx];
        let seg_to = geom[seg_idx + 1];
        let dx = seg_to.lat - seg_from.lat;
        let dy = seg_to.lon - seg_from.lon;
        let mid = LatLon(seg_from.lat + dx * seg_frac, seg_from.lon + dy * seg_frac);
        let seg_dist = crate::graph::haversine_m(seg_from, mid);
        dist += seg_dist * seg_frac;
    }
    dist as usize
}

// ── Access check ──────────────────────────────────────────────────────────────
