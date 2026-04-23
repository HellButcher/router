use router_storage::data::attrib::{HighwayClass, SURFACE_QUALITY_COUNT};

/// Physical dimensions of a vehicle, used to check against way dimension restrictions.
/// A value of 0 in any field means "this vehicle has no declared dimension" (skip check).
#[derive(Copy, Clone, Debug, Default)]
pub struct VehicleDim {
    /// Vehicle height in decimetres (0.1 m). E.g. 40 = 4.0 m.
    pub height_dm: u8,
    /// Vehicle width in decimetres (0.1 m). E.g. 26 = 2.6 m.
    pub width_dm: u8,
    /// Vehicle length in decimetres (0.1 m). E.g. 120 = 12.0 m.
    pub length_dm: u8,
    /// Vehicle weight in units of 250 kg. E.g. 160 = 40 t.
    pub weight_250kg: u8,
}

impl VehicleDim {
    pub const NONE: Self = Self {
        height_dm: 0,
        width_dm: 0,
        length_dm: 0,
        weight_250kg: 0,
    };
}

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
    speed_table: [u8; 23],
    /// Speed multiplier (0–100 %) per surface quality tier (index = `SurfaceQuality as u8`).
    /// 0 means the surface is impassable for this vehicle.
    pub surface_pct: [u8; SURFACE_QUALITY_COUNT],
    /// Physical dimensions of the vehicle for dimension-restriction checks.
    /// Fields set to 0 are not checked against way restrictions.
    pub vehicle_dim: VehicleDim,
    /// Extra travel-time cost in milliseconds added when passing through a node with
    /// `TRAFFIC_SIGNALS`. Models the average delay at signalised intersections.
    pub traffic_signal_penalty_ms: u32,
    /// Extra travel-time cost in milliseconds added when traversing a tolled way.
    /// Set to 0 to accept tolls without penalty.
    pub toll_penalty_ms: u32,
    /// Extra travel-time cost in milliseconds added when boarding a ferry.
    /// Models waiting time at the terminal.
    pub ferry_penalty_ms: u32,
}

impl Profile {
    /// Returns the default speed (km/h) for the given highway class.
    #[inline]
    pub fn default_speed(&self, highway: HighwayClass) -> u8 {
        self.speed_table[highway as usize]
    }

    // Speed table index layout (23 entries):
    //  0=Unknown  1=Mway  2=Trunk  3=Pri  4=Sec  5=Ter
    //  6=MwayLnk  7=TLnk  8=PLnk  9=SLnk 10=TLnk
    // 11=Uncl 12=Res 13=Liv 14=Svc 15=Trk 16=Rd
    // 17=Ped 18=Ftw 19=Cyc 20=Path 21=Brdwy 22=Ferry

    pub const CAR: Profile = Profile {
        name: "car",
        vehicle_type: VehicleType::Car,
        max_speed_kmh: 130,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy Ferry
            0, 110, 90, 80, 60, 50, 90, 70, 60, 50, 40, 50, 30, 10, 20, 20, 40, 0, 0, 0, 0, 0, 5,
        ],
        surface_pct: [100, 100, 100, 80, 60, 40, 20, 0],
        // Typical passenger car: width ~1.9 m.
        vehicle_dim: VehicleDim {
            height_dm: 0,
            width_dm: 19,
            length_dm: 0,
            weight_250kg: 0,
        },
        traffic_signal_penalty_ms: 15_000,
        toll_penalty_ms: 5 * 60_000,
        ferry_penalty_ms: 30 * 60_000,
    };

    pub const HGV: Profile = Profile {
        name: "hgv",
        vehicle_type: VehicleType::Hgv,
        max_speed_kmh: 90,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy Ferry
            0, 80, 80, 70, 60, 50, 70, 60, 60, 50, 40, 50, 30, 10, 20, 20, 40, 0, 0, 0, 0, 0, 5,
        ],
        surface_pct: [100, 100, 100, 85, 60, 30, 0, 0],
        // Typical European HGV: height 4.0 m, width 2.55 m, gross weight 40 t.
        vehicle_dim: VehicleDim {
            height_dm: 40,
            width_dm: 26,
            length_dm: 0,
            weight_250kg: 160,
        },
        traffic_signal_penalty_ms: 20_000,
        toll_penalty_ms: 5 * 60_000,
        ferry_penalty_ms: 30 * 60_000,
    };

    pub const BIKE: Profile = Profile {
        name: "bike",
        vehicle_type: VehicleType::Bicycle,
        max_speed_kmh: 25,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy Ferry
            0, 0, 0, 15, 18, 18, 0, 0, 15, 18, 18, 16, 16, 10, 12, 8, 16, 6, 6, 20, 10, 4, 5,
        ],
        surface_pct: [100, 100, 95, 85, 70, 50, 30, 0],
        // Typical bicycle: width ~0.6 m (handlebars).
        vehicle_dim: VehicleDim {
            height_dm: 0,
            width_dm: 6,
            length_dm: 0,
            weight_250kg: 0,
        },
        traffic_signal_penalty_ms: 10_000,
        toll_penalty_ms: 0,
        ferry_penalty_ms: 30 * 60_000,
    };

    pub const FOOT: Profile = Profile {
        name: "foot",
        vehicle_type: VehicleType::Foot,
        max_speed_kmh: 6,
        speed_table: [
            //  Unk  Mwy  Trk  Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ftw  Cyc  Path Brdwy Ferry
            0, 0, 0, 5, 5, 5, 0, 0, 5, 5, 5, 5, 5, 4, 4, 3, 5, 5, 5, 4, 5, 4, 5,
        ],
        surface_pct: [100, 100, 100, 95, 85, 75, 60, 0],
        vehicle_dim: VehicleDim::NONE,
        traffic_signal_penalty_ms: 5_000,
        toll_penalty_ms: 0,
        ferry_penalty_ms: 20 * 60_000,
    };
}

pub static PROFILES: &[&Profile] = &[&Profile::CAR, &Profile::HGV, &Profile::BIKE, &Profile::FOOT];
