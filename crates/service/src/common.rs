use std::collections::HashMap;
use std::num::{NonZeroU32, NonZeroU64};
use std::ops::{Deref, DerefMut};

use router_types::coordinate::LatLon;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::meta::{EdgeMeta, NodeMeta};

/// Units for distances
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Unit {
    #[cfg_attr(
        feature = "serde",
        serde(rename = "km", alias = "kilometer", alias = "kilometers")
    )]
    #[default]
    Kilometers,
    #[cfg_attr(
        feature = "serde",
        serde(rename = "mi", alias = "mile", alias = "miles")
    )]
    Miles,
}

/// A Location is a Point giben as latitude (lat) and longitude (lon) with additional information
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Location {
    #[cfg_attr(feature = "serde", serde(flatten))]
    pub coordinate: LatLon,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub radius: Option<NonZeroU32>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub allow_u_turns: Option<bool>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub way_id: Option<NonZeroU64>,

    /// Fraction along the snapped way segment (0.0 = from-node, 1.0 = to-node).
    /// Only present for [`SnapMode::Edge`] snaps.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub fraction: Option<f32>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub id: Option<String>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub node_meta: Option<NodeMeta>,

    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub edge_meta: Option<EdgeMeta>,

    #[cfg(feature = "serde")]
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

impl Deref for Location {
    type Target = LatLon;
    #[inline(always)]
    fn deref(&self) -> &LatLon {
        &self.coordinate
    }
}

impl DerefMut for Location {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut LatLon {
        &mut self.coordinate
    }
}

impl<T: Into<LatLon>> From<T> for Location {
    #[inline]
    fn from(value: T) -> Self {
        Self {
            coordinate: value.into(),
            ..Default::default()
        }
    }
}

/// A list of Points
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Points {
    /// List of Points given as Pairs of latitude (lat) and longitude (lon).
    Array(Vec<[f32; 2]>),
    /// List of Points encoded as a single string using the Polyline Format
    Encoded(String),
}

/// A list of Locations
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "serde", serde(untagged))]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub enum Locations {
    /// List of Locations given as Location-Objects
    LocationArray(Vec<Location>),
    /// List of Locations given as points as pairs of latitude (lat) and longitude (lon).
    Array(Vec<[f32; 2]>),
    /// List of Locations given as points encoded as a single string using the Polyline Format
    Encoded(String),
}

impl TryFrom<Locations> for Vec<Location> {
    type Error = router_polyline::Error;
    fn try_from(locs: Locations) -> Result<Self, router_polyline::Error> {
        Ok(match locs {
            Locations::LocationArray(locs) => locs,
            Locations::Array(coords) => coords.into_iter().map(Into::into).collect(),
            Locations::Encoded(s) => router_polyline::decode::<2>(&s, 5)?
                .into_iter()
                .map(Into::into)
                .collect(),
        })
    }
}

impl Points {
    pub fn array_from(points: impl IntoIterator<Item = impl Into<[f32; 2]>>) -> Self {
        Self::Array(points.into_iter().map(Into::into).collect())
    }

    pub fn encoded_from(points: impl IntoIterator<Item = impl Into<[f32; 2]>>) -> Self {
        Self::Encoded(router_polyline::encode(
            points.into_iter().map(Into::into),
            5,
        ))
    }

    #[inline]
    pub fn into_encoded(self) -> String {
        match self {
            Points::Array(coords) => router_polyline::encode(coords, 5),
            Points::Encoded(s) => s,
        }
    }
    #[inline]
    pub fn try_into_array(self) -> Result<Vec<[f32; 2]>, router_polyline::Error> {
        Ok(match self {
            Points::Array(coords) => coords,
            Points::Encoded(s) => router_polyline::decode(&s, 5)?,
        })
    }

    #[inline]
    pub fn encode<T: Into<[f32; 2]>>(points: impl IntoIterator<Item = T>) -> String {
        router_polyline::encode(points.into_iter().map(Into::into), 5)
    }
}

impl<I: Into<[f32; 2]>> FromIterator<I> for Points {
    #[inline]
    fn from_iter<T: IntoIterator<Item = I>>(iter: T) -> Self {
        Self::Array(iter.into_iter().map(Into::into).collect())
    }
}

impl<T: From<LatLon>> TryFrom<Points> for Vec<T> {
    type Error = router_polyline::Error;
    fn try_from(points: Points) -> Result<Self, router_polyline::Error> {
        Ok(match points {
            Points::Array(coords) => coords
                .into_iter()
                .map(|arr| LatLon::from(arr).into())
                .collect(),
            Points::Encoded(s) => router_polyline::decode::<2>(&s, 5)?
                .into_iter()
                .map(|arr| LatLon::from(arr).into())
                .collect(),
        })
    }
}
