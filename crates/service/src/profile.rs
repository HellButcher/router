use router_storage::data::attrib::{HighwayClass, SURFACE_QUALITY_COUNT};

/// Vehicle type used for access-restriction checks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VehicleType {
    Car = 0,
    Hgv = 1,
    Bicycle = 2,
    Foot = 3,
}

impl VehicleType {
    /// Number of `VehicleType` variants.
    pub const COUNT: usize = 4;
    pub fn name(self) -> &'static str {
        match self {
            Self::Car => "car",
            Self::Hgv => "hgv",
            Self::Bicycle => "bike",
            Self::Foot => "foot",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        match s {
            "car" => Some(Self::Car),
            "hgv" => Some(Self::Hgv),
            "bike" => Some(Self::Bicycle),
            "foot" => Some(Self::Foot),
            _ => None,
        }
    }
}

/// A routing profile: vehicle characteristics and speed table.
pub struct Profile {
    pub name: &'static str,
    pub vehicle_type: VehicleType,
    /// Maximum speed the vehicle is capable of, in km/h.
    pub max_speed_kmh: u8,
    /// Default speed per highway class in km/h (index = `HighwayClass as u8`).
    /// A value of 0 means the highway class is forbidden for this vehicle.
    speed_table: [u8; 22],
    /// Speed multiplier (0–100 %) per surface quality tier (index = `SurfaceQuality as u8`).
    /// 0 means the surface is impassable for this vehicle.
    pub surface_pct: [u8; SURFACE_QUALITY_COUNT],
}

impl Profile {
    /// Returns the default speed (km/h) for the given highway class.
    #[inline]
    pub fn default_speed(&self, highway: HighwayClass) -> u8 {
        self.speed_table[highway as usize]
    }

    // Speed table index layout (22 entries):
    //  0=Unknown  1=Mway  2=Trunk  3=Pri  4=Sec  5=Ter
    //  6=MwayLnk  7=TLnk  8=PLnk  9=SLnk 10=TLnk
    // 11=Uncl 12=Res 13=Liv 14=Svc 15=Trk 16=Rd
    // 17=Ped 18=Ftw 19=Cyc 20=Path 21=Brdwy

    pub const CAR: Profile = Profile {
        name: "car",
        vehicle_type: VehicleType::Car,
        max_speed_kmh: 130,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy
            0, 110, 90, 80, 60, 50, 90, 70, 60, 50, 40, 50, 30, 10, 20, 20, 40, 0, 0, 0, 0, 0,
        ],
        surface_pct: [100, 100, 100, 80, 60, 40, 20, 0],
    };

    pub const HGV: Profile = Profile {
        name: "hgv",
        vehicle_type: VehicleType::Hgv,
        max_speed_kmh: 90,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy
            0, 80, 80, 70, 60, 50, 70, 60, 60, 50, 40, 50, 30, 10, 20, 20, 40, 0, 0, 0, 0, 0,
        ],
        surface_pct: [100, 100, 100, 85, 60, 30, 0, 0],
    };

    pub const BIKE: Profile = Profile {
        name: "bike",
        vehicle_type: VehicleType::Bicycle,
        max_speed_kmh: 25,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy
            0, 0, 0, 15, 18, 18, 0, 0, 15, 18, 18, 16, 16, 10, 12, 8, 16, 6, 6, 20, 10, 4,
        ],
        surface_pct: [100, 100, 95, 85, 70, 50, 30, 0],
    };

    pub const FOOT: Profile = Profile {
        name: "foot",
        vehicle_type: VehicleType::Foot,
        max_speed_kmh: 6,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy
            0, 0, 0, 5, 5, 5, 0, 0, 5, 5, 5, 5, 5, 4, 4, 3, 5, 5, 5, 4, 5, 4,
        ],
        surface_pct: [100, 100, 100, 95, 85, 75, 60, 0],
    };
}

pub static PROFILES: &[&Profile] = &[&Profile::CAR, &Profile::HGV, &Profile::BIKE, &Profile::FOOT];
