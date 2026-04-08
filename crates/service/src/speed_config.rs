use std::path::Path;

use router_storage::data::attrib::HighwayClass;
use router_types::country::CountryId;

use crate::profile::VehicleType;

const STRIDE: usize = VehicleType::COUNT * HighwayClass::COUNT;

/// Per-country, per-profile default speeds for highway classes.
///
/// Loaded from a TOML file at server startup. TOML structure:
///
/// ```toml
/// [country_speed.DE.car]
/// motorway = 130
/// trunk    = 100
///
/// [country_speed.DE.hgv]
/// motorway = 80
/// ```
///
/// Keys under each profile table are highway-class names as used in OSM
/// (`motorway`, `trunk`, `primary`, …). Missing entries fall back to the
/// profile's built-in speed table.
///
/// Internally stored as a flat array indexed by
/// `country_id * STRIDE + vehicle_type * HighwayClass::COUNT + highway_class`.
/// A value of 0 means "no override".
pub struct SpeedConfig {
    speeds: Box<[u8]>, // length = CountryId::COUNT * STRIDE
}

impl SpeedConfig {
    fn new_speeds() -> Box<[u8]> {
        vec![0u8; CountryId::COUNT * STRIDE].into_boxed_slice()
    }

    /// Load from a TOML file.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content =
            std::fs::read_to_string(path).map_err(|e| ConfigError::Io(path.to_path_buf(), e))?;
        let value: toml::Value = content
            .parse()
            .map_err(|e| ConfigError::Parse(path.to_path_buf(), e))?;
        Self::from_toml(&value).map_err(|e| ConfigError::Schema(path.to_path_buf(), e))
    }

    fn from_toml(value: &toml::Value) -> Result<Self, String> {
        let mut speeds = Self::new_speeds();

        let Some(country_speed) = value.get("country_speed").and_then(|v| v.as_table()) else {
            return Ok(SpeedConfig { speeds });
        };

        for (country_iso, profiles) in country_speed {
            let country_id = CountryId::from_iso2(&country_iso.to_uppercase());
            if country_id.is_unknown() {
                return Err(format!("unknown country code: {country_iso}"));
            }

            let profiles = profiles
                .as_table()
                .ok_or_else(|| format!("country_speed.{country_iso} must be a table"))?;

            for (profile_name, highway_speeds) in profiles {
                let vehicle_type = VehicleType::from_name(profile_name.to_lowercase().as_str())
                    .ok_or_else(|| format!("unknown profile: {profile_name}"))?;

                let highway_speeds = highway_speeds.as_table().ok_or_else(|| {
                    format!("country_speed.{country_iso}.{profile_name} must be a table")
                })?;

                for (highway_name, v) in highway_speeds {
                    let highway = HighwayClass::from_name(highway_name)
                        .ok_or_else(|| format!("unknown highway class: {highway_name}"))?;
                    let kmh = v
                        .as_integer()
                        .ok_or_else(|| format!("{highway_name} must be an integer"))?;
                    let kmh = u8::try_from(kmh)
                        .map_err(|_| format!("{highway_name} = {kmh} does not fit in u8"))?;

                    let idx = country_id.0 as usize * STRIDE
                        + vehicle_type as usize * HighwayClass::COUNT
                        + highway as usize;
                    speeds[idx] = kmh;
                }
            }
        }

        Ok(SpeedConfig { speeds })
    }

    /// Look up the default speed for a given country, vehicle type, and highway class.
    /// Returns `None` if no country-specific override is configured — caller
    /// should fall back to the profile's built-in speed table.
    #[inline]
    pub fn default_speed(
        &self,
        country_id: CountryId,
        vehicle_type: VehicleType,
        highway: HighwayClass,
    ) -> Option<u8> {
        let idx = country_id.0 as usize * STRIDE
            + vehicle_type as usize * HighwayClass::COUNT
            + highway as usize;
        let speed = self.speeds[idx];
        if speed > 0 { Some(speed) } else { None }
    }
}

impl Default for SpeedConfig {
    fn default() -> Self {
        SpeedConfig {
            speeds: Self::new_speeds(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read speed config {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),
    #[error("failed to parse speed config {0}: {1}")]
    Parse(std::path::PathBuf, toml::de::Error),
    #[error("invalid speed config structure in {0}: {1}")]
    Schema(std::path::PathBuf, String),
}
