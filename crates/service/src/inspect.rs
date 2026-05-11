#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    meta::WayMeta,
    route::Points,
};

use super::Service;

// ── request / response ────────────────────────────────────────────────────────

/// Look up meta information for a node or way by its OSM ID.
///
/// Exactly one of `node_id` or `way_id` must be set.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InspectRequest {
    pub way_id: i64,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InspectResponse {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Points::is_empty"))]
    pub points: Points,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub way: Option<WayMeta>,
}

// ── service impl ──────────────────────────────────────────────────────────────

impl Service {
    pub async fn inspect(&self, request: InspectRequest) -> Result<InspectResponse> {
        let id = request.way_id;
        let (_, entry) = self
            .way_id_index
            .find(id as u64)?
            .ok_or_else(|| Error::InvalidRequest(format!("way {id} not found")))?;
        let way_idx = entry.idx as usize;
        let way = self.ways.get(way_idx)?;
        let geometry = self.geometry.get_range(way.geometry_range())?;
        Ok(InspectResponse {
            points: Points::encoded_from(geometry.iter()),
            way: Some(WayMeta::from(&way)),
        })
    }
}
