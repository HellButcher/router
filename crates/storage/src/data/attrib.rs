use bitflags::bitflags;

bitflags! {
    /// Access and direction restrictions encoded per-way.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct WayFlags: u8 {
        /// Way is only traversable from → to (forward direction).
        const ONEWAY         = 0b0000_0001;
        /// Motor vehicles not allowed.
        const NO_MOTOR       = 0b0000_0100;
        /// HGV (heavy goods vehicles) not allowed.
        const NO_HGV         = 0b0000_1000;
        /// Bicycles not allowed.
        const NO_BICYCLE     = 0b0001_0000;
        /// Pedestrians not allowed.
        const NO_FOOT        = 0b0010_0000;
        /// Toll road — passing this way incurs a toll charge.
        const TOLL           = 0b0000_0010;
        /// Way passes through a tunnel.
        const TUNNEL         = 0b0100_0000;
        /// Way is on a bridge.
        const BRIDGE         = 0b1000_0000;
    }
}

unsafe impl bytemuck::Zeroable for WayFlags {}
unsafe impl bytemuck::Pod for WayFlags {}

bitflags! {
    /// Access restrictions and routing hints encoded per-node.
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
    #[repr(transparent)]
    pub struct NodeFlags: u8 {
        /// Motor vehicles cannot pass this node (barrier).
        const NO_MOTOR        = 0b0000_0001;
        /// HGV cannot pass this node.
        const NO_HGV          = 0b0000_0010;
        /// Bicycles cannot pass this node (barrier).
        const NO_BICYCLE      = 0b0000_0100;
        /// Pedestrians cannot pass this node.
        const NO_FOOT         = 0b0000_1000;
        /// Node has traffic signals — routing may add an intersection penalty.
        const TRAFFIC_SIGNALS = 0b0001_0000;
        /// Toll booth at this node.
        const TOLL            = 0b0010_0000;
    }
}

unsafe impl bytemuck::Zeroable for NodeFlags {}
unsafe impl bytemuck::Pod for NodeFlags {}

/// Highway class used for default speed lookups.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum HighwayClass {
    #[default]
    Unknown = 0,
    Motorway = 1,
    Trunk = 2,
    Primary = 3,
    Secondary = 4,
    Tertiary = 5,
    MotorwayLink = 6,
    TrunkLink = 7,
    PrimaryLink = 8,
    SecondaryLink = 9,
    TertiaryLink = 10,
    Unclassified = 11,
    Residential = 12,
    LivingStreet = 13,
    Service = 14,
    Track = 15,
    Road = 16,
    Pedestrian = 17,
    Footway = 18,
    Cycleway = 19,
    Path = 20,
    Bridleway = 21,
    Ferry = 22,
}

unsafe impl bytemuck::Zeroable for HighwayClass {}
unsafe impl bytemuck::Pod for HighwayClass {}

impl HighwayClass {
    /// Number of `HighwayClass` variants.
    pub const COUNT: usize = 23;

    pub fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Motorway => "motorway",
            Self::Trunk => "trunk",
            Self::Primary => "primary",
            Self::Secondary => "secondary",
            Self::Tertiary => "tertiary",
            Self::MotorwayLink => "motorway_link",
            Self::TrunkLink => "trunk_link",
            Self::PrimaryLink => "primary_link",
            Self::SecondaryLink => "secondary_link",
            Self::TertiaryLink => "tertiary_link",
            Self::Unclassified => "unclassified",
            Self::Residential => "residential",
            Self::LivingStreet => "living_street",
            Self::Service => "service",
            Self::Track => "track",
            Self::Road => "road",
            Self::Pedestrian => "pedestrian",
            Self::Footway => "footway",
            Self::Cycleway => "cycleway",
            Self::Path => "path",
            Self::Bridleway => "bridleway",
            Self::Ferry => "ferry",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "unknown" => Some(Self::Unknown),
            "motorway" => Some(Self::Motorway),
            "trunk" => Some(Self::Trunk),
            "primary" => Some(Self::Primary),
            "secondary" => Some(Self::Secondary),
            "tertiary" => Some(Self::Tertiary),
            "motorway_link" => Some(Self::MotorwayLink),
            "trunk_link" => Some(Self::TrunkLink),
            "primary_link" => Some(Self::PrimaryLink),
            "secondary_link" => Some(Self::SecondaryLink),
            "tertiary_link" => Some(Self::TertiaryLink),
            "unclassified" => Some(Self::Unclassified),
            "residential" => Some(Self::Residential),
            "living_street" => Some(Self::LivingStreet),
            "service" => Some(Self::Service),
            "track" => Some(Self::Track),
            "road" => Some(Self::Road),
            "pedestrian" => Some(Self::Pedestrian),
            "footway" => Some(Self::Footway),
            "cycleway" => Some(Self::Cycleway),
            "path" => Some(Self::Path),
            "bridleway" => Some(Self::Bridleway),
            "ferry" => Some(Self::Ferry),
            _ => None,
        }
    }
}

/// Road surface quality tier, derived from OSM `surface`, `smoothness`, and `tracktype` tags.
/// Stored as one byte per way; used by profiles to compute a per-vehicle speed penalty.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u8)]
pub enum SurfaceQuality {
    /// No surface tag — profile default speed applied without penalty.
    #[default]
    Unknown = 0,
    /// Smooth asphalt / concrete; smoothness=excellent or good.
    Excellent = 1,
    /// Paving stones, sett; tracktype=grade1.
    Good = 2,
    /// Cobblestone, compacted gravel; tracktype=grade2; smoothness=intermediate.
    Intermediate = 3,
    /// Fine gravel, shells; tracktype=grade3; smoothness=bad.
    Bad = 4,
    /// Gravel, ground, dirt; tracktype=grade4; smoothness=very_bad.
    VeryBad = 5,
    /// Grass, sand; tracktype=grade5; smoothness=horrible or very_horrible.
    Horrible = 6,
    /// Ice, snow; smoothness=impassable.
    Impassable = 7,
}

unsafe impl bytemuck::Zeroable for SurfaceQuality {}
// SAFETY: SurfaceQuality is repr(u8). Values 8–255 are never written by this codebase;
// any such byte read from disk will be treated as Impassable (safe worst-case behaviour).
unsafe impl bytemuck::Pod for SurfaceQuality {}

/// Number of `SurfaceQuality` variants — length of `surface_pct` arrays in profiles.
pub const SURFACE_QUALITY_COUNT: usize = 8;
