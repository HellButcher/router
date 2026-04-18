pub mod country_lookup;
mod tags;

use osm_pbf_reader::Blobs;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelBridge,
    ParallelIterator,
};
use router_storage::{
    data::{
        attrib::{HighwayClass, NodeFlags, SurfaceQuality, WayFlags},
        dim_restriction::DimRestriction,
        link_nodes_and_ways,
        node::{Node, NodeId},
        way::{Way, WayId},
        way_extended::WayExtended,
    },
    spatial::SpatialIndexBuilder,
    spatial::haversine_m,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
};
use thiserror::Error;

use crate::{
    country_lookup::CountryLookup,
    tags::{NodeTags, WayTags},
};

/// Built-in named maxspeed values.
/// Keys are OSM `maxspeed` tag values; values are km/h.
/// Country-coded entries (`"DE:urban"`) take priority over generic ones (`"urban"`).
static BUILTIN_MAXSPEED: &[(&str, u8)] = &[
    // Generic
    ("walk", 7),
    ("walking", 7),
    ("living_street", 10),
    ("urban", 50),
    ("rural", 90),
    ("motorway", 130),
    // Germany
    ("DE:living_street", 10),
    ("DE:urban", 50),
    ("DE:rural", 100),
    ("DE:motorway", 130),
    // Austria
    ("AT:living_street", 10),
    ("AT:urban", 50),
    ("AT:rural", 100),
    ("AT:motorway", 130),
    // Switzerland
    ("CH:living_street", 10),
    ("CH:urban", 50),
    ("CH:rural", 80),
    ("CH:motorway", 120),
    // France
    ("FR:living_street", 20),
    ("FR:urban", 50),
    ("FR:rural", 80),
    ("FR:motorway", 130),
    // Netherlands
    ("NL:living_street", 15),
    ("NL:urban", 50),
    ("NL:rural", 80),
    ("NL:motorway", 100),
    // Belgium
    ("BE:living_street", 20),
    ("BE:urban", 50),
    ("BE:rural", 90),
    ("BE:motorway", 120),
    // Italy
    ("IT:living_street", 10),
    ("IT:urban", 50),
    ("IT:rural", 90),
    ("IT:motorway", 130),
    // Spain
    ("ES:living_street", 20),
    ("ES:urban", 50),
    ("ES:rural", 90),
    ("ES:motorway", 120),
    // Portugal
    ("PT:living_street", 20),
    ("PT:urban", 50),
    ("PT:rural", 90),
    ("PT:motorway", 120),
    // Poland
    ("PL:living_street", 20),
    ("PL:urban", 50),
    ("PL:rural", 90),
    ("PL:motorway", 140),
    // Czech Republic
    ("CZ:living_street", 20),
    ("CZ:urban", 50),
    ("CZ:rural", 90),
    ("CZ:motorway", 130),
    // United Kingdom
    ("GB:living_street", 10),
    ("GB:urban", 48),      // 30 mph
    ("GB:rural", 96),      // 60 mph
    ("GB:motorway", 112),  // 70 mph
    ("GB:nsl_single", 96), // 60 mph National Speed Limit, single carriageway
    ("GB:nsl_dual", 112),  // 70 mph National Speed Limit, dual carriageway
    // Russia
    ("RU:living_street", 20),
    ("RU:urban", 60),
    ("RU:rural", 90),
    ("RU:motorway", 110),
    // Ukraine
    ("UA:living_street", 20),
    ("UA:urban", 60),
    ("UA:rural", 90),
    ("UA:motorway", 130),
    // United States
    ("US:living_street", 25),
    ("US:urban", 40),
    ("US:rural", 88),     // 55 mph
    ("US:motorway", 104), // 65 mph
];

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
    country_boundaries: Option<PathBuf>,
    maxspeed: HashMap<String, u8>,
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
            country_boundaries: None,
            maxspeed: HashMap::new(),
        }
    }

    /// Set the output directory for the imported data files.
    pub fn with_target_dir(mut self, dir: PathBuf) -> Self {
        self.target_dir = dir;
        self
    }

    /// Set the path to a GeoJSON file with country boundary polygons.
    /// If not set, country lookup is skipped and `country_id` is left as unknown.
    pub fn with_country_boundaries(mut self, path: PathBuf) -> Self {
        self.country_boundaries = Some(path);
        self
    }

    /// Set named maxspeed overrides (merged over built-in defaults).
    pub fn with_maxspeed(mut self, overrides: HashMap<String, u8>) -> Self {
        self.maxspeed = overrides;
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

        // Build merged maxspeed map: built-ins overridden by config values.
        let maxspeed_map: HashMap<String, u8> = BUILTIN_MAXSPEED
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .chain(self.maxspeed.into_iter())
            .collect();

        let mut nodes = TableFile::<Node>::open_override(self.target_dir.join("nodes.bin"))
            .map_err(Error::WriteError)?;
        let mut ways = TableFile::<Way>::open_override(self.target_dir.join("ways.bin"))
            .map_err(Error::WriteError)?;
        let mut way_extended =
            TableFile::<WayExtended>::open_override(self.target_dir.join("way_extended.bin"))
                .map_err(Error::WriteError)?;

        let mut nodes_append = nodes.appender().map_err(Error::WriteError)?.spawn();
        let mut ways_append = ways.appender().map_err(Error::WriteError)?.spawn();
        let mut way_extended_append = way_extended.appender().map_err(Error::WriteError)?.spawn();

        let _span = tracing::info_span!("import").entered();
        let _span = tracing::info_span!("parse_blobs").entered();
        blobs
            .into_iter()
            .map(|b| {
                (
                    nodes_append.start(),
                    ways_append.start(),
                    way_extended_append.start(),
                    b,
                )
            })
            .par_bridge()
            .try_for_each(
                |(nodes_appender, ways_appender, extended_appender, blob)| -> Result<()> {
                    let data = blob?.into_decoded()?;
                    let mut nodes = Vec::new();
                    let mut ways = Vec::new();
                    let mut extended = Vec::new();
                    let mut old_id = i64::MIN;
                    for group in data.iter_groups() {
                        for n in group.iter_nodes() {
                            let id = NodeId(n.id());
                            assert!(id.0 > old_id);
                            old_id = id.0;
                            let pos = LatLon(n.lat_deg() as f32, n.lon_deg() as f32);
                            let mut node_tags = NodeTags::default();
                            n.tags().iter().for_each(|(k, v)| {
                                node_tags.set_tag(k, v);
                            });
                            let mut node = Node::new(id, pos);
                            node.flags = derive_node_flags(&node_tags);
                            nodes.push(node);
                        }
                        for n in group.iter_dense_nodes() {
                            let id = NodeId(n.id());
                            assert!(id.0 > old_id);
                            old_id = id.0;
                            let pos = LatLon(n.lat_deg() as f32, n.lon_deg() as f32);
                            let mut node_tags = NodeTags::default();
                            n.tags().iter().for_each(|(k, v)| {
                                node_tags.set_tag(k, v);
                            });
                            let mut node = Node::new(id, pos);
                            node.flags = derive_node_flags(&node_tags);
                            nodes.push(node);
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
                            let highway = if way_tags.ferry {
                                HighwayClass::Ferry
                            } else {
                                highway_class(way_tags.highway)
                            };
                            let dim = dim_restriction_from_tags(&way_tags);
                            let mut flags = way_flags(&way_tags);
                            if !dim.is_none() {
                                flags |= WayFlags::HAS_EXTENDED;
                                extended.push(WayExtended::new(id, dim));
                            }
                            let surface_quality = surface_quality(&way_tags);
                            let max_speed = way_tags
                                .raw_max_speed
                                .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                                .unwrap_or(0);
                            let max_speed_forward = way_tags
                                .raw_max_speed_forward
                                .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                                .unwrap_or(max_speed);
                            let max_speed_backward = way_tags
                                .raw_max_speed_backward
                                .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                                .unwrap_or(max_speed);
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
                                    let bicycle_contraflow = is_bicycle_contraflow(&way_tags);

                                    let mut way = Way::new(id, a.0 as u64, b.0 as u64);
                                    way.highway = highway;
                                    way.flags = flags;
                                    way.max_speed = max_speed_forward;
                                    way.surface_quality = surface_quality;
                                    ways.push(way);

                                    if !is_oneway && !is_reverse {
                                        let mut rev = Way::new(id, b.0 as u64, a.0 as u64);
                                        rev.highway = highway;
                                        rev.flags = flags;
                                        rev.max_speed = max_speed_backward;
                                        rev.surface_quality = surface_quality;
                                        ways.push(rev);
                                    } else if is_oneway && bicycle_contraflow {
                                        let mut rev = Way::new(id, b.0 as u64, a.0 as u64);
                                        rev.highway = highway;
                                        let mut rev_flags = flags & !WayFlags::ONEWAY;
                                        rev_flags |= WayFlags::NO_MOTOR | WayFlags::NO_HGV;
                                        rev.flags = rev_flags;
                                        rev.max_speed = max_speed_backward;
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
                    extended_appender.done(extended);
                    Ok(())
                },
            )?;

        nodes_append
            .join()
            .expect("the node-writer thread has panicked");

        ways_append
            .join()
            .expect("the way-writer thread has panicked");

        way_extended_append
            .join()
            .expect("the way-extended-writer thread has panicked");

        tracing::info!(
            nodes = nodes.len(),
            ways = ways.len(),
            extended = way_extended.len(),
            "written nodes and ways"
        );
        drop(_span);

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
        let country_lookup = match &self.country_boundaries {
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

    // Toll, tunnel, bridge.
    if tags.toll == Some(true) {
        flags |= WayFlags::TOLL;
    }
    if tags.tunnel {
        flags |= WayFlags::TUNNEL;
    }
    if tags.bridge {
        flags |= WayFlags::BRIDGE;
    }

    flags
}

/// Derive `NodeFlags` from parsed node tags.
fn derive_node_flags(tags: &tags::NodeTags<'_>) -> NodeFlags {
    use tags::{Barrier, Conditional as Cond, Mode, NodeHighway};
    let mut flags = NodeFlags::empty();

    if let Some(barrier) = tags.barrier {
        match barrier {
            Barrier::bollard => {
                flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV;
            }
            Barrier::gate => {
                flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV;
            }
            Barrier::kissing_gate => {
                flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE;
            }
            Barrier::cycle_barrier => {
                flags |= NodeFlags::NO_BICYCLE;
            }
            Barrier::unknown => {}
        }
    }

    if let Cond::Simple(a) = &tags.access
        && a.is_excluded()
    {
        flags |=
            NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE | NodeFlags::NO_FOOT;
    }
    if let Cond::Multi(items) = &tags.access {
        for item in items.iter().filter(|i| i.value.is_excluded()) {
            match item.mode {
                Mode::default => {
                    flags |= NodeFlags::NO_MOTOR
                        | NodeFlags::NO_HGV
                        | NodeFlags::NO_BICYCLE
                        | NodeFlags::NO_FOOT;
                }
                Mode::vehicle => {
                    flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE;
                }
                Mode::motor_vehicle => flags |= NodeFlags::NO_MOTOR | NodeFlags::NO_HGV,
                Mode::motorcar | Mode::motorcycle | Mode::moped | Mode::mofa | Mode::motorhome => {
                    flags |= NodeFlags::NO_MOTOR;
                }
                Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                    flags |= NodeFlags::NO_HGV;
                }
                Mode::bicycle => flags |= NodeFlags::NO_BICYCLE,
                Mode::foot => flags |= NodeFlags::NO_FOOT,
                _ => {}
            }
        }
        for item in items.iter().filter(|i| !i.value.is_excluded()) {
            match item.mode {
                Mode::vehicle => {
                    flags &= !(NodeFlags::NO_MOTOR | NodeFlags::NO_HGV | NodeFlags::NO_BICYCLE);
                }
                Mode::motor_vehicle => flags &= !(NodeFlags::NO_MOTOR | NodeFlags::NO_HGV),
                Mode::motorcar | Mode::motorcycle | Mode::moped | Mode::mofa | Mode::motorhome => {
                    flags &= !NodeFlags::NO_MOTOR;
                }
                Mode::hgv | Mode::goods | Mode::coach | Mode::tourist_bus => {
                    flags &= !NodeFlags::NO_HGV;
                }
                Mode::bicycle => flags &= !NodeFlags::NO_BICYCLE,
                Mode::foot => flags &= !NodeFlags::NO_FOOT,
                _ => {}
            }
        }
    }

    if tags
        .highway
        .is_some_and(|h| matches!(h, NodeHighway::traffic_signals))
    {
        flags |= NodeFlags::TRAFFIC_SIGNALS;
    }

    if tags.toll == Some(true) {
        flags |= NodeFlags::TOLL;
    }

    flags
}

fn parse_dim_m(v: &str) -> Option<f32> {
    let v = v.trim();
    if let Some(rest) = v.find('\'').map(|p| (v[..p].trim(), v[p + 1..].trim())) {
        let feet: f32 = rest.0.parse().ok()?;
        let inches: f32 = rest
            .1
            .strip_suffix('"')
            .unwrap_or(rest.1)
            .trim_end()
            .parse()
            .unwrap_or(0.0);
        return Some(feet * 0.3048 + inches * 0.0254);
    }
    let v = v.strip_suffix('m').unwrap_or(v).trim_end();
    v.parse::<f32>().ok()
}

fn parse_weight_t(v: &str) -> Option<f32> {
    let v = v.trim();
    let v = v.strip_suffix('t').unwrap_or(v).trim_end();
    v.parse::<f32>().ok()
}

fn dim_restriction_from_tags(tags: &tags::WayTags<'_>) -> DimRestriction {
    let height_dm = tags
        .raw_max_height
        .and_then(parse_dim_m)
        .map(|m| (m * 10.0).round() as u8)
        .unwrap_or(0);
    let width_dm = tags
        .raw_max_width
        .and_then(parse_dim_m)
        .map(|m| (m * 10.0).round() as u8)
        .unwrap_or(0);
    let weight_250kg = tags
        .raw_max_weight
        .and_then(parse_weight_t)
        .map(|t| (t * 4.0).round() as u8)
        .unwrap_or(0);
    DimRestriction {
        max_height_dm: height_dm,
        max_width_dm: width_dm,
        max_weight_250kg: weight_250kg,
    }
}

fn is_bicycle_contraflow(tags: &tags::WayTags<'_>) -> bool {
    use tags::{Conditional, Mode, OneWay};
    if let Conditional::Multi(items) = &tags.oneway {
        items
            .iter()
            .any(|i| i.mode == Mode::bicycle && matches!(i.value, OneWay::no))
    } else {
        false
    }
}

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
