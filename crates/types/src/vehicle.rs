/// Vehicle type used for access-restriction checks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VehicleType {
    Car = 0,
    Motorcycle = 1,
    Hgv = 2,
    Bicycle = 3,
    Foot = 4,
}

impl VehicleType {
    /// Number of `VehicleType` variants.
    pub const COUNT: usize = 5;

    pub fn name(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Motorcycle => "motorcycle",
            Self::Hgv => "hgv",
            Self::Bicycle => "bike",
            Self::Foot => "foot",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "car" => Some(Self::Car),
            "motorcycle" => Some(Self::Motorcycle),
            "hgv" => Some(Self::Hgv),
            "bike" => Some(Self::Bicycle),
            "foot" => Some(Self::Foot),
            _ => None,
        }
    }
}
