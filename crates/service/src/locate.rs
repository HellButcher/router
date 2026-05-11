use rayon::prelude::*;
use router_types::coordinate::LatLon;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::meta::{EdgeMeta, WayMeta};
use crate::snap::{Snap, Snapper};

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};

// ── MetaDetail ────────────────────────────────────────────────────────────────

/// Controls how much meta information is included in locate responses.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum MetaDetail {
    /// No meta information.
    #[default]
    None,
    /// include [`EdgeMeta`], [`WayMeta`], and the two endpoins of the edge.
    /// (`from_node_idx-0` / `to_node_idx=1` in [`EdgeMeta`]).
    Light,
    /// Like [`MetaDetail::Light`], but include all points of the edge geometry.
    /// (`from_node_idx=0` / `to_node_idx=points.len()-1`).
    FullEdge,
    /// Like [`MetaDetail::Light`], but includes all nodes of the way.
    /// `from_node_idx` / `to_node_idx` point to the sub-range of the points.
    FullWay,
}

// ── request / response ────────────────────────────────────────────────────────

/// A request to snap a list of coordinates to the nearest routable position.
///
/// See: [`LocateResponse`], [`Service::locate`]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LocateRequest {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    pub locations: Locations,
    /// Controls how much meta information is included in the response.
    /// Defaults to [`MetaDetail::None`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub with_meta: MetaDetail,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

/// A response for a [`LocateRequest`], containing the snapped locations.
///
/// Each output location corresponds to the input at the same index.  If a
/// routable position was found within `max_radius_m`, the coordinate is
/// replaced with the snapped position.  Otherwise the input coordinate is
/// returned unchanged.
///
/// For [`SnapMode::Edge`] snaps the location also carries `way_id` and
/// `fraction` (0.0 = from-node end, 1.0 = to-node end).
///
/// See: [`LocateRequest`], [`Service::locate`]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LocateResponse {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    pub locations: Vec<LocateResponseLocation>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LocateResponseLocation {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub location: Location,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub distance: Option<f32>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)
    )]
    pub meta_points: Option<Points>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)
    )]
    pub edge_meta: Option<EdgeMeta>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none", skip_deserializing)
    )]
    pub way_meta: Option<WayMeta>,
}

impl<L: Into<Location>> From<L> for LocateResponseLocation {
    fn from(coord: L) -> Self {
        Self {
            location: coord.into(),
            distance: None,
            meta_points: None,
            edge_meta: None,
            way_meta: None,
        }
    }
}

impl TryFrom<Locations> for Vec<LocateResponseLocation> {
    type Error = router_polyline::Error;
    fn try_from(locs: Locations) -> Result<Self, router_polyline::Error> {
        Ok(match locs {
            Locations::LocationArray(locs) => locs.into_iter().map(Into::into).collect(),
            Locations::Array(coords) => coords.into_iter().map(Into::into).collect(),
            Locations::Encoded(s) => router_polyline::decode::<2>(&s, 5)?
                .into_iter()
                .map(Into::into)
                .collect(),
        })
    }
}

// ── service impl ──────────────────────────────────────────────────────────────

impl Snapper<'_> {
    pub fn resolve_meta(
        &self,
        snap: &Snap,
        meta_detail: MetaDetail,
    ) -> Option<(EdgeMeta, WayMeta, Vec<LatLon>)> {
        if meta_detail == MetaDetail::None {
            return None;
        }
        let mut points = Vec::new();
        let edge = self.edge_nodes.get(snap.edge_node_idx)?;
        let way = self.ways.get(edge.way_idx())?;

        let (from_node_idx, to_node_idx) = match meta_detail {
            MetaDetail::FullWay => {
                points.reserve(way.geometry_len());
                if edge.is_backward() {
                    points.extend(self.geometry[edge.geometry_range()].iter().rev().cloned());
                    let ofs = way.geometry_offset_idx() + way.geometry_len();
                    let from = ofs - edge.geometry_from_idx();
                    let to = from + edge.geometry_count();
                    (Some(from), Some(to))
                } else {
                    points.extend_from_slice(&self.geometry[edge.geometry_range()]);
                    let ofs = way.geometry_offset_idx();
                    let from = edge.geometry_from_idx() - ofs;
                    let to = from + edge.geometry_count();
                    (Some(from), Some(to))
                }
            }
            MetaDetail::FullEdge => {
                points.reserve(edge.geometry_count());
                if edge.is_backward() {
                    points.extend(self.geometry[edge.geometry_range()].iter().rev().cloned());
                } else {
                    points.extend_from_slice(&self.geometry[edge.geometry_range()]);
                }
                (Some(0), Some(points.len()))
            }
            MetaDetail::Light => {
                points.reserve_exact(2);
                points.push(self.geometry[edge.geometry_from_idx()]);
                points.push(self.geometry[edge.geometry_to_idx()]);
                (Some(0), Some(1))
            }
            MetaDetail::None => unreachable!(),
        };

        let edge_meta = EdgeMeta::from(edge, from_node_idx, to_node_idx);
        let way_meta = WayMeta::from(way);
        Some((edge_meta, way_meta, points))
    }
}

impl Service {
    /// Snap each input coordinate to the nearest routable position.
    pub async fn locate(&self, request: LocateRequest) -> Result<LocateResponse> {
        let (profile, restrict_to) = if let Some(profile_name) = &request.profile {
            let profile = self.get_profile(profile_name)?;
            (Some(profile.name.to_owned()), Some(profile.vehicle_type))
        } else {
            (None, None)
        };
        let mut locations: Vec<LocateResponseLocation> = request.locations.try_into()?;

        let unit = request.units;
        let max_radius_m = self.max_radius_m;

        locations.par_iter_mut().for_each(|loc| {
            let _span = tracing::trace_span!("locate").entered();
            let Ok(snapper) = self.snapper() else {
                tracing::error!("Failed to create snapper for locate request");
                return;
            };
            if let Some(snap) = snapper
                .snap(
                    loc.location.lat,
                    loc.location.lon,
                    loc.location
                        .radius
                        .map_or(max_radius_m, |r| unit.to_meters(r).min(max_radius_m)),
                    restrict_to,
                )
                .first()
            {
                loc.distance = Some(unit.from_meters(snap.distance_m));
                loc.location.lat = snap.pos.lat;
                loc.location.lon = snap.pos.lon;
                if let Some((edge_meta, way_meta, meta_points)) =
                    snapper.resolve_meta(snap, request.with_meta)
                {
                    loc.edge_meta = Some(edge_meta);
                    loc.way_meta = Some(way_meta);
                    loc.meta_points = Some(Points::encoded_from(meta_points));
                }
            }
        });

        Ok(LocateResponse {
            id: request.id,
            profile,
            units: request.units,
            locations,
        })
    }
}
