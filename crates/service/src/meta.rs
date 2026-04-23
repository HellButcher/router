use router_storage::data::{
    attrib::{HighwayClass, NodeFlags, WayFlags},
    edge::{Edge, EdgeFlags},
    node::Node,
    way::{NO_EDGE, Way},
};

use crate::Service;

// ── WayTraversal ──────────────────────────────────────────────────────────────

/// All nodes and edges belonging to one OSM way, in traversal order.
pub struct WayTraversal {
    pub nodes: Vec<NodeMeta>,
    pub edges: Vec<EdgeMeta>,
    /// Storage node indices parallel to `nodes`, used for position lookup.
    node_storage_indices: Vec<usize>,
}

impl WayTraversal {
    /// Returns the position of a node in `nodes` by its storage index.
    pub fn node_pos(&self, storage_idx: usize) -> Option<usize> {
        self.node_storage_indices
            .iter()
            .position(|&i| i == storage_idx)
    }
}

impl Service {
    /// Collect all ordered nodes and edges of the OSM way identified by
    /// `way_idx`, starting from `way.first_edge_idx()`.
    pub fn collect_way(&self, way_idx: usize, way: &Way) -> WayTraversal {
        let mut nodes: Vec<NodeMeta> = Vec::new();
        let mut edges: Vec<EdgeMeta> = Vec::new();
        let mut node_storage_indices: Vec<usize> = Vec::new();

        let Ok(mut edge) = self.edges.get(way.first_edge_idx()) else {
            return WayTraversal {
                nodes,
                edges,
                node_storage_indices,
            };
        };

        // Seed with the first from-node
        let start_idx = edge.from_node_idx();
        if let Ok(n) = self.nodes.get(start_idx) {
            node_storage_indices.push(start_idx);
            nodes.push(NodeMeta::from(&n));
        }

        loop {
            let to_idx = edge.to_node_idx();
            // Full circle: stop before adding a duplicate
            if node_storage_indices.contains(&to_idx) {
                edges.push(EdgeMeta::from(&edge, None, None));
                break;
            }
            if let Ok(n) = self.nodes.get(to_idx) {
                node_storage_indices.push(to_idx);
                nodes.push(NodeMeta::from(&n));
            }
            edges.push(EdgeMeta::from(&edge, None, None));

            // Advance: find the outbound edge from to_idx that belongs to this way.
            // next_edge() is the *from-node* adjacency list, so we must start from
            // the to_node's outbound list, not from the current edge's next_edge.
            let Ok(to_node) = self.nodes.get(to_idx) else {
                break;
            };
            let mut next_idx = to_node.first_edge_idx_outbound();
            let mut found = false;
            while next_idx != NO_EDGE as usize {
                let Ok(next) = self.edges.get(next_idx) else {
                    break;
                };
                if next.way_idx() == way_idx {
                    edge = next;
                    found = true;
                    break;
                }
                next_idx = next.next_edge();
            }
            if !found {
                break;
            }
        }

        WayTraversal {
            nodes,
            edges,
            node_storage_indices,
        }
    }
}

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_zero_u8(n: &u8) -> bool {
    *n == 0
}

fn is_zero_u16(n: &u16) -> bool {
    *n == 0
}

// ── NodeMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct NodeMeta {
    pub id: i64,
    pub lat: f32,
    pub lon: f32,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_motor: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_hgv: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_bicycle: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_foot: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub traffic_signals: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub toll: bool,
}

impl NodeMeta {
    pub fn from(node: &Node) -> Self {
        Self {
            id: node.id.0,
            lat: node.pos.lat,
            lon: node.pos.lon,
            no_motor: node.flags.contains(NodeFlags::NO_MOTOR),
            no_hgv: node.flags.contains(NodeFlags::NO_HGV),
            no_bicycle: node.flags.contains(NodeFlags::NO_BICYCLE),
            no_foot: node.flags.contains(NodeFlags::NO_FOOT),
            traffic_signals: node.flags.contains(NodeFlags::TRAFFIC_SIGNALS),
            toll: node.flags.contains(NodeFlags::TOLL),
        }
    }
}

// ── WayMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct WayMeta {
    /// OSM way ID.
    pub id: i64,
    pub highway: String,
    pub surface_quality: String,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub oneway: bool,
    /// Per-direction access flags (from the representative edge).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub toll: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub tunnel: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub bridge: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub ferry: bool,
    /// Maximum height clearance in centimetres; 0 = no restriction.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero_u16"))]
    pub max_height_cm: u16,
    /// Maximum width clearance in centimetres; 0 = no restriction.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero_u16"))]
    pub max_width_cm: u16,
    /// Maximum allowed weight in kilograms; 0 = no restriction.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero_u16"))]
    pub max_weight_kg: u16,
}

impl WayMeta {
    /// Build `WayMeta` from a representative `way`.
    pub fn from(way: &Way) -> Self {
        Self {
            id: way.id.0,
            highway: format!("{:?}", way.highway),
            surface_quality: format!("{:?}", way.surface_quality),
            oneway: way.flags.contains(WayFlags::ONEWAY),
            toll: way.flags.contains(WayFlags::TOLL),
            tunnel: way.flags.contains(WayFlags::TUNNEL),
            bridge: way.flags.contains(WayFlags::BRIDGE),
            ferry: way.highway == HighwayClass::Ferry,
            // Convert internal units to human-readable: dm → cm (×10), 250 kg units → kg (×250)
            max_height_cm: way.dim.max_height_dm as u16 * 10,
            max_width_cm: way.dim.max_width_dm as u16 * 10,
            max_weight_kg: way.dim.max_weight_250kg as u16 * 250,
        }
    }
}

// ── EdgeMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EdgeMeta {
    /// Max speed from OSM tag in km/h; 0 means use highway-class default.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero_u8"))]
    pub max_speed: u8,
    /// ISO 3166-1 alpha-2 country code, or `null` if unknown.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub country_id: Option<String>,
    /// Length of the representative edge segment in metres.
    pub dist_m: u16,
    /// Per-direction access flags (from the representative edge).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_motor: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_hgv: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_bicycle: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_foot: bool,
    /// Index of the from-node of this edge within the accompanying `node_meta` array.
    /// Only present when `node_meta` is populated.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub from_node_idx: Option<usize>,
    /// Index of the to-node of this edge within the accompanying `node_meta` array.
    /// Only present when `node_meta` is populated.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub to_node_idx: Option<usize>,
}

impl EdgeMeta {
    pub fn from(edge: &Edge, from_node_idx: Option<usize>, to_node_idx: Option<usize>) -> Self {
        Self {
            max_speed: edge.max_speed,
            country_id: edge.country_id.to_iso2().map(str::to_owned),
            dist_m: edge.dist_m,
            no_motor: edge.flags.contains(EdgeFlags::NO_MOTOR),
            no_hgv: edge.flags.contains(EdgeFlags::NO_HGV),
            no_bicycle: edge.flags.contains(EdgeFlags::NO_BICYCLE),
            no_foot: edge.flags.contains(EdgeFlags::NO_FOOT),
            from_node_idx,
            to_node_idx,
        }
    }
}
