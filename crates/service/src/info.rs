use serde::{Deserialize, Serialize};

use super::Service;

/// Status of the service
#[derive(Copy, Clone, Debug, PartialEq, Default, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum ServiceStatus {
    #[cfg_attr(feature = "serde", serde(rename = "ok", alias = "alive"))]
    #[default]
    Ok,
}

/// Response for the [`Service::info`] method, containing information about the service, such as the available profiles and the version.
///
/// See: [`Service::info`]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InfoResponse {
    pub status: ServiceStatus,
    pub version: &'static str,
    pub profiles: Vec<String>,
}

impl Service {
    /// Get information about the service, such as the available profiles and the version.
    pub fn info(&self) -> InfoResponse {
        InfoResponse {
            status: ServiceStatus::Ok,
            profiles: self.profile_names().map(str::to_owned).collect(),
            version: env!("CARGO_PKG_VERSION"),
        }
    }
}
