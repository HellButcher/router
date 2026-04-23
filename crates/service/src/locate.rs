use rayon::prelude::*;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::common::SingleOrVec;
use crate::error::Result;
use crate::meta::{EdgeMeta, NodeMeta, WayMeta};
use crate::snap::EdgeSnapper;

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
    profile::VehicleType,
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
    /// For node snaps: the snapped [`NodeMeta`].
    /// For edge snaps: [`EdgeMeta`], [`WayMeta`], and the two endpoint [`NodeMeta`]s
    /// (`from_node_idx` / `to_node_idx` in [`EdgeMeta`] index into `node_meta`).
    Light,
    /// Like [`MetaDetail::Light`], but `node_meta` contains all ordered nodes of the
    /// full OSM way; `from_node_idx` / `to_node_idx` still point to the snapped edge.
    FullWay,
}

// ── SnapMode ──────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum SnapMode {
    /// Snap to the nearest graph node.
    Node,
    /// Snap to the nearest point on the nearest way segment.
    /// The response location will include `way_id` and `fraction`.
    #[default]
    Edge,
}

// ── request / response ────────────────────────────────────────────────────────

/// A request to snap a list of coordinates to the nearest routable position.
///
/// See: [`LocateResponse`], [`Service::locate`]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LocateRequest {
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    pub locations: Locations,
    /// Whether to snap to the nearest node or the nearest point on a way
    /// segment. Defaults to [`SnapMode::Node`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub snap_mode: SnapMode,
    /// Controls how much meta information is included in the response.
    /// Defaults to [`MetaDetail::None`].
    #[cfg_attr(feature = "serde", serde(default))]
    pub with_meta: MetaDetail,
    /// When `true` and `snap_mode` is [`SnapMode::Edge`], ways that are
    /// inaccessible for the selected profile are skipped during snapping.
    /// Defaults to `false`.
    #[cfg_attr(feature = "serde", serde(default))]
    pub filter_by_profile: bool,
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
    pub profile: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    pub locations: Vec<Location>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

// ── service impl ──────────────────────────────────────────────────────────────

impl Service {
    /// Snap each input coordinate to the nearest routable position.
    pub async fn locate(&self, request: LocateRequest) -> Result<LocateResponse> {
        let profile = self.get_opt_profile(request.profile.as_deref())?;
        let profile_name = profile.name.to_owned();
        let restrict_to: Option<VehicleType> = if request.filter_by_profile {
            Some(profile.vehicle_type)
        } else {
            None
        };
        let mut locations: Vec<Location> = request.locations.try_into()?;

        let max_radius_m = self.max_radius_m;
        let snapper = EdgeSnapper {
            nodes: &self.nodes,
            edges: &self.edges,
            ways: &self.ways,
            edge_spatial: &self.edge_spatial,
        };

        locations.par_iter_mut().for_each(|loc| {
            let _span = tracing::trace_span!("locate").entered();
            match request.snap_mode {
                SnapMode::Node => {
                    if let Some((node_idx, snapped_lat, snapped_lon, _dist)) =
                        self.node_spatial.nearest(loc.lat, loc.lon, max_radius_m)
                    {
                        loc.coordinate.lat = snapped_lat;
                        loc.coordinate.lon = snapped_lon;
                        if request.with_meta != MetaDetail::None
                            && let Ok(node) = self.nodes.get(node_idx as usize)
                        {
                            loc.node_meta = Some(SingleOrVec::single(NodeMeta::from(&node)));
                        }
                    }
                }
                SnapMode::Edge => {
                    if let Some(snap) =
                        snapper.snap_to_edge(loc.lat, loc.lon, max_radius_m, restrict_to)
                    {
                        loc.coordinate = snap.pos;
                        loc.fraction = Some(snap.fraction);
                        if request.with_meta != MetaDetail::None
                            && let Ok(edge) = self.edges.get(snap.edge_idx)
                            && let Ok(way) = self.ways.get(edge.way_idx())
                        {
                            let (nodes, from_node_idx, to_node_idx) =
                                if request.with_meta == MetaDetail::FullWay {
                                    let wt = self.collect_way(edge.way_idx(), &way);
                                    let from = wt.node_pos(edge.from_node_idx());
                                    let to = wt.node_pos(edge.to_node_idx());
                                    (wt.nodes, from, to)
                                } else {
                                    // Light: just the two edge endpoints, indices are always 0 and 1
                                    let mut v = Vec::with_capacity(2);
                                    if let Ok(n) = self.nodes.get(edge.from_node_idx()) {
                                        v.push(NodeMeta::from(&n));
                                    }
                                    if let Ok(n) = self.nodes.get(edge.to_node_idx()) {
                                        v.push(NodeMeta::from(&n));
                                    }
                                    (v, Some(0), Some(1))
                                };
                            loc.edge_meta =
                                Some(EdgeMeta::from(&edge, from_node_idx, to_node_idx));
                            loc.way_meta = Some(WayMeta::from(&way));
                            loc.node_meta = Some(SingleOrVec::Vec(nodes));
                        }
                    }
                }
            }
        });

        Ok(LocateResponse {
            id: request.id,
            profile: profile_name,
            units: request.units,
            locations,
        })
    }
}
