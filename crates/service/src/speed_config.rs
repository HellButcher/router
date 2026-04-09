#[cfg(feature = "serde")]
use std::collections::HashMap;

use router_storage::data::attrib::HighwayClass;
use router_types::country::CountryId;

use crate::profile::VehicleType;

const STRIDE: usize = VehicleType::COUNT * HighwayClass::COUNT;

// Private deserialization helper.
#[cfg(feature = "serde")]
#[derive(serde::Deserialize, Default)]
struct SpeedConfigRaw {
    #[serde(default)]
    country_speed: HashMap<String, HashMap<String, HashMap<String, u8>>>,
}

/// Per-country, per-profile default speeds for highway classes (serve-time).
///
/// Deserializable (with the `serde` feature) — usable standalone or via
/// `#[serde(flatten)]` in a parent config struct:
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
/// Internally stored as a flat array indexed by
/// `country_id * STRIDE + vehicle_type * HighwayClass::COUNT + highway_class`.
/// A value of 0 means "no override".
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "SpeedConfigRaw"))]
pub struct SpeedConfig {
    speeds: Box<[u8]>, // length = CountryId::COUNT * STRIDE
}

#[cfg(feature = "serde")]
impl TryFrom<SpeedConfigRaw> for SpeedConfig {
    type Error = String;

    fn try_from(raw: SpeedConfigRaw) -> Result<Self, String> {
        struct Entry {
            /// `None` = all countries, `Some(idx)` = specific country.
            country: Option<usize>,
            /// `None` = all profiles, `Some(idx)` = specific profile.
            vehicle: Option<usize>,
            highway: HighwayClass,
            kmh: u8,
        }

        let mut entries: Vec<Entry> = Vec::new();

        for (country_pat, profiles) in &raw.country_speed {
            let country = if country_pat == "*" {
                None
            } else {
                let id = CountryId::from_iso2(&country_pat.to_uppercase());
                if id.is_unknown() {
                    return Err(format!("unknown country code: {country_pat}"));
                }
                Some(id.0 as usize)
            };

            for (profile_pat, highway_speeds) in profiles {
                let vehicle = if profile_pat == "*" {
                    None
                } else {
                    let vt = VehicleType::from_name(profile_pat.to_lowercase().as_str())
                        .ok_or_else(|| format!("unknown profile: {profile_pat}"))?;
                    Some(vt as usize)
                };

                for (highway_name, &kmh) in highway_speeds {
                    let highway = HighwayClass::from_name(highway_name)
                        .ok_or_else(|| format!("unknown highway class: {highway_name}"))?;
                    entries.push(Entry {
                        country,
                        vehicle,
                        highway,
                        kmh,
                    });
                }
            }
        }

        // Apply least-specific first (None < Some) so specific entries overwrite wildcards.
        entries.sort_unstable_by_key(|e| (e.country.is_some(), e.vehicle.is_some()));

        let all_countries: Vec<usize> = (1..CountryId::COUNT).collect();
        let all_vehicles: Vec<usize> = (0..VehicleType::COUNT).collect();

        let mut speeds = vec![0u8; CountryId::COUNT * STRIDE].into_boxed_slice();
        for e in entries {
            let countries: &[usize] = match e.country {
                None => &all_countries,
                Some(ref c) => std::slice::from_ref(c),
            };
            let vehicles: &[usize] = match e.vehicle {
                None => &all_vehicles,
                Some(ref v) => std::slice::from_ref(v),
            };
            for &c in countries {
                for &v in vehicles {
                    speeds[c * STRIDE + v * HighwayClass::COUNT + e.highway as usize] = e.kmh;
                }
            }
        }

        Ok(SpeedConfig { speeds })
    }
}

impl SpeedConfig {
    /// Look up the serve-time speed for a given country, vehicle type, and highway class.
    /// Returns `None` if no override is configured — caller should fall back to
    /// the profile's built-in speed table.
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
            speeds: vec![0u8; CountryId::COUNT * STRIDE].into_boxed_slice(),
        }
    }
}
