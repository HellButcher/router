use router_storage::{
    data::{
        attrib::{HighwayClass, NodeFlags, WayFlags},
        dim_restriction::DimRestrictionsTable,
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

fn is_zero(n: &u8) -> bool {
    *n == 0
}

// ── NodeMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct NodeMeta {
    /// OSM node ID.
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

// ── WayMeta ───────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct WayMeta {
    /// OSM way ID.
    pub id: i64,
    /// Highway classification (e.g. `"Residential"`, `"Primary"`).
    pub highway: String,
    /// Explicit max speed in km/h; 0 means use highway-class default.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero"))]
    pub max_speed: u8,
    /// Surface quality tier (e.g. `"Excellent"`, `"Good"`, `"Bad"`).
    pub surface_quality: String,
    /// ISO 3166-1 alpha-2 country code, or `null` if unknown.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub country_id: Option<String>,
    /// Haversine distance between the two endpoint nodes in metres.
    pub dist_m: u16,
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_false"))]
    pub oneway: bool,
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
    /// Max height in decimetres (0.1 m); 0 = no restriction.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero"))]
    pub max_height_dm: u8,
    /// Max width in decimetres (0.1 m); 0 = no restriction.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero"))]
    pub max_width_dm: u8,
    /// Max weight in units of 250 kg; 0 = no restriction.
    #[cfg_attr(feature = "serde", serde(default, skip_serializing_if = "is_zero"))]
    pub max_weight_250kg: u8,
    pub from_node: NodeMeta,
    pub to_node: NodeMeta,
}

impl WayMeta {
    pub fn from(
        way: &Way,
        nodes: &TableFile<Node>,
        dim_table: &DimRestrictionsTable,
    ) -> std::io::Result<Self> {
        let from_node = nodes.get(way.from_node_idx as usize)?;
        let to_node = nodes.get(way.to_node_idx as usize)?;
        let dim = dim_table.get(way.dim_restriction_idx);
        Ok(Self {
            id: way.id.0,
            highway: format!("{:?}", way.highway),
            max_speed: way.max_speed,
            surface_quality: format!("{:?}", way.surface_quality),
            country_id: way.country_id.to_iso2().map(str::to_owned),
            dist_m: way.dist_m,
            oneway: way.flags.contains(WayFlags::ONEWAY),
            no_motor: way.flags.contains(WayFlags::NO_MOTOR),
            no_hgv: way.flags.contains(WayFlags::NO_HGV),
            no_bicycle: way.flags.contains(WayFlags::NO_BICYCLE),
            no_foot: way.flags.contains(WayFlags::NO_FOOT),
            toll: way.flags.contains(WayFlags::TOLL),
            tunnel: way.flags.contains(WayFlags::TUNNEL),
            bridge: way.flags.contains(WayFlags::BRIDGE),
            ferry: way.highway == HighwayClass::Ferry,
            max_height_dm: dim.max_height_dm,
            max_width_dm: dim.max_width_dm,
            max_weight_250kg: dim.max_weight_250kg,
            from_node: NodeMeta::from(&from_node),
            to_node: NodeMeta::from(&to_node),
        })
    }
}
