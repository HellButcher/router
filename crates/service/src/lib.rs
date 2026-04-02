use std::ops::Deref;

pub mod error;
pub mod common;
pub mod info;
pub mod locate;
pub mod matrix;
pub mod route;

use crate::error::{Error, Result};

pub struct Service {
    profiles: Vec<String>,
}

impl Service {
    pub fn new() -> Self {
        Self {
            profiles: vec!["car".to_owned(), "hgv".to_owned()],
        }
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

impl Default for Service {
    #[inline(always)]
    fn default() -> Self {
        Self::new()
    }
}
