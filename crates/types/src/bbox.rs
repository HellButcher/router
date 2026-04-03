use crate::coordinate::LatLon;
#[cfg(feature = "bytemuck")]
use bytemuck::{Pod, Zeroable};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[repr(C)]
#[derive(Copy, Clone, Debug, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "bytemuck", derive(Pod, Zeroable))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct BoundingBox {
    pub min: LatLon,
    pub max: LatLon,
}

impl BoundingBox {
    pub const VOID: Self = BoundingBox {
        min: LatLon::INFINITY,
        max: LatLon::NEG_INFINITY,
    };
    pub const INFINITY: Self = BoundingBox {
        min: LatLon::NEG_INFINITY,
        max: LatLon::INFINITY,
    };

    #[inline]
    pub fn extents(&self) -> (f32, f32) {
        (self.max.lat - self.min.lat, self.max.lon - self.min.lon)
    }

    pub fn expand(&mut self, rhs: &Self) {
        if rhs.min.lat < self.min.lat {
            self.min.lat = rhs.min.lat;
        }
        if rhs.min.lon < self.min.lon {
            self.min.lon = rhs.min.lon;
        }
        if rhs.max.lat > self.max.lat {
            self.max.lat = rhs.max.lat;
        }
        if rhs.max.lon > self.max.lon {
            self.max.lon = rhs.max.lon;
        }
    }

    pub fn add(&mut self, pos: LatLon) {
        if pos.lat < self.min.lat {
            self.min.lat = pos.lat;
        }
        if pos.lon < self.min.lon {
            self.min.lon = pos.lon;
        }
        if pos.lat > self.max.lat {
            self.max.lat = pos.lat;
        }
        if pos.lon > self.max.lon {
            self.max.lon = pos.lon;
        }
    }
}

impl From<LatLon> for BoundingBox {
    #[inline]
    fn from(pos: LatLon) -> Self {
        Self { min: pos, max: pos }
    }
}
