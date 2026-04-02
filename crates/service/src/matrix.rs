#[cfg(feature="serde")]
use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::{
    common::{Location, Locations, Unit},
    route::Summary, Service,
};

#[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature="serde", serde(untagged))]
pub enum MatrixRequestLocations {
    Symetric { locations: Locations },
    Asymetric { from: Locations, to: Locations },
}

#[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
pub struct MatrixRequest {
    pub profile: Option<String>,
    #[cfg_attr(feature="serde", serde(default))]
    pub units: Unit,
    #[cfg_attr(feature="serde", serde(flatten))]
    pub locations: MatrixRequestLocations,

    #[cfg_attr(feature="serde", serde(default, skip_serializing_if = "Vec::is_empty"))]
    pub pairs: Vec<(usize, usize)>,

    #[cfg_attr(feature="serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub id: Option<String>,
}

#[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
pub struct MatrixResponseEntry {
    from: usize,
    to: usize,
    #[cfg_attr(feature="serde", serde(flatten))]
    summary: Summary,
}

#[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
pub struct MatrixResponse {
    pub profile: String,
    #[cfg_attr(feature="serde", serde(default))]
    pub units: Unit,
    pub from: Vec<Location>,
    pub to: Vec<Location>,

    pub result: Vec<MatrixResponseEntry>,

    #[cfg_attr(feature="serde", serde(default, skip_serializing_if = "Option::is_none"))]
    pub id: Option<String>,
}

impl Service {
    pub async fn calculate_matrix(&self, request: MatrixRequest) -> Result<MatrixResponse> {
        let profile = self
            .get_opt_profile(request.profile.as_deref())?
            .to_owned();
        let (from, to) = match request.locations {
            MatrixRequestLocations::Symetric { locations } => {
                let locations: Vec<Location> = locations.try_into()?;
                (locations.clone(), locations)
            }
            MatrixRequestLocations::Asymetric { from, to } => (from.try_into()?, to.try_into()?),
        };
        Ok(MatrixResponse {
            id: request.id,
            profile,
            units: request.units,
            from,
            to,
            result: Vec::new(),
        })
    }
}
