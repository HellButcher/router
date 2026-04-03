mod tags;

use osm_pbf_reader::Blobs;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, ParallelBridge, ParallelIterator,
};
use router_storage::{
    data::{
        attrib::{HighwayClass, WayFlags},
        link_nodes_and_ways,
        node::{Node, NodeId},
        way::{Way, WayId},
    },
    spatial::SpatialIndexBuilder,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;
use std::{
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::tags::WayTags;

#[derive(Error, Debug)]
pub enum Error {
    #[error(transparent)]
    ReadError(#[from] osm_pbf_reader::error::Error),

    #[error(transparent)]
    WriteError(io::Error),

    #[error("The pbf-feature {0} is not supported")]
    UnsupportedFeature(String),

    #[error("The pbf-feature {0} is and not available but required for processign the file")]
    FeatureRequired(&'static str),

    #[error("The pbf-file is not sorted")]
    NotSorted,

    #[error("Node id {0:?} not found")]
    NodeIdNotFound(NodeId),

    #[error("Way id {0:?} not found")]
    WayIdNotFound(WayId),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

const SUPPORTED_FEATURES: &[&[u8]] = &[
    b"OsmSchema-V0.6",
    b"DenseNodes",
    // b"HistoricalInformation",
];

const REQUIRED_FEATURES: &[&str] = &["Sort.Type_then_ID"];

pub struct Importer<R> {
    target_dir: PathBuf,
    read: R,
}

impl Importer<io::BufReader<File>> {
    #[inline]
    pub fn from_path(source: &Path) -> Result<Self> {
        Ok(Self::from_read(
            File::open(source).map_err(Into::<osm_pbf_reader::error::Error>::into)?,
        ))
    }
}

impl<R: io::Read + Send> Importer<BufReader<R>> {
    #[inline]
    pub fn from_read(read: R) -> Self {
        Self::from_buf_read(io::BufReader::new(read))
    }
}

impl<R: io::BufRead + Send> Importer<R> {
    pub fn from_buf_read(read: R) -> Self {
        Self {
            target_dir: PathBuf::from("storage"),
            read,
        }
    }

    pub fn import(self) -> Result<()> {
        tracing::info!("importing into {:?}", self.target_dir);
        let _ = std::fs::create_dir_all(&self.target_dir);

        let blobs = Blobs::from_buf_read(self.read)?;
        // validate header
        let header = blobs.header();
        tracing::info!("PBF required features: {:?}", header.required_features());
        tracing::info!("PBF optional features: {:?}", header.optional_features());
        for f in &header.required_features() {
            if !SUPPORTED_FEATURES.contains(&f.as_bytes()) {
                return Err(Error::UnsupportedFeature(f.to_cow_lossy().into_owned()));
            }
        }
        for feat in REQUIRED_FEATURES {
            if !header
                .optional_features()
                .iter()
                .any(|f| f.as_bytes() == feat.as_bytes())
            {
                return Err(Error::FeatureRequired(feat));
            }
        }

        let mut nodes = TableFile::<Node>::open_override(self.target_dir.join("nodes.bin"))
            .map_err(Error::WriteError)?;
        let mut ways = TableFile::<Way>::open_override(self.target_dir.join("ways.bin"))
            .map_err(Error::WriteError)?;

        let mut nodes_append = nodes.appender().map_err(Error::WriteError)?.spawn();
        let mut ways_append = ways.appender().map_err(Error::WriteError)?.spawn();

        let _span = tracing::info_span!("import").entered();
        let _span = tracing::info_span!("parse_blobs").entered();
        blobs
            .into_iter()
            .map(|b| (nodes_append.start(), ways_append.start(), b))
            .par_bridge()
            .try_for_each(|(nodes_appender, ways_appender, blob)| -> Result<()> {
                let data = blob?.into_decoded()?;
                let mut nodes = Vec::new();
                let mut ways = Vec::new();
                let mut old_id = i64::MIN;
                for group in data.iter_groups() {
                    for n in group.iter_nodes() {
                        let id = NodeId(n.id());
                        assert!(id.0 > old_id);
                        old_id = id.0;
                        let pos = LatLon(n.lat_deg() as f32, n.lon_deg() as f32);
                        nodes.push(Node::new(id, pos));
                    }
                    for n in group.iter_dense_nodes() {
                        let id = NodeId(n.id());
                        assert!(id.0 > old_id);
                        old_id = id.0;
                        let pos = LatLon(n.lat_deg() as f32, n.lon_deg() as f32);
                        nodes.push(Node::new(id, pos));
                    }
                    for w in group.iter_ways() {
                        let id = w.id();
                        assert!(id > old_id);
                        old_id = id;
                        let id = WayId(id);
                        let mut way_tags = WayTags::default();
                        w.tags().iter().for_each(|(k, v)| {
                            way_tags.set_tag(k, v);
                        });
                        if way_tags.is_excluded() {
                            continue;
                        }
                        let highway = highway_class(way_tags.highway);
                        let flags = way_flags(&way_tags);
                        let max_speed = way_tags.max_speed.unwrap_or(0);
                        let mut refs = w.refs().iter();
                        if let Some(mut current) = refs.next() {
                            for next in refs {
                                let mut way = Way::new(id, NodeId(current), NodeId(next));
                                way.highway = highway;
                                way.flags = flags;
                                way.max_speed = max_speed;
                                ways.push(way);
                                current = next;
                            }
                        }
                    }
                }
                nodes_appender.done(nodes);
                ways_appender.done(ways);
                Ok(())
            })?;

        nodes_append
            .join()
            .expect("the node-writer thread has panicked");

        ways_append
            .join()
            .expect("the way-writer thread has panicked");

        drop(_span);
        tracing::info!("written nodes and ways");

        {
            let _span = tracing::info_span!("link_nodes_and_ways").entered();
            let nodes_slice = nodes.get_all().map_err(Error::WriteError)?;
            let ways_slice = ways.get_all().map_err(Error::WriteError)?;
            let nodes_slice: &[Node] = &nodes_slice;
            let ways_slice: &[Way] = &ways_slice;
            ways_slice.par_iter().enumerate().for_each(|(i, way)| {
                link_nodes_and_ways(&nodes_slice, i, way);
            });
        }

        tracing::info!("filter nodes");
        {
            let _span = tracing::info_span!("filter_nodes").entered();
            nodes
                .filter(Node::is_connected)
                .map_err(Error::WriteError)?;
        }

        tracing::info!("linked nodes and ways");

        nodes.flush().map_err(Error::WriteError)?;
        ways.flush().map_err(Error::WriteError)?;

        tracing::info!("building spatial index");
        {
            let _span = tracing::info_span!("build_spatial_index").entered();
            let nodes_ref = nodes.get_all().map_err(Error::WriteError)?;
            // Deref to &[Node] (Sync) so the closure can be shared across rayon threads.
            let nodes: &[Node] = &*nodes_ref;
            SpatialIndexBuilder::new()
                .build(
                    nodes.len(),
                    |i| (nodes[i].pos.lat, nodes[i].pos.lon),
                    self.target_dir.join("spatial.bin"),
                )
                .map_err(Error::WriteError)?;
        }

        tracing::info!("flushed");

        Ok(())
    }
}

fn highway_class(highway: Option<tags::Highway>) -> HighwayClass {
    use tags::Highway as H;
    match highway {
        Some(H::motorway) => HighwayClass::Motorway,
        Some(H::trunk) => HighwayClass::Trunk,
        Some(H::primary) => HighwayClass::Primary,
        Some(H::secondary) => HighwayClass::Secondary,
        Some(H::tertiary) => HighwayClass::Tertiary,
        Some(H::motorway_link) => HighwayClass::MotorwayLink,
        Some(H::trunk_link) => HighwayClass::TrunkLink,
        Some(H::primary_link) => HighwayClass::PrimaryLink,
        Some(H::secondary_link) => HighwayClass::SecondaryLink,
        Some(H::tertiary_link) => HighwayClass::TertiaryLink,
        Some(H::unclassified) => HighwayClass::Unclassified,
        Some(H::residential) => HighwayClass::Residential,
        Some(H::living_street) => HighwayClass::LivingStreet,
        Some(H::service) => HighwayClass::Service,
        Some(H::track) => HighwayClass::Track,
        Some(H::road) => HighwayClass::Road,
        Some(H::pedestrian) => HighwayClass::Pedestrian,
        Some(H::footway) => HighwayClass::Footway,
        Some(H::cycleway) => HighwayClass::Cycleway,
        Some(H::path) => HighwayClass::Path,
        _ => HighwayClass::Unknown,
    }
}

fn way_flags(tags: &tags::WayTags<'_>) -> WayFlags {
    use tags::{Conditional, OneWay};
    let mut flags = WayFlags::empty();

    // Oneway
    let is_oneway = matches!(&tags.oneway, Conditional::Simple(OneWay::yes));
    let is_reverse = matches!(&tags.oneway, Conditional::Simple(OneWay::reverse));
    if is_oneway {
        flags |= WayFlags::ONEWAY;
    }
    if is_reverse {
        flags |= WayFlags::ONEWAY_REVERSE;
    }

    // Motor vehicle / HGV restrictions based on highway class
    use tags::Highway;
    match tags.highway {
        Some(Highway::footway)
        | Some(Highway::pedestrian)
        | Some(Highway::cycleway)
        | Some(Highway::path) => {
            flags |= WayFlags::NO_MOTOR;
            flags |= WayFlags::NO_HGV;
        }
        Some(Highway::motorway) | Some(Highway::motorway_link) => {
            flags |= WayFlags::NO_BICYCLE;
            flags |= WayFlags::NO_FOOT;
        }
        _ => {}
    }

    flags
}
