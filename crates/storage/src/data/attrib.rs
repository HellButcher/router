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
    }
}

unsafe impl bytemuck::Zeroable for WayFlags {}
unsafe impl bytemuck::Pod for WayFlags {}

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
}

unsafe impl bytemuck::Zeroable for HighwayClass {}
unsafe impl bytemuck::Pod for HighwayClass {}
