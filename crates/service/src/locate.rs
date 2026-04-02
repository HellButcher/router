use crate::error::Result;

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A Resuest to locate a list of locations to the closest locations on the map (snapping).
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

/// A Response for a [`LocateRequest`], containing the snapped locations.
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
    /// Locate a list of locations to the closest locations on the map (snapping).
    pub async fn locate(&self, request: LocateRequest) -> Result<LocateResponse> {
        let profile = self.get_opt_profile(request.profile.as_deref())?.to_owned();
        let locations: Vec<Location> = request.locations.try_into()?;
        // TODO
        Ok(LocateResponse {
            id: request.id,
            profile,
            units: request.units,
            locations,
        })
    }
}
