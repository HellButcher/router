use std::{io, ops::Deref, path::PathBuf};

pub mod common;
pub mod error;
pub mod info;
pub mod locate;
pub mod matrix;
pub mod route;

use crate::error::{Error, Result};
use router_storage::spatial::SpatialIndex;

/// Options for creating a [`Service`].
pub struct ServiceOptions {
    /// Path to the storage directory (must contain `spatial.bin`).
    pub storage_dir: PathBuf,
    /// Maximum locate search radius in metres.
    pub max_radius_m: f32,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from("storage"),
            max_radius_m: 1_000.0,
        }
    }
}

pub struct Service {
    profiles: Vec<String>,
    pub(crate) spatial: SpatialIndex,
    pub(crate) max_radius_m: f32,
}

impl Service {
    pub fn open(options: ServiceOptions) -> io::Result<Self> {
        let spatial = SpatialIndex::open(options.storage_dir.join("spatial.bin"))?;
        Ok(Self {
            profiles: vec!["car".to_owned(), "hgv".to_owned()],
            spatial,
            max_radius_m: options.max_radius_m,
        })
    }

    pub fn default_profile(&self) -> Result<&str> {
        self.profiles
            .first()
            .map(Deref::deref)
            .ok_or(Error::NoProfilesAvailable)
    }

    pub fn get_profile(&self, profile: &str) -> Result<&'_ str> {
        for p in self.profiles.iter() {
            if p.eq_ignore_ascii_case(profile) {
                return Ok(p);
            }
        }
        Err(Error::UnknownProfile(profile.to_owned()))
    }

    #[inline]
    pub fn get_opt_profile(&self, profile: Option<&str>) -> Result<&'_ str> {
        if let Some(profile) = profile {
            self.get_profile(profile)
        } else {
            self.default_profile()
        }
    }
}
