use crate::error::Result;

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A request to snap a list of coordinates to the nearest routable node.
///
/// See: [`LocateResponse`], [`Service::locate`]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LocateRequest {
    pub profile: Option<String>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,
    pub locations: Locations,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

/// A response for a [`LocateRequest`], containing the snapped locations.
///
/// Each output location corresponds to the input at the same index. If a node
/// was found within the configured `max_radius_m`, the coordinate is replaced
/// with that node's exact position. Otherwise the input coordinate is returned
/// unchanged.
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

impl Service {
    /// Snap each input coordinate to the nearest routable node.
    ///
    /// Uses the spatial index with the configured `max_radius_m`. Inputs with
    /// no node within that radius are returned with their original coordinate.
    pub async fn locate(&self, request: LocateRequest) -> Result<LocateResponse> {
        let profile = self
            .get_opt_profile(request.profile.as_deref())?
            .name
            .to_owned();
        let mut locations: Vec<Location> = request.locations.try_into()?;

        let max_radius_m = self.max_radius_m;
        for loc in &mut locations {
            let _span = tracing::trace_span!("locate").entered();
            if let Some((_node_idx, snapped_lat, snapped_lon, _dist)) =
                self.spatial.nearest(loc.lat, loc.lon, max_radius_m)
            {
                loc.coordinate.lat = snapped_lat;
                loc.coordinate.lon = snapped_lon;
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
