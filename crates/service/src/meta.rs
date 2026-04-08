use router_storage::{
    data::{attrib::WayFlags, node::Node, way::Way},
    tablefile::TableFile,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

// ── NodeMeta ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct NodeMeta {
    /// OSM node ID.
    pub id: i64,
    pub lat: f32,
    pub lon: f32,
}

impl NodeMeta {
    pub fn from(node: &Node) -> Self {
        Self {
            id: node.id.0,
            lat: node.pos.lat,
            lon: node.pos.lon,
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
    pub max_speed: u8,
    /// Surface quality tier (e.g. `"Excellent"`, `"Good"`, `"Bad"`).
    pub surface_quality: String,
    /// ISO 3166-1 alpha-2 country code, or `null` if unknown.
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    pub country_id: Option<String>,
    /// Haversine distance between the two endpoint nodes in metres.
    pub dist_m: u16,
    pub oneway: bool,
    pub no_motor: bool,
    pub no_hgv: bool,
    pub no_bicycle: bool,
    pub no_foot: bool,
    pub from_node: NodeMeta,
    pub to_node: NodeMeta,
}

impl WayMeta {
    pub fn from(way: &Way, nodes: &TableFile<Node>) -> std::io::Result<Self> {
        let from_node = nodes.get(way.from_node_idx as usize)?;
        let to_node = nodes.get(way.to_node_idx as usize)?;
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
            from_node: NodeMeta::from(&from_node),
            to_node: NodeMeta::from(&to_node),
        })
    }
}
