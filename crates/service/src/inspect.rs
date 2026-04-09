use router_storage::data::{node::NodeId, way::WayId};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    meta::{NodeMeta, WayMeta},
};

use super::Service;

// ── request / response ────────────────────────────────────────────────────────

/// Look up meta information for a node or way by its OSM ID.
///
/// Exactly one of `node_id` or `way_id` must be set.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InspectRequest {
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub node_id: Option<i64>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub way_id: Option<i64>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InspectResponse {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub node: Option<NodeMeta>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub way: Option<WayMeta>,
}

// ── service impl ──────────────────────────────────────────────────────────────

impl Service {
    pub async fn inspect(&self, request: InspectRequest) -> Result<InspectResponse> {
        match (request.node_id, request.way_id) {
            (Some(id), None) => {
                let result = self.nodes.find(&NodeId(id))?;
                let node = result
                    .map(|(_, n)| NodeMeta::from(&n))
                    .ok_or_else(|| Error::InvalidRequest(format!("node {id} not found")))?;
                Ok(InspectResponse {
                    node: Some(node),
                    way: None,
                })
            }
            (None, Some(id)) => {
                let result = self.ways.find(&WayId(id))?;
                let way = result
                    .map(|(_, w)| WayMeta::from(&w, &self.nodes, &self.dim_table))
                    .ok_or_else(|| Error::InvalidRequest(format!("way {id} not found")))?
                    .map_err(Error::StorageError)?;
                Ok(InspectResponse {
                    node: None,
                    way: Some(way),
                })
            }
            _ => Err(Error::InvalidRequest(
                "exactly one of node_id or way_id must be set".into(),
            )),
        }
    }
}
