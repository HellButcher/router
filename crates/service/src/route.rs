use std::time::Duration;

use crate::error::Result;

pub use super::common::Points;
use super::{
    Service,
    common::{Location, Locations, Unit},
};
use router_types::bbox::BoundingBox;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RouteRequest {
    #[cfg_attr(feature = "serde", serde(default))]
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

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RouteResponse {
    pub profile: String,
    #[cfg_attr(feature = "serde", serde(default))]
    pub units: Unit,

    pub locations: Vec<Location>,

    #[cfg_attr(feature = "serde", serde(flatten))]
    pub trip_summary: Summary,

    pub legs: Vec<Leg>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Summary {
    pub duration: Duration,
    pub length: u32,
    pub bounds: BoundingBox,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Leg {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub leg_summary: Summary,

    pub path: Points,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub maneuvers: Vec<Maneuver>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Maneuver {
    #[cfg_attr(feature = "serde", serde(rename = "maneuver"))]
    pub maneuver_type: ManeuverType,
    pub maneuver_direction: Option<ManeuverDirection>,
    pub instruction: String,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Vec::is_empty")
    )]
    pub street_names: Vec<String>,
    pub path_segment: [usize; 2],
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ManeuverType {
    Start,
    Destination,
    Continue,
    Turn,
    SlightTurn,
    SharpTurn,
    UTurn,
    Ramp,
    Exit,
    Stay,
    Merge,
    RoundaboutEnter,
    RoundaboutExit(u8),
    FerryEnter,
    FerryExit,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ManeuverDirection {
    Straight,
    Left,
    Right,
}

impl Service {
    pub async fn calculate_route(&self, request: RouteRequest) -> Result<RouteResponse> {
        let profile = self.get_opt_profile(request.profile.as_deref())?.to_owned();
        let locations: Vec<Location> = request.locations.try_into()?;
        Ok(RouteResponse {
            id: request.id,
            profile,
            units: request.units,
            locations,
            trip_summary: Summary {
                duration: Duration::from_secs(123),
                length: 456,
                bounds: BoundingBox::VOID,
            },
            legs: Vec::new(),
        })
    }
}
