/// Opaque country identifier stored in way data.
/// Value 0 means unknown; values 1–N index into [`COUNTRIES`].
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub struct CountryId(pub u8);

impl CountryId {
    pub const UNKNOWN: CountryId = CountryId(0);
    /// Number of valid `CountryId` values (includes the unknown entry at index 0).
    pub const COUNT: usize = COUNTRIES.len() + 1;

    pub fn from_iso2(iso: &str) -> Self {
        CountryId(country_id_from_iso2(iso))
    }

    pub fn to_iso2(self) -> Option<&'static str> {
        country_iso_from_id(self.0)
    }

    pub fn is_unknown(self) -> bool {
        self.0 == 0
    }
}

#[cfg(feature = "bytemuck")]
unsafe impl bytemuck::Zeroable for CountryId {}
#[cfg(feature = "bytemuck")]
unsafe impl bytemuck::Pod for CountryId {}

/// Sorted list of ISO 3166-1 alpha-2 country codes.
/// The country ID stored in way data is `index + 1` (0 = unknown).
/// This list must never be reordered — only appended to.
pub const COUNTRIES: &[&str] = &[
    "AD", "AE", "AF", "AG", "AI", "AL", "AM", "AO", "AQ", "AR", "AS", "AT", "AU", "AW", "AX", "AZ",
    "BA", "BB", "BD", "BE", "BF", "BG", "BH", "BI", "BJ", "BL", "BM", "BN", "BO", "BQ", "BR", "BS",
    "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN",
    "CO", "CR", "CU", "CV", "CW", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE",
    "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "GA", "GB", "GD", "GE", "GF",
    "GG", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GU", "GW", "GY", "HK", "HM",
    "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IM", "IN", "IO", "IQ", "IR", "IS", "IT", "JE", "JM",
    "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC",
    "LI", "LK", "LR", "LS", "LT", "LU", "LV", "LY", "MA", "MC", "MD", "ME", "MF", "MG", "MH", "MK",
    "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA",
    "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG",
    "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT", "PW", "PY", "QA", "RE", "RO", "RS", "RU", "RW",
    "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "SS",
    "ST", "SV", "SX", "SY", "SZ", "TC", "TD", "TF", "TG", "TH", "TJ", "TK", "TL", "TM", "TN", "TO",
    "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VI",
    "VN", "VU", "WF", "WS", "XK", "YE", "YT", "ZA", "ZM", "ZW",
];

/// Returns the country ID (1-based index into `COUNTRIES`) for a given ISO alpha-2 code,
/// or `0` if the code is unknown.
pub fn country_id_from_iso2(iso: &str) -> u8 {
    COUNTRIES
        .binary_search(&iso)
        .map(|i| (i + 1) as u8)
        .unwrap_or(0)
}

/// Returns the ISO alpha-2 code for a given country ID, or `None` if ID is 0 or out of range.
pub fn country_iso_from_id(id: u8) -> Option<&'static str> {
    if id == 0 {
        return None;
    }
    COUNTRIES.get((id as usize).checked_sub(1)?).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        assert_eq!(country_iso_from_id(country_id_from_iso2("DE")), Some("DE"));
        assert_eq!(country_iso_from_id(country_id_from_iso2("US")), Some("US"));
        assert_eq!(country_iso_from_id(country_id_from_iso2("GB")), Some("GB"));
        assert_eq!(country_id_from_iso2("ZZ"), 0);
        assert_eq!(country_iso_from_id(0), None);
    }

    #[test]
    fn sorted() {
        let mut sorted = COUNTRIES.to_vec();
        sorted.sort_unstable();
        assert_eq!(COUNTRIES, sorted.as_slice(), "COUNTRIES must be sorted");
    }

    #[test]
    fn fits_in_u8() {
        assert!(COUNTRIES.len() < 255, "country list too long for u8 ID");
    }
}
