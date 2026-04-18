use router_storage::{
    data::{
        attrib::{HighwayClass, NodeFlags, WayFlags},
        edge::{Edge, EdgeFlags},
        node::Node,
        way::Way,
    },
    tablefile::TableFile,
};

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

// ── EdgeMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EdgeMeta {
    /// OSM way ID.
    pub id: i64,
    pub highway: String,
    /// Max speed from OSM tag in km/h; 0 means use highway-class default.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero_u8"))]
    pub max_speed: u8,
    pub surface_quality: String,
    /// ISO 3166-1 alpha-2 country code, or `null` if unknown.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub country_id: Option<String>,
    /// Length of the representative edge segment in metres.
    pub dist_m: u16,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub oneway: bool,
    /// Per-direction access flags (from the representative edge).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_motor: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_hgv: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_bicycle: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_foot: bool,
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
    pub from_node: NodeMeta,
    pub to_node: NodeMeta,
}

impl EdgeMeta {
    /// Build `EdgeMeta` from a representative `edge` and its parent `way`.
    ///
    /// `edge` should be `edges.get(way.first_edge_idx)`.
    pub fn from(edge: &Edge, way: &Way, nodes: &TableFile<Node>) -> std::io::Result<Self> {
        let from_node = nodes.get(edge.from_node_idx as usize)?;
        let to_node = nodes.get(edge.to_node_idx as usize)?;
        Ok(Self {
            id: way.id.0,
            highway: format!("{:?}", way.highway),
            max_speed: way.max_speed,
            surface_quality: format!("{:?}", way.surface_quality),
            country_id: edge.country_id.to_iso2().map(str::to_owned),
            dist_m: edge.dist_m,
            oneway: way.flags.contains(WayFlags::ONEWAY),
            no_motor: edge.flags.contains(EdgeFlags::NO_MOTOR),
            no_hgv: edge.flags.contains(EdgeFlags::NO_HGV),
            no_bicycle: edge.flags.contains(EdgeFlags::NO_BICYCLE),
            no_foot: edge.flags.contains(EdgeFlags::NO_FOOT),
            toll: way.flags.contains(WayFlags::TOLL),
            tunnel: way.flags.contains(WayFlags::TUNNEL),
            bridge: way.flags.contains(WayFlags::BRIDGE),
            ferry: way.highway == HighwayClass::Ferry,
            // Convert internal units to human-readable: dm → cm (×10), 250 kg units → kg (×250)
            max_height_cm: way.dim.max_height_dm as u16 * 10,
            max_width_cm: way.dim.max_width_dm as u16 * 10,
            max_weight_kg: way.dim.max_weight_250kg as u16 * 250,
            from_node: NodeMeta::from(&from_node),
            to_node: NodeMeta::from(&to_node),
        })
    }
}
