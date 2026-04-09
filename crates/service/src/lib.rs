use std::{io, path::PathBuf};

pub mod common;
pub mod error;
pub mod graph;
pub mod info;
pub mod inspect;
pub mod isochrone;
pub mod locate;
pub mod matrix;
pub mod meta;
pub mod profile;
pub mod route;
pub mod snap;
pub mod speed_config;
pub mod virtual_graph;

use crate::error::{Error, Result};
use profile::{PROFILES, Profile};
use router_storage::{
    data::{dim_restriction::DimRestrictionsTable, node::Node, way::Way},
    spatial::SpatialIndex,
    tablefile::TableFile,
};
use speed_config::SpeedConfig;

/// Options for creating a [`Service`].
pub struct ServiceOptions {
    /// Path to the storage directory (must contain `node_spatial.bin`, `edge_spatial.bin`, `nodes.bin`, `ways.bin`).
    pub storage_dir: PathBuf,
    /// Maximum locate search radius in metres.
    pub max_radius_m: f32,
    /// Optional path to a TOML file with per-country, per-profile speed overrides.
    pub speed_config_path: Option<std::path::PathBuf>,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from("storage"),
            max_radius_m: 1_000.0,
            speed_config_path: None,
        }
    }
}

pub struct Service {
    profiles: Vec<&'static Profile>,
    pub(crate) node_spatial: SpatialIndex,
    pub(crate) edge_spatial: SpatialIndex,
    pub(crate) nodes: TableFile<Node>,
    pub(crate) ways: TableFile<Way>,
    pub(crate) max_radius_m: f32,
    pub(crate) speed_config: SpeedConfig,
    pub(crate) dim_table: DimRestrictionsTable,
}

impl Service {
    pub fn open(options: ServiceOptions) -> io::Result<Self> {
        let node_spatial = SpatialIndex::open(options.storage_dir.join("node_spatial.bin"))?;
        let edge_spatial = SpatialIndex::open(options.storage_dir.join("edge_spatial.bin"))?;
        let nodes = TableFile::<Node>::open_read_only(options.storage_dir.join("nodes.bin"))?;
        let ways = TableFile::<Way>::open_read_only(options.storage_dir.join("ways.bin"))?;
        nodes.header()?.verify()?;
        ways.header()?.verify()?;

        let speed_config = match options.speed_config_path {
            Some(ref path) => SpeedConfig::from_file(path)
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?,
            None => SpeedConfig::default(),
        };

        let dim_table = DimRestrictionsTable::read_from_file(
            &options.storage_dir.join("dim_restrictions.bin"),
        )?;

        Ok(Self {
            profiles: PROFILES.to_vec(),
            node_spatial,
            edge_spatial,
            nodes,
            ways,
            max_radius_m: options.max_radius_m,
            speed_config,
            dim_table,
        })
    }

    pub fn default_profile(&self) -> Result<&'static Profile> {
        self.profiles
            .first()
            .copied()
            .ok_or(Error::NoProfilesAvailable)
    }

    pub fn get_profile(&self, name: &str) -> Result<&'static Profile> {
        for p in self.profiles.iter() {
            if p.name.eq_ignore_ascii_case(name) {
                return Ok(p);
            }
        }
        Err(Error::UnknownProfile(name.to_owned()))
    }

    #[inline]
    pub fn get_opt_profile(&self, profile: Option<&str>) -> Result<&'static Profile> {
        if let Some(name) = profile {
            self.get_profile(name)
        } else {
            self.default_profile()
        }
    }
}

// Keep backward compatibility for `info` which uses profile names as strings.
impl Service {
    pub(crate) fn profile_names(&self) -> impl Iterator<Item = &str> {
        self.profiles.iter().map(|p| p.name)
    }
}
