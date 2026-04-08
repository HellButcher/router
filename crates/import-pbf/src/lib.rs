pub mod config;
pub mod country_lookup;
mod tags;

use osm_pbf_reader::Blobs;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelBridge,
    ParallelIterator,
};
use router_storage::{
    data::{
        attrib::{HighwayClass, SurfaceQuality, WayFlags},
        link_nodes_and_ways,
        node::{Node, NodeId},
        way::{Way, WayId},
    },
    spatial::SpatialIndexBuilder,
    spatial::haversine_m,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;
use std::{
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::{config::ImportConfig, country_lookup::CountryLookup, tags::WayTags};

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

    #[error(transparent)]
    Config(#[from] config::ConfigError),

    #[error(transparent)]
    CountryLookup(#[from] Box<country_lookup::LookupError>),
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
    config: ImportConfig,
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
            config: ImportConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ImportConfig) -> Self {
        self.config = config;
        self
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

        let maxspeed_map = self.config.maxspeed_map();

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
                        let surface_quality = surface_quality(&way_tags);
                        let max_speed = way_tags
                            .raw_max_speed
                            .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                            .unwrap_or(0);
                        let is_oneway = flags.contains(WayFlags::ONEWAY);
                        let is_reverse = is_oneway_reverse(&way_tags);
                        let mut refs = w.refs();
                        if let Some(mut current) = refs.next() {
                            for next in refs {
                                let (a, b) = if is_reverse {
                                    (NodeId(next), NodeId(current))
                                } else {
                                    (NodeId(current), NodeId(next))
                                };
                                let mut way = Way::new(id, a.0 as u64, b.0 as u64);
                                way.highway = highway;
                                way.flags = flags;
                                way.max_speed = max_speed;
                                way.surface_quality = surface_quality;
                                ways.push(way);
                                // For bidirectional roads also create the reverse edge.
                                if !is_oneway && !is_reverse {
                                    let mut rev = Way::new(id, b.0 as u64, a.0 as u64);
                                    rev.highway = highway;
                                    rev.flags = flags;
                                    rev.max_speed = max_speed;
                                    rev.surface_quality = surface_quality;
                                    ways.push(rev);
                                }
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
            tracing::info!(
                nodes = nodes_slice.len(),
                ways = ways_slice.len(),
                "linking"
            );
            ways_slice.par_iter().enumerate().for_each(|(i, way)| {
                link_nodes_and_ways(nodes_slice, i, way);
            });
        }

        tracing::info!("filter nodes");
        {
            let _span = tracing::info_span!("filter_nodes").entered();
            nodes
                .filter(Node::is_connected)
                .map_err(Error::WriteError)?;
            tracing::info!(nodes = nodes.len(), ways = ways.len(), "filtered");
        }

        // Build country lookup once (may be skipped if no boundaries file configured).
        let country_lookup = match &self.config.import.country_boundaries {
            Some(path) => {
                tracing::info!("loading country boundaries from {:?}", path);
                Some(CountryLookup::from_file(path).map_err(Box::new)?)
            }
            None => {
                tracing::warn!(
                    "no country_boundaries configured — country_id will be unknown for all ways"
                );
                None
            }
        };

        // Resolve from_node_idx / to_node_idx and fill in country_id.
        tracing::info!("resolving node indices");
        {
            let _span = tracing::info_span!("resolve_node_indices").entered();
            let nodes_slice = nodes.get_all().map_err(Error::WriteError)?;
            let nodes_slice: &[Node] = &nodes_slice;
            let ways_slice = ways.get_all_mut().map_err(Error::WriteError)?;

            ways_slice
                .par_iter_mut()
                .try_for_each(|way| -> Result<()> {
                    let from_id = NodeId(way.from_node_idx as i64);
                    let to_id = NodeId(way.to_node_idx as i64);
                    let from_idx = nodes_slice
                        .binary_search_by_key(&from_id, |n| n.id)
                        .map_err(|_| Error::NodeIdNotFound(from_id))?;
                    let to_idx = nodes_slice
                        .binary_search_by_key(&to_id, |n| n.id)
                        .map_err(|_| Error::NodeIdNotFound(to_id))?;
                    let from_pos = nodes_slice[from_idx].pos;
                    let to_pos = nodes_slice[to_idx].pos;

                    way.from_node_idx = from_idx as u64;
                    way.to_node_idx = to_idx as u64;

                    way.dist_m = haversine_m(from_pos.lat, from_pos.lon, to_pos.lat, to_pos.lon)
                        .min(u16::MAX as f32) as u16;

                    if let Some(lookup) = &country_lookup {
                        way.country_id = lookup.lookup(from_pos.lat, from_pos.lon);
                    }

                    Ok(())
                })?;
        }

        tracing::info!("flush nodes and ways");
        nodes.flush().map_err(Error::WriteError)?;
        ways.flush().map_err(Error::WriteError)?;

        tracing::info!("building spatial indices");
        {
            let nodes_ref = nodes.get_all().map_err(Error::WriteError)?;
            let ways_ref = ways.get_all().map_err(Error::WriteError)?;
            let nodes_s: &[Node] = &nodes_ref;
            let ways_s: &[Way] = &ways_ref;

            {
                let _span = tracing::info_span!("build_node_spatial_index").entered();
                SpatialIndexBuilder::new()
                    .build(
                        nodes_s.len(),
                        |i| {
                            let p = nodes_s[i].pos;
                            (p.lat, p.lon, p.lat, p.lon)
                        },
                        self.target_dir.join("node_spatial.bin"),
                    )
                    .map_err(Error::WriteError)?;
            }

            {
                let _span = tracing::info_span!("build_edge_spatial_index").entered();
                SpatialIndexBuilder::new()
                    .build(
                        ways_s.len(),
                        |i| {
                            let from = nodes_s[ways_s[i].from_node_idx as usize].pos;
                            let to = nodes_s[ways_s[i].to_node_idx as usize].pos;
                            (
                                from.lat.min(to.lat),
                                from.lon.min(to.lon),
                                from.lat.max(to.lat),
                                from.lon.max(to.lon),
                            )
                        },
                        self.target_dir.join("edge_spatial.bin"),
                    )
                    .map_err(Error::WriteError)?;
            }
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
        Some(H::bridleway) => HighwayClass::Bridleway,
        _ => HighwayClass::Unknown,
    }
}

fn is_oneway_reverse(tags: &tags::WayTags<'_>) -> bool {
    use tags::{Conditional, OneWay};
    matches!(&tags.oneway, Conditional::Simple(OneWay::reverse))
}

fn way_flags(tags: &tags::WayTags<'_>) -> WayFlags {
    use tags::{Conditional, Highway, OneWay};
    let mut flags = WayFlags::empty();

    // Oneway from explicit tag or junction type implying circulation direction.
    if matches!(
        &tags.oneway,
        Conditional::Simple(OneWay::yes | OneWay::reverse)
    ) || tags.junction.is_some_and(|j| j.implies_oneway())
    {
        flags |= WayFlags::ONEWAY;
    }

    // Access restrictions derived from highway class.
    match tags.highway {
        Some(Highway::footway)
        | Some(Highway::pedestrian)
        | Some(Highway::cycleway)
        | Some(Highway::path) => {
            flags |= WayFlags::NO_MOTOR;
            flags |= WayFlags::NO_HGV;
        }
        Some(Highway::bridleway) => {
            flags |= WayFlags::NO_MOTOR;
            flags |= WayFlags::NO_HGV;
            flags |= WayFlags::NO_BICYCLE;
        }
        Some(Highway::motorway) | Some(Highway::motorway_link) => {
            flags |= WayFlags::NO_BICYCLE;
            flags |= WayFlags::NO_FOOT;
        }
        _ => {}
    }

    // motorroad=yes has the same access restrictions as a motorway.
    if tags.motorroad {
        flags |= WayFlags::NO_BICYCLE;
        flags |= WayFlags::NO_FOOT;
    }

    // Mode-specific access tags (e.g. motorcar=no, bicycle=yes on access=no road).
    // Pass 1 — exclusions: set restriction flags (default mode blocks everything).
    // Pass 2 — inclusions: clear restriction flags re-opened by explicit mode tags.
    use tags::{Conditional as Cond, Mode};
    // Simple access=no/private/etc. blocks all modes.
    if let Cond::Simple(a) = &tags.access
        && a.is_excluded()
    {
        flags |= WayFlags::NO_MOTOR | WayFlags::NO_HGV | WayFlags::NO_BICYCLE | WayFlags::NO_FOOT;
    }
    if let Cond::Multi(items) = &tags.access {
        for item in items.iter().filter(|i| i.value.is_excluded()) {
            match item.mode {
                Mode::default => {
                    flags |= WayFlags::NO_MOTOR
                        | WayFlags::NO_HGV
                        | WayFlags::NO_BICYCLE
                        | WayFlags::NO_FOOT;
                }
                Mode::vehicle => {
                    flags |= WayFlags::NO_MOTOR | WayFlags::NO_HGV | WayFlags::NO_BICYCLE;
                }
                Mode::motor_vehicle => {
                    flags |= WayFlags::NO_MOTOR | WayFlags::NO_HGV;
                }
                Mode::motorcar | Mode::motorcycle | Mode::moped | Mode::mofa | Mode::motorhome => {
                    flags |= WayFlags::NO_MOTOR;
                }
                Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                    flags |= WayFlags::NO_HGV;
                }
                Mode::bicycle => flags |= WayFlags::NO_BICYCLE,
                Mode::foot => flags |= WayFlags::NO_FOOT,
                _ => {}
            }
        }
        for item in items.iter().filter(|i| !i.value.is_excluded()) {
            match item.mode {
                Mode::vehicle => {
                    flags &= !(WayFlags::NO_MOTOR | WayFlags::NO_HGV | WayFlags::NO_BICYCLE);
                }
                Mode::motor_vehicle => {
                    flags &= !(WayFlags::NO_MOTOR | WayFlags::NO_HGV);
                }
                Mode::motorcar | Mode::motorcycle | Mode::moped | Mode::mofa | Mode::motorhome => {
                    flags &= !WayFlags::NO_MOTOR;
                }
                Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                    flags &= !WayFlags::NO_HGV;
                }
                Mode::bicycle => flags &= !WayFlags::NO_BICYCLE,
                Mode::foot => flags &= !WayFlags::NO_FOOT,
                _ => {}
            }
        }
    }

    flags
}

/// Derive a `SurfaceQuality` tier from the way's surface/smoothness/tracktype tags.
/// Priority: smoothness > tracktype > surface (most specific first).
fn surface_quality(tags: &tags::WayTags<'_>) -> SurfaceQuality {
    use tags::{Smoothness, Surface, TrackType};

    if let Some(s) = tags.smoothness {
        return match s {
            Smoothness::excellent => SurfaceQuality::Excellent,
            Smoothness::good | Smoothness::intermediate => SurfaceQuality::Good,
            Smoothness::bad => SurfaceQuality::Bad,
            Smoothness::very_bad => SurfaceQuality::VeryBad,
            Smoothness::horrible | Smoothness::very_horrible => SurfaceQuality::Horrible,
            Smoothness::impassable => SurfaceQuality::Impassable,
            Smoothness::unknown => SurfaceQuality::Unknown,
        };
    }

    if let Some(t) = tags.tracktype {
        return match t {
            TrackType::grade1 => SurfaceQuality::Good,
            TrackType::grade2 => SurfaceQuality::Intermediate,
            TrackType::grade3 => SurfaceQuality::Bad,
            TrackType::grade4 => SurfaceQuality::VeryBad,
            TrackType::grade5 => SurfaceQuality::Horrible,
            TrackType::unknown => SurfaceQuality::Unknown,
        };
    }

    if let Some(s) = tags.surface {
        return match s {
            Surface::asphalt | Surface::concrete | Surface::metal | Surface::rubber => {
                SurfaceQuality::Excellent
            }
            Surface::paved | Surface::paving_stones => SurfaceQuality::Good,
            Surface::cobblestone
            | Surface::wood
            | Surface::stepping_stones
            | Surface::compacted => SurfaceQuality::Intermediate,
            Surface::unpaved => SurfaceQuality::Bad,
            Surface::gravel | Surface::ground => SurfaceQuality::VeryBad,
            Surface::grass => SurfaceQuality::Horrible,
            Surface::sand | Surface::ice => SurfaceQuality::Impassable,
            Surface::unknown => SurfaceQuality::Unknown,
        };
    }

    SurfaceQuality::Unknown
}
