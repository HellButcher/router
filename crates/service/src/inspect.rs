use router_storage::data::{
    node::NodeId,
    way::{NO_EDGE, WayId},
};

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
                        .find(&NodeId(id))?
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
                    .find(&WayId(id))?
                    .ok_or_else(|| Error::InvalidRequest(format!("way {id} not found")))?;
                // Retrieve the stripe of edges from first_edge_idx.
                let mut visited = std::collections::HashSet::new();
                let mut edges = Vec::new();
                let mut nodes = Vec::new();
                let mut edge = self
                    .edges
                    .get(way.first_edge_idx())
                    .map_err(Error::StorageError)?;
                let node_index = edge.from_node_idx();
                let node = self.nodes.get(node_index).map_err(Error::StorageError)?;
                visited.insert(node_index);
                nodes.push(NodeMeta::from(&node));
                loop {
                    // add the end of the edge
                    let node_index = edge.to_node_idx();
                    let node = self.nodes.get(node_index).map_err(Error::StorageError)?;
                    nodes.push(NodeMeta::from(&node));
                    edges.push(EdgeMeta::from(&edge));
                    if visited.insert(edge.to_node_idx()) {
                        break; // we've come full circle, so stop
                    }

                    // follow the way
                    let mut next_edge_idx = edge.next_edge();
                    let mut found = false;
                    while next_edge_idx != NO_EDGE as usize {
                        let next_edge =
                            self.edges.get(next_edge_idx).map_err(Error::StorageError)?;
                        if next_edge.way_idx() == way_idx {
                            edge = next_edge;
                            found = true;
                            break;
                        } else {
                            next_edge_idx = next_edge.next_edge();
                        }
                    }
                    if !found {
                        break; // end of stripe, so stop
                    }
                }
                let way = WayMeta::from(&way);
                Ok(InspectResponse {
                    node: vec![],
                    way: Some(way),
                    edge: edges,
                })
            }
            _ => Err(Error::InvalidRequest(
                "exactly one of node_id or way_id must be set".into(),
            )),
        }
    }
}
