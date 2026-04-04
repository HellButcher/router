use std::num::NonZeroU64;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::snap::EdgeSnapper;

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};

// ── SnapMode ──────────────────────────────────────────────────────────────────

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum SnapMode {
    /// Snap to the nearest graph node (default).
    #[default]
    Node,
    /// Snap to the nearest point on the nearest way segment.
    /// The response location will include `way_id` and `fraction`.
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
        let profile = self
            .get_opt_profile(request.profile.as_deref())?
            .name
            .to_owned();
        let mut locations: Vec<Location> = request.locations.try_into()?;

        let max_radius_m = self.max_radius_m;
        let snapper = EdgeSnapper {
            nodes: &self.nodes,
            ways: &self.ways,
            edge_spatial: &self.edge_spatial,
        };

        for loc in &mut locations {
            let _span = tracing::trace_span!("locate").entered();
            match request.snap_mode {
                SnapMode::Node => {
                    if let Some((_node_idx, snapped_lat, snapped_lon, _dist)) =
                        self.spatial.nearest(loc.lat, loc.lon, max_radius_m)
                    {
                        loc.coordinate.lat = snapped_lat;
                        loc.coordinate.lon = snapped_lon;
                    }
                }
                SnapMode::Edge => {
                    if let Some(snap) = snapper.snap_to_edge(loc.lat, loc.lon, max_radius_m) {
                        loc.coordinate = snap.pos;
                        loc.way_id = NonZeroU64::new(snap.way_id);
                        loc.fraction = Some(snap.fraction);
                    }
                }
            }
        }

        Ok(LocateResponse {
            id: request.id,
            profile,
            units: request.units,
            locations,
        })
    }
}
