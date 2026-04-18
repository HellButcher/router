/// Physical dimension restriction on a way (height, width, weight).
/// Encoding: 0 in any field means "no restriction".
/// - `max_height_dm`: max height in decimetres (0.1 m). E.g. 40 = 4.0 m.
/// - `max_width_dm`: max width in decimetres (0.1 m). E.g. 25 = 2.5 m.
/// - `max_weight_250kg`: max weight in units of 250 kg. E.g. 30 = 7.5 t.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct DimRestriction {
    pub max_height_dm: u8,
    pub max_width_dm: u8,
    pub max_weight_250kg: u8,
}

impl DimRestriction {
    pub const NONE: Self = Self {
        max_height_dm: 0,
        max_width_dm: 0,
        max_weight_250kg: 0,
    };

    pub fn is_none(self) -> bool {
        self == Self::NONE
    }

    /// Returns true if a vehicle with the given dimensions is blocked by this restriction.
    /// Pass 0 for any dimension that should not be checked.
    pub fn blocks_vehicle(self, height_dm: u8, width_dm: u8, weight_250kg: u8) -> bool {
        (self.max_height_dm > 0 && height_dm > 0 && height_dm > self.max_height_dm)
            || (self.max_width_dm > 0 && width_dm > 0 && width_dm > self.max_width_dm)
            || (self.max_weight_250kg > 0
                && weight_250kg > 0
                && weight_250kg > self.max_weight_250kg)
    }
}
