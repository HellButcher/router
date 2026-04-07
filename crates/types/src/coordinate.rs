#[cfg(feature = "bytemuck")]
use bytemuck::{Pod, Zeroable};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::fmt;
use std::ops::{Deref, DerefMut, Index, IndexMut};

/// A simple 2D coordinate in x and y.
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bytemuck", derive(Pod, Zeroable))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[doc(hidden)]
#[repr(C)]
pub struct XY {
    pub x: f32,
    pub y: f32,
}

impl Deref for XY {
    type Target = (f32, f32);
    #[inline(always)]
    fn deref(&self) -> &(f32, f32) {
        unsafe { &*(self as *const Self as *const _) }
    }
}
impl DerefMut for XY {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut (f32, f32) {
        unsafe { &mut *(self as *mut Self as *mut _) }
    }
}

/// A geographic coordinate in latitude (lat) and longitude (lon) in degrees.
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bytemuck", derive(Pod, Zeroable))]
#[repr(C)]
#[derive(Copy, Clone, PartialEq)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct LatLon {
    pub lat: f32,
    #[cfg_attr(feature = "serde", serde(alias = "lng"))]
    pub lon: f32,
}

impl LatLon {
    pub const ZERO: Self = Self::new(0., 0.);
    pub const INFINITY: Self = Self::new(f32::INFINITY, f32::INFINITY);
    pub const NEG_INFINITY: Self = Self::new(f32::NEG_INFINITY, f32::NEG_INFINITY);

    #[inline(always)]
    pub const fn new(lat: f32, lon: f32) -> Self {
        Self { lat, lon }
    }
}

#[inline(always)]
#[allow(non_snake_case)]
pub const fn LatLon(lat: f32, lon: f32) -> LatLon {
    LatLon { lat, lon }
}

impl Deref for LatLon {
    type Target = XY;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        unsafe { &*(self as *const Self as *const _) }
    }
}
impl DerefMut for LatLon {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(self as *mut Self as *mut _) }
    }
}

impl AsRef<[f32; 2]> for LatLon {
    #[inline(always)]
    fn as_ref(&self) -> &[f32; 2] {
        unsafe { &*(self as *const Self as *const [f32; 2]) }
    }
}
impl AsMut<[f32; 2]> for LatLon {
    #[inline(always)]
    fn as_mut(&mut self) -> &mut [f32; 2] {
        unsafe { &mut *(self as *mut Self as *mut [f32; 2]) }
    }
}

impl Default for LatLon {
    #[inline(always)]
    fn default() -> Self {
        Self::ZERO
    }
}
impl Index<usize> for LatLon {
    type Output = f32;
    #[inline(always)]
    fn index(&self, index: usize) -> &Self::Output {
        match index {
            0 => &self.lat,
            1 => &self.lon,
            _ => panic!("index out of bounds"),
        }
    }
}

impl IndexMut<usize> for LatLon {
    #[inline(always)]
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        match index {
            0 => &mut self.lat,
            1 => &mut self.lon,
            _ => panic!("index out of bounds"),
        }
    }
}

impl fmt::Display for LatLon {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {})", self.lat, self.lon)
    }
}

impl fmt::Debug for LatLon {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt.debug_tuple(stringify!($vec2))
            .field(&self.lat)
            .field(&self.lon)
            .finish()
    }
}

impl From<(f32, f32)> for LatLon {
    #[inline(always)]
    fn from((lat, lon): (f32, f32)) -> Self {
        Self { lat, lon }
    }
}
impl From<[f32; 2]> for LatLon {
    #[inline(always)]
    fn from([lat, lon]: [f32; 2]) -> Self {
        Self { lat, lon }
    }
}
impl From<LatLon> for (f32, f32) {
    #[inline(always)]
    fn from(v: LatLon) -> Self {
        (v.lat, v.lon)
    }
}
impl From<LatLon> for [f32; 2] {
    #[inline(always)]
    fn from(v: LatLon) -> Self {
        [v.lat, v.lon]
    }
}

#[cfg(feature = "geo-types")]
impl From<geo_types::Coordinate<f32>> for LatLon {
    #[inline(always)]
    fn from(coord: geo_types::Coordinate<f32>) -> Self {
        // geo_types uses x=lon, y=lat
        Self {
            lat: coord.y,
            lon: coord.x,
        }
    }
}

#[cfg(feature = "geo-types")]
impl From<geo_types::Coordinate<f64>> for LatLon {
    #[inline(always)]
    fn from(coord: geo_types::Coordinate<f64>) -> Self {
        // geo_types uses x=lon, y=lat
        Self {
            lat: coord.y as f32,
            lon: coord.x as f32,
        }
    }
}

#[cfg(feature = "geo-types")]
impl Into<geo_types::Coordinate<f32>> for LatLon {
    #[inline(always)]
    fn into(self) -> geo_types::Coordinate<f32> {
        // geo_types uses x=lon, y=lat
        geo_types::Coordinate {
            x: self.lon,
            y: self.lat,
        }
    }
}

#[cfg(feature = "geo-types")]
impl Into<geo_types::Coordinate<f64>> for LatLon {
    #[inline(always)]
    fn into(self) -> geo_types::Coordinate<f64> {
        // geo_types uses x=lon, y=lat
        geo_types::Coordinate {
            x: self.lon as f64,
            y: self.lat as f64,
        }
    }
}
