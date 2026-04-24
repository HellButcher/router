#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    meta::{EdgeMeta, NodeMeta, WayMeta},
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
    pub node_id: Option<Vec<i64>>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub way_id: Option<i64>,
}

#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct InspectResponse {
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub node: Vec<NodeMeta>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub way: Option<WayMeta>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Vec::is_empty"))]
    pub edge: Vec<EdgeMeta>,
}

// ── service impl ──────────────────────────────────────────────────────────────

impl Service {
    pub async fn inspect(&self, request: InspectRequest) -> Result<InspectResponse> {
        match (request.node_id, request.way_id) {
            (Some(ids), None) => {
                let mut nodes = Vec::with_capacity(ids.len());
                for id in ids {
                    let (_, node) = self
                        .nodes
                        .find(id as u64)?
                        .ok_or(Error::NotFound("NodeId", id))?;
                    nodes.push(NodeMeta::from(&node));
                }
                Ok(InspectResponse {
                    node: nodes,
                    way: None,
                    edge: vec![],
                })
            }
            (None, Some(id)) => {
                let (way_idx, way) = self
                    .ways
                    .find(id as u64)?
                    .ok_or_else(|| Error::InvalidRequest(format!("way {id} not found")))?;
                let wt = self.collect_way(way_idx, &way);
                Ok(InspectResponse {
                    node: wt.nodes,
                    way: Some(WayMeta::from(&way)),
                    edge: wt.edges,
                })
            }
            _ => Err(Error::InvalidRequest(
                "exactly one of node_id or way_id must be set".into(),
            )),
        }
    }
}
