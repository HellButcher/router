use router_storage::data::attrib::HighwayClass;

/// Vehicle type used for access-restriction checks.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum VehicleType {
    Car,
    Hgv,
}

/// A routing profile: vehicle characteristics and speed table.
pub struct Profile {
    pub name: &'static str,
    pub vehicle_type: VehicleType,
    /// Maximum speed the vehicle is capable of, in km/h.
    pub max_speed_kmh: u8,
    /// Default speed per highway class in km/h (index = `HighwayClass as u8`).
    speed_table: [u8; 21],
}

impl Profile {
    /// Returns the default speed (km/h) for the given highway class.
    #[inline]
    pub fn default_speed(&self, highway: HighwayClass) -> u8 {
        self.speed_table[highway as usize]
    }

    pub const CAR: Profile = Profile {
        name: "car",
        vehicle_type: VehicleType::Car,
        max_speed_kmh: 130,
        //                          0    1    2    3    4    5    6    7    8    9   10   11   12   13   14   15   16   17   18   19   20
        // index =              Unknown  Mway Trunk Pri  Sec  Ter  MLnk TLnk PLnk SLnk TLnk Uncl Res  Liv  Svc  Trk  Rd   Ped  Ft   Cyc  Path
        speed_table: [
            0, 110, 90, 80, 60, 50, 90, 70, 60, 50, 40, 50, 30, 10, 20, 20, 40, 0, 0, 0, 0,
        ],
    };

    pub const HGV: Profile = Profile {
        name: "hgv",
        vehicle_type: VehicleType::Hgv,
        max_speed_kmh: 90,
        speed_table: [
            0, 80, 80, 70, 60, 50, 70, 60, 60, 50, 40, 50, 30, 10, 20, 20, 40, 0, 0, 0, 0,
        ],
    };
}

pub static PROFILES: &[&Profile] = &[&Profile::CAR, &Profile::HGV];
