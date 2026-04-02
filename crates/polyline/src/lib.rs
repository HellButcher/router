use std::fmt;

const BASE: i32 = 10;

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct Error(usize);
impl std::error::Error for Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Invalid input at offset {}", self.0)
    }
}

/// Encode an iterator of coordinates into a polyline: a compact base64 representation
///
/// https://developers.google.com/maps/documentation/utilities/polylinealgorithm
pub fn encode_fixed<I, F, const DIM: usize>(coords: I, convert: F) -> String
where
    I: IntoIterator,
    F: Fn(I::Item) -> [i32; DIM],
{
    let mut output = String::new();
    let mut previous = [0i32; DIM];
    for current in coords.into_iter() {
        let current = convert(current);
        for i in 0..DIM {
            let current = current[i];
            let previous = std::mem::replace(&mut previous[i], current);
            let diff = current - previous;
            let mut bits = (diff << 1) as u32;
            if diff < 0 {
                bits = !bits;
            }
            while bits >= 0x20 {
                let value = (0x20 | bits & 0x1f) + 63;
                bits >>= 5;
                // SAFETY: value is in range 95 ('_') to 126 ('~')
                let ch = unsafe { char::from_u32_unchecked(value) };
                output.push(ch);
            }
            let value = bits + 63;
            // SAFETY: value is in range 63 ('?') to 94 ('^')
            let ch = unsafe { char::from_u32_unchecked(value) };
            output.push(ch);
        }
    }
    output
}

/// Encode an iterator of coordinates into a polyline: a compact base64 representation
///
/// https://developers.google.com/maps/documentation/utilities/polylinealgorithm
pub fn encode<const DIM: usize>(
    coords: impl IntoIterator<Item = [f32; DIM]>,
    precision: u32,
) -> String {
    let factor = BASE.pow(precision) as f32;
    encode_fixed(coords, |current| {
        let mut result = [0i32; DIM];
        for i in 0..DIM {
            result[i] = (current[i] * factor).round() as i32;
        }
        result
    })
}

/// Decode polyline into an vector of coordinates.
///
/// https://developers.google.com/maps/documentation/utilities/polylinealgorithm
pub fn decode_fixed<F, R, const DIM: usize>(polyline: &str, convert: F) -> Result<Vec<R>, Error>
where
    F: Fn([i32; DIM]) -> R,
{
    let polyline = polyline.as_bytes();
    let mut i = 0;
    let mut j = 0;
    let mut word = 0u32;
    let mut shift = 0;
    let mut coords = Vec::new();
    let mut tmp = [0i32; DIM];
    while i < polyline.len() {
        let byte = polyline[i];
        if !(63..63 + 0x40).contains(&byte) {
            return Err(Error(i));
        }
        let byte = polyline[i] - 63;
        word |= (byte as u32 & 0x1f) << shift;
        i += 1;
        shift += 5;
        if byte >= 0x20 {
            continue; // shift next byte
        }
        let mut value = (word >> 1) as i32;
        if word & 1 != 0 {
            // was negative (first bit set)?
            value = !value;
        }
        // reset word & shift for next iteration
        word = 0;
        shift = 0;

        tmp[j] += value;
        j += 1;
        if j >= DIM {
            j = 0;
            coords.push(convert(tmp));
        }
    }
    Ok(coords)
}

/// Decode polyline into an vector of coordinates.
///
/// https://developers.google.com/maps/documentation/utilities/polylinealgorithm
pub fn decode<const DIM: usize>(polyline: &str, precision: u32) -> Result<Vec<[f32; DIM]>, Error> {
    let factor = BASE.pow(precision) as f32;
    decode_fixed(polyline, |fixed: [i32; DIM]| {
        let mut result = [0f32; DIM];
        for i in 0..DIM {
            result[i] = fixed[i] as f32 / factor;
        }
        result
    })
}

#[cfg(test)]
mod test {
    use crate::{decode, encode};

    const POLYLINE: &str = "_p~iF~ps|U_ulLnnqC_mqNvxq`@";
    const COORDS: &[[f32; 2]] = &[[38.5, -120.2], [40.7, -120.95], [43.252, -126.453]];

    #[test]
    fn test_encode() {
        assert_eq!(POLYLINE, encode(COORDS.iter().copied(), 5));
    }

    #[test]
    fn test_decode() {
        assert_eq!(COORDS, decode(POLYLINE, 5).unwrap());
    }
}
