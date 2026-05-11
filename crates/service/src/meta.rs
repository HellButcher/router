use router_storage::data::{
    attrib::{HighwayClass, WayFlags},
    edge::EdgeFlags,
    edge_node::EdgeNode,
    way::Way,
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

// ── WayMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct WayMeta {
    /// OSM way ID.
    pub id: i64,
    /// Max speed from OSM tag in km/h; 0 means use highway-class default.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero_u8"))]
    pub max_speed: u8,
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
    /// Per-direction access flags (from the representative edge).
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_motor: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_hgv: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_bicycle: bool,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub no_foot: bool,
}

impl WayMeta {
    /// Build `WayMeta` from a representative `way`.
    pub fn from(way: &Way) -> Self {
        Self {
            id: way.id.0,
            max_speed: way.max_speed,
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
            no_motor: way.access.contains(EdgeFlags::NO_MOTOR),
            no_hgv: way.access.contains(EdgeFlags::NO_HGV),
            no_bicycle: way.access.contains(EdgeFlags::NO_BICYCLE),
            no_foot: way.access.contains(EdgeFlags::NO_FOOT),
        }
    }
}

// ── EdgeMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct EdgeMeta {
    /// ISO 3166-1 alpha-2 country code, or `null` if unknown.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub country_id: Option<String>,
    /// Length of the representative edge segment in metres.
    pub dist_m: u32,
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
    pub fn from(edge: &EdgeNode, from_node_idx: Option<usize>, to_node_idx: Option<usize>) -> Self {
        Self {
            country_id: edge.country_id.to_iso2().map(str::to_owned),
            dist_m: edge.dist_m,
            from_node_idx,
            to_node_idx,
        }
    }
}
