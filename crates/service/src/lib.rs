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
    data::{edge_node::EdgeNode, turn_edge::TurnEdge, way::Way},
    idindex::IdEntry,
    spatial::SpatialIndex,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;
use speed_config::SpeedConfig;

pub struct ServiceOptions {
    pub storage_dir: PathBuf,
    pub max_radius_m: f32,
    pub speed_config: SpeedConfig,
}

impl Default for ServiceOptions {
    fn default() -> Self {
        Self {
            storage_dir: PathBuf::from("storage"),
            max_radius_m: 1_000.0,
            speed_config: SpeedConfig::default(),
        }
    }
}

pub struct Service {
    profiles: Vec<&'static Profile>,
    pub(crate) edge_node_spatial: SpatialIndex,
    pub(crate) edge_nodes: TableFile<EdgeNode>,
    pub(crate) turn_edges: TableFile<TurnEdge>,
    pub(crate) geometry: TableFile<LatLon>,
    pub(crate) ways: TableFile<Way>,
    pub(crate) way_id_index: TableFile<IdEntry>,
    pub(crate) max_radius_m: f32,
    pub(crate) speed_config: SpeedConfig,
}

impl Service {
    pub fn open(options: ServiceOptions) -> io::Result<Self> {
        let d = &options.storage_dir;
        let edge_node_spatial = SpatialIndex::open(d.join("edge_node_spatial.bin"))?;
        let edge_nodes = TableFile::<EdgeNode>::open_read_only(d.join("edge_nodes.bin"))?;
        let turn_edges = TableFile::<TurnEdge>::open_read_only(d.join("turn_edges.bin"))?;
        let geometry = TableFile::<LatLon>::open_read_only(d.join("geometry.bin"))?;
        let ways = TableFile::<Way>::open_read_only(d.join("ways.bin"))?;
        let way_id_index = TableFile::<IdEntry>::open_read_only(d.join("way_id_index.bin"))?;

        edge_nodes.header()?.verify()?;
        turn_edges.header()?.verify()?;
        geometry.header()?.verify()?;
        ways.header()?.verify()?;
        way_id_index.header()?.verify()?;

        Ok(Self {
            profiles: PROFILES.to_vec(),
            edge_node_spatial,
            edge_nodes,
            turn_edges,
            geometry,
            ways,
            way_id_index,
            max_radius_m: options.max_radius_m,
            speed_config: options.speed_config,
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

    pub(crate) fn profile_names(&self) -> impl Iterator<Item = &str> {
        self.profiles.iter().map(|p| p.name)
    }
}
