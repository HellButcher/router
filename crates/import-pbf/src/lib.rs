pub mod country_lookup;
mod tags;
mod tags_convert;

use osm_pbf_reader::Blobs;
use rayon::iter::{
    IndexedParallelIterator, IntoParallelRefIterator, IntoParallelRefMutIterator, ParallelBridge,
    ParallelIterator,
};
use router_storage::{
    data::{
        self as storage_data,
        attrib::{NodeFlags, TurnFlags, WayFlags},
        edge::EdgeFlags,
        edge_node::{EdgeNode, EdgeNodeChain},
        node::{Node, NodeId},
        pod64::Pod64,
        turn_edge::TurnEdge,
        way::{Way, WayId},
    },
    idindex::IdEntry,
    morton::{DEFAULT_CHUNK_SIZE, sort_by_morton},
    spatial::haversine_m,
    tablefile::TableFile,
};
use router_types::coordinate::LatLon;
use std::{
    collections::HashMap,
    fs::File,
    io::{self, BufReader},
    path::{Path, PathBuf},
    sync::{Mutex, atomic::Ordering},
};
use thiserror::Error;

use crate::tags::{NodeTags, WayTags};

// ── Result container ─────────────────────────────────────────────────────────

/// All data produced by the import pipeline, handed back to the caller.
pub struct ImportResult {
    /// DIrectory where the data structures are stored (e.g. `nodes.bin`, `ways.bin`, etc.).
    pub storage_dir: PathBuf,
    /// Turn restrictions parsed from OSM `type=restriction` relations.
    pub restrictions: Vec<RawRestriction>,
}

// ── Phase 1 helper types ──────────────────────────────────────────────────────

/// A parsed turn restriction from an OSM `type=restriction` relation.
/// Only simple via-node restrictions are captured; complex via-way
/// restrictions are ignored in the initial implementation.
#[derive(Clone, Debug)]
pub struct RawRestriction {
    pub from_way_id: i64,
    pub via_node_id: i64,
    pub to_way_id: i64,
    /// `true` = `only_*` restriction, `false` = `no_*` restriction.
    pub only: bool,
    /// Vehicles affected (0 = all vehicles).
    pub vehicle_mask: EdgeFlags,
}

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

    pub fn with_target_dir(mut self, dir: PathBuf) -> Self {
        self.target_dir = dir;
        self
    }

    pub fn with_country_boundaries(mut self, path: PathBuf) -> Self {
        self.country_boundaries = Some(path);
        self
    }

    pub fn with_maxspeed(mut self, overrides: HashMap<String, u8>) -> Self {
        self.maxspeed = overrides;
        self
    }

    pub fn import(self) -> Result<ImportResult> {
        tracing::info!("importing into {:?}", self.target_dir);
        let _span = tracing::info_span!("import").entered();
        let _ = std::fs::create_dir_all(&self.target_dir);

        let blobs = Blobs::from_buf_read(self.read)?;
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

        let maxspeed_map: HashMap<String, u8> = BUILTIN_MAXSPEED
            .iter()
            .map(|(k, v)| (k.to_string(), *v))
            .chain(self.maxspeed)
            .collect();

        // ── Phase 1a: temp_nodes.bin ─────────────────────────────────────────
        let ways_path = self.target_dir.join("ways.bin");
        let way_nodes_path = self.target_dir.join("way_nodes.bin");

        let mut nodes = TableFile::<Node>::open_override(self.target_dir.join("nodes.bin"))
            .map_err(Error::WriteError)?;
        let mut ways = TableFile::<Way>::open_override(&ways_path).map_err(Error::WriteError)?;
        let mut way_nodes =
            TableFile::<Pod64>::open_override(&way_nodes_path).map_err(Error::WriteError)?;

        let restrictions;
        {
            let _span = tracing::info_span!("parse_blobs").entered();
            let mut nodes_append = nodes.appender().map_err(Error::WriteError)?.spawn_ordered();
            let mut ways_append = ways.appender().map_err(Error::WriteError)?.spawn_ordered();
            let way_nodes_append = way_nodes
                .appender()
                .map_err(Error::WriteError)?
                .spawn_with_index()
                .map_err(Error::WriteError)?;
            // node_ids_per_way[i] = raw OSM node IDs for way i (in ways.bin order, first entry).
            // Owned i64 values, no blob borrows.
            let raw_restrictions: Mutex<Vec<RawRestriction>> = Mutex::new(Vec::new());
            blobs
                .into_iter()
                .map(|b| (nodes_append.start(), ways_append.start(), b))
                .par_bridge()
                .try_for_each(|(nodes_appender, ways_appender, blob)| -> Result<()> {
                    let data = blob?.into_decoded()?;

                    // ── Phase 1a: nodes ───────────────────────────────────
                    let mut out_nodes = Vec::new();
                    let mut old_id = i64::MIN;
                    for group in data.iter_groups() {
                        for n in group.iter_nodes() {
                            let id = NodeId(n.id());
                            assert!(id.0 > old_id);
                            old_id = id.0;
                            let pos = LatLon(n.lat_deg() as f32, n.lon_deg() as f32);
                            let node_tags = NodeTags::parse_tags(n);
                            let node = Node::new(id, pos);
                            node.flags
                                .store(node_tags.derive_node_flags().bits(), Ordering::Relaxed);
                            out_nodes.push(node);
                        }
                        for n in group.iter_dense_nodes() {
                            let id = NodeId(n.id());
                            assert!(id.0 > old_id);
                            old_id = id.0;
                            let pos = LatLon(n.lat_deg() as f32, n.lon_deg() as f32);
                            let node_tags = NodeTags::parse_tags_dense(n);
                            let node = Node::new(id, pos);
                            node.flags
                                .store(node_tags.derive_node_flags().bits(), Ordering::Relaxed);
                            out_nodes.push(node);
                        }
                    }
                    nodes_appender.done(out_nodes);

                    // ── Phase 1b: ways ────────────────────────────────────
                    let mut out_ways: Vec<Way> = Vec::new();
                    let mut way_node_refs: Vec<Pod64> = Vec::new();
                    let mut local_restrictions: Vec<RawRestriction> = Vec::new();
                    for group in data.iter_groups() {
                        for w in group.iter_ways() {
                            let id = w.id();
                            assert!(id > old_id);
                            old_id = id;
                            let id = WayId(id);

                            let way_tags = WayTags::parse_tags(w);
                            if way_tags.is_excluded() {
                                continue;
                            }

                            let highway_class = way_tags.highway_class();
                            let surface_quality = way_tags.surface_quality();
                            let dim = way_tags.derive_dim_restriction();
                            let mut way_flags = way_tags.derive_way_flags();
                            let fwd_access =
                                way_tags.derive_directional_access(highway_class, true);
                            let bwd_access =
                                way_tags.derive_directional_access(highway_class, false);

                            let base_speed = way_tags
                                .raw_max_speed
                                .or(way_tags.raw_max_speed_advisory)
                                .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                                .unwrap_or(0);
                            let fwd_speed = way_tags
                                .raw_max_speed_forward
                                .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                                .unwrap_or(base_speed);
                            let bwd_speed = way_tags
                                .raw_max_speed_backward
                                .and_then(|v| tags::parse_max_speed(v, &maxspeed_map))
                                .unwrap_or(base_speed);

                            let is_oneway = way_flags.contains(WayFlags::ONEWAY);
                            let is_reverse = way_tags.is_oneway_reverse();
                            let bicycle_contraflow = way_tags.is_bicycle_contraflow();

                            // Collect raw node IDs (owned integers, no blob borrow).
                            let node_refs_len = w.refs().len();
                            let node_refs_idx = way_node_refs.len();
                            way_node_refs.reserve(node_refs_len);
                            let last_ref = node_refs_len - 1;
                            for (i, node_ref) in w.refs().enumerate() {
                                let Some((node_index, node)) =
                                    nodes.find(node_ref as u64).ok().flatten()
                                else {
                                    return Err(Error::NodeIdNotFound(NodeId(node_ref)));
                                };
                                way_node_refs.push(Pod64(node_index as u64));
                                if node.num_refs.fetch_add(1, Ordering::AcqRel) == 1 {
                                    // Second reference → this node is an intersection.
                                    node.flags.fetch_or(
                                        NodeFlags::INTERSECTION.bits(),
                                        Ordering::Relaxed,
                                    );
                                }
                                // Mark endpoint flags on nodes.
                                if i == 0 || i == last_ref {
                                    node.flags
                                        .fetch_or(NodeFlags::ENDPOINT.bits(), Ordering::Relaxed);
                                }
                            }

                            // Emit 1–2 Way entries depending on direction requirements.
                            // Convention: DIRECTION_FORWARD = traverse node_ids[0]→[n-1].
                            //             DIRECTION_BACKWARD = traverse node_ids[n-1]→[0].
                            let needs_pair =
                                !is_oneway && (fwd_access != bwd_access || fwd_speed != bwd_speed);
                            let has_fwd_contraflow = is_oneway && !is_reverse && bicycle_contraflow;
                            let has_bwd_contraflow = is_oneway && is_reverse && bicycle_contraflow;

                            if is_oneway && !bicycle_contraflow {
                                // Single-direction oneway.
                                let dir = if is_reverse {
                                    WayFlags::DIRECTION_BACKWARD
                                } else {
                                    WayFlags::DIRECTION_FORWARD
                                };
                                let mut way = Way::new(id);
                                way.highway = highway_class;
                                way.flags = way_flags | dir;
                                way.surface_quality = surface_quality;
                                way.access = if is_reverse { bwd_access } else { fwd_access };
                                way.max_speed = if is_reverse { bwd_speed } else { fwd_speed };
                                way.dim = dim;
                                way.geometry_offset_idx = node_refs_idx as u64;
                                way.geometry_len = node_refs_len as u16;
                                out_ways.push(way);
                            } else if has_fwd_contraflow || has_bwd_contraflow {
                                // Oneway with bicycle contraflow → two entries.
                                way_flags |= WayFlags::HAS_PAIR;
                                let (fwd_acc, bwd_acc) = if has_fwd_contraflow {
                                    (fwd_access, EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV)
                                } else {
                                    (EdgeFlags::NO_MOTOR | EdgeFlags::NO_HGV, bwd_access)
                                };
                                let mut fwd = Way::new(id);
                                fwd.highway = highway_class;
                                fwd.flags = way_flags | WayFlags::DIRECTION_FORWARD;
                                fwd.surface_quality = surface_quality;
                                fwd.access = fwd_acc;
                                fwd.max_speed = fwd_speed;
                                fwd.dim = dim;
                                fwd.geometry_offset_idx = node_refs_idx as u64;
                                fwd.geometry_len = node_refs_len as u16;
                                out_ways.push(fwd);
                                let mut bwd = Way::new(id);
                                bwd.highway = highway_class;
                                bwd.flags = way_flags | WayFlags::DIRECTION_BACKWARD;
                                bwd.surface_quality = surface_quality;
                                bwd.access = bwd_acc;
                                bwd.max_speed = bwd_speed;
                                bwd.dim = dim;
                                bwd.geometry_offset_idx = node_refs_idx as u64;
                                bwd.geometry_len = node_refs_len as u16;
                                out_ways.push(bwd);
                            } else if needs_pair {
                                // Bidirectional with different per-direction properties.
                                way_flags |= WayFlags::HAS_PAIR;
                                let mut fwd = Way::new(id);
                                fwd.highway = highway_class;
                                fwd.flags = way_flags | WayFlags::DIRECTION_FORWARD;
                                fwd.surface_quality = surface_quality;
                                fwd.access = fwd_access;
                                fwd.max_speed = fwd_speed;
                                fwd.dim = dim;
                                fwd.geometry_offset_idx = node_refs_idx as u64;
                                fwd.geometry_len = node_refs_len as u16;
                                out_ways.push(fwd);
                                let mut bwd = Way::new(id);
                                bwd.highway = highway_class;
                                bwd.flags = way_flags | WayFlags::DIRECTION_BACKWARD;
                                bwd.surface_quality = surface_quality;
                                bwd.access = bwd_access;
                                bwd.max_speed = bwd_speed;
                                bwd.dim = dim;
                                bwd.geometry_offset_idx = node_refs_idx as u64;
                                bwd.geometry_len = node_refs_len as u16;
                                out_ways.push(bwd);
                            } else {
                                // Bidirectional, identical properties in both directions.
                                let mut way = Way::new(id);
                                way.highway = highway_class;
                                way.flags = way_flags;
                                way.surface_quality = surface_quality;
                                way.access = fwd_access;
                                way.max_speed = fwd_speed;
                                way.dim = dim;
                                way.geometry_offset_idx = node_refs_idx as u64;
                                way.geometry_len = node_refs_len as u16;
                                out_ways.push(way);
                            }
                        }
                    }

                    if !way_node_refs.is_empty() {
                        let target_index_offset = way_nodes_append.append(way_node_refs);
                        // update node_refs_idx for all ways in this blob
                        for way in &mut out_ways {
                            way.geometry_offset_idx += target_index_offset as u64;
                        }
                    }

                    ways_appender.done(out_ways);

                    // ── Phase 1c: relations ───────────────────────────────
                    for group in data.iter_groups() {
                        for r in group.iter_relations() {
                            let mut restriction_tag: Option<&str> = None;
                            r.tags().iter().for_each(|(k, v)| {
                                if k == "restriction" || k.starts_with("restriction:") {
                                    restriction_tag = Some(v);
                                }
                            });
                            let Some(restriction) = restriction_tag else {
                                continue;
                            };
                            let only = restriction.starts_with("only_");
                            if !only && !restriction.starts_with("no_") {
                                continue;
                            }

                            let mut from_way_id: Option<i64> = None;
                            let mut via_node_id: Option<i64> = None;
                            let mut to_way_id: Option<i64> = None;
                            use osm_pbf_proto::protos::relation::MemberType;
                            for m in r.members() {
                                match (m.member_type, m.role) {
                                    (MemberType::Way, "from") => from_way_id = Some(m.id),
                                    (MemberType::Node, "via") => via_node_id = Some(m.id),
                                    (MemberType::Way, "to") => to_way_id = Some(m.id),
                                    _ => {}
                                }
                            }
                            if let (Some(from), Some(via), Some(to)) =
                                (from_way_id, via_node_id, to_way_id)
                            {
                                local_restrictions.push(RawRestriction {
                                    from_way_id: from,
                                    via_node_id: via,
                                    to_way_id: to,
                                    only,
                                    // TODO: parse vehicle types
                                    vehicle_mask: EdgeFlags::empty(),
                                });
                            }
                        }
                    }

                    raw_restrictions.lock().unwrap().extend(local_restrictions);
                    Ok(())
                })?;

            nodes_append.join().expect("node-writer thread panicked");
            ways_append.join().expect("way-writer thread panicked");
            way_nodes_append.join().expect("way-writer thread panicked");
            restrictions = raw_restrictions.into_inner().unwrap();

            nodes.flush().map_err(Error::WriteError)?;
        }
        let nodes_ref = nodes.get_all().map_err(Error::WriteError)?;
        let nodes_slice: &[Node] = &nodes_ref;

        tracing::info!(
            nodes = nodes.len(),
            ways = ways.len(),
            way_nodes = ways.len(),
            restrictions = restrictions.len(),
            "parsing pbf blobs complete"
        );

        // ── Phase 2a: Build WayId index from already sorted Ways ─────────────
        let mut way_id_index;
        {
            let _span = tracing::info_span!("build_way_id_index").entered();
            let ways_ref = ways.get_all().map_err(Error::WriteError)?;
            let ways_slice: &[Way] = &ways_ref;
            let count = ways_slice.len();
            way_id_index = TableFile::<IdEntry>::create_with_capacity(
                self.target_dir.join("way_id_index.bin"),
                count,
                |entries| {
                    entries.par_iter_mut().enumerate().for_each(|(idx, entry)| {
                        entry.key = ways_slice[idx].id.0 as u64;
                        entry.idx = idx as u64;
                    });
                    Ok(())
                },
            )
            .map_err(Error::WriteError)?;
            way_id_index
                .build_index_sorted()
                .map_err(Error::WriteError)?;
            tracing::info!(count, "way id index written");
        }

        // ── Phase 2b: Morton-sort ways by first geometry point ─────────────
        {
            let _span = tracing::info_span!("morton_sort_ways").entered();
            let reordered_path = self.target_dir.join("ways_reordered.bin");
            {
                let ways_ref = ways.get_all().map_err(Error::WriteError)?;
                let ways_slice: &[Way] = &ways_ref;
                let count = ways_slice.len();
                let way_nodes_ref = way_nodes.get_all().map_err(Error::WriteError)?;
                let way_nodes_slice: &[Pod64] = &way_nodes_ref;
                let scratch = self.target_dir.join("ways.sort.tmp");
                let id_entries = way_id_index.get_all_mut().map_err(Error::WriteError)?;
                let mut new_way_idx: u64 = 0;
                TableFile::<Way>::create_with_capacity(&reordered_path, count, |entries| {
                    sort_by_morton(
                        count,
                        DEFAULT_CHUNK_SIZE,
                        |i| {
                            let way = &ways_slice[i];
                            let node_idx =
                                way_nodes_slice[way.geometry_offset_idx as usize].0 as usize;
                            nodes_slice[node_idx].pos.into()
                        },
                        &scratch,
                        |old_idx| {
                            entries[new_way_idx as usize] =
                                unsafe { std::ptr::read(&ways_slice[old_idx as usize]) };
                            id_entries[old_idx as usize].idx = new_way_idx;
                            new_way_idx += 1;
                            Ok(())
                        },
                    )
                })
                .map_err(Error::WriteError)?;
            }
            way_id_index.flush().map_err(Error::WriteError)?;

            drop(ways);
            std::fs::rename(&reordered_path, &ways_path).map_err(Error::WriteError)?;

            tracing::info!("ways Morton-sorted");
        }
        // re-open ways (now in Morton order) for subsequent phases
        let mut ways = TableFile::<Way>::open(&ways_path).map_err(Error::WriteError)?;

        // ── Phase 3: geometry.bin + edge_nodes.bin ────────────────────────
        // ── 3a: reorder way_nodes + write geometry + update way.node_refs_idx ──
        let geometry_path = self.target_dir.join("geometry.bin");
        let mut geometry;
        {
            let _span = tracing::info_span!("reorder_way_nodes_and_geometry").entered();
            let way_nodes_reordered_path = self.target_dir.join("way_nodes_reordered.bin");
            {
                let mut new_way_nodes =
                    TableFile::<Pod64>::open_override(&way_nodes_reordered_path)
                        .map_err(Error::WriteError)?;
                geometry = TableFile::<LatLon>::open_override(&geometry_path)
                    .map_err(Error::WriteError)?;

                let mut new_way_nodes_appender =
                    new_way_nodes.appender().map_err(Error::WriteError)?;
                let mut geometry_appender = geometry.appender().map_err(Error::WriteError)?;

                let ways_mut = ways.get_all_mut().map_err(Error::WriteError)?;
                let way_nodes_ref = way_nodes.get_all().map_err(Error::WriteError)?;
                let way_nodes_slice: &[Pod64] = &way_nodes_ref;

                let mut node_indices_buf: Vec<Pod64> = Vec::new();
                let mut geometry_buf: Vec<LatLon> = Vec::new();

                let mut geo_offset: u64 = 0;
                for way in ways_mut.iter_mut() {
                    let old_start = way.geometry_offset_idx as usize;
                    let len = way.geometry_len as usize;

                    if way.flags.contains(WayFlags::HAS_PAIR)
                        && way.flags.contains(WayFlags::DIRECTION_BACKWARD)
                    {
                        way.geometry_offset_idx = geo_offset - len as u64;
                        continue;
                    }

                    node_indices_buf.clear();
                    geometry_buf.clear();
                    for j in 0..len {
                        let pod = way_nodes_slice[old_start + j];
                        node_indices_buf.push(pod);
                        geometry_buf.push(nodes_slice[pod.0 as usize].pos);
                    }
                    new_way_nodes_appender
                        .append(&node_indices_buf)
                        .map_err(Error::WriteError)?;
                    geometry_appender
                        .append(&geometry_buf)
                        .map_err(Error::WriteError)?;

                    way.geometry_offset_idx = geo_offset;
                    geo_offset += len as u64;
                }

                new_way_nodes.flush().map_err(Error::WriteError)?;
                geometry.flush().map_err(Error::WriteError)?;
                ways.flush().map_err(Error::WriteError)?;
            }

            drop(way_nodes);
            std::fs::rename(&way_nodes_reordered_path, &way_nodes_path)
                .map_err(Error::WriteError)?;

            tracing::info!("geometry and way_nodes reordered and written");
        }
        // re-open way_nodes (now in updated order) for subsequent phases
        let way_nodes = TableFile::<Pod64>::open(&way_nodes_path).map_err(Error::WriteError)?;
        let way_nodes_ref = way_nodes.get_all().map_err(Error::WriteError)?;
        let way_nodes_slice: &[Pod64] = &way_nodes_ref;
        let geometry_ref = geometry.get_all().map_err(Error::WriteError)?;
        let geometry_slice: &[LatLon] = &geometry_ref;
        let ways_ref = ways.get_all().map_err(Error::WriteError)?;
        let ways_slice: &[Way] = &ways_ref;

        let country_lookup = match &self.country_boundaries {
            Some(p) => {
                let _span = tracing::info_span!("build_country_lookup").entered();
                let lookup = country_lookup::CountryLookup::from_file(p)
                    .map_err(|e| Error::CountryLookup(Box::new(e)))?;
                Some(lookup)
            }
            None => None,
        };

        // ── 3b: edge_nodes.bin (parallel) ────────────────────────────
        let edge_nodes_path = self.target_dir.join("edge_nodes.bin");
        let mut edge_nodes;
        {
            let _span = tracing::info_span!("build_edge_nodes").entered();

            edge_nodes = TableFile::<EdgeNode>::open_override(&edge_nodes_path)
                .map_err(Error::WriteError)?;
            let edge_nodes_appender = edge_nodes
                .appender()
                .map_err(Error::WriteError)?
                .spawn_with_index()
                .map_err(Error::WriteError)?;

            ways_slice
                .par_iter()
                .enumerate()
                .try_for_each(|(way_idx, way)| -> Result<()> {
                    let flags = way.flags;
                    let refs_start = way.geometry_offset_idx as usize;
                    let refs_len = way.geometry_len as usize;

                    let (fwd_way_idx, bwd_way_idx): (Option<usize>, Option<usize>) =
                        if flags.contains(WayFlags::HAS_PAIR) {
                            if flags.contains(WayFlags::DIRECTION_BACKWARD) {
                                (None, Some(way_idx))
                            } else {
                                (Some(way_idx), Some(way_idx + 1))
                            }
                        } else if way.is_forward() && way.is_backward() {
                            (Some(way_idx), Some(way_idx))
                        } else if way.is_forward() {
                            (Some(way_idx), None)
                        } else {
                            (None, Some(way_idx))
                        };

                    let node_refs = &way_nodes_slice[refs_start..refs_start + refs_len];
                    let mut out = Vec::new();
                    let mut seg_start = 0usize;

                    for i in 1..refs_len {
                        let node_flags = nodes_slice[node_refs[i].0 as usize].node_flags();
                        if i != refs_len - 1 && node_flags.is_empty() {
                            continue;
                        }

                        let seg_len = (i - seg_start + 1) as i16;
                        let seg_geom_from = refs_start as u64 + seg_start as u64;

                        let mut dist_m: u32 = 0;
                        let mut prev = nodes_slice[node_refs[seg_start].0 as usize].pos;
                        for j in seg_start + 1..=i {
                            let pos = nodes_slice[node_refs[j].0 as usize].pos;
                            dist_m = dist_m.saturating_add(haversine_m(
                                prev.lat, prev.lon, pos.lat, pos.lon,
                            ) as u32);
                            prev = pos;
                        }

                        let from_pos = nodes_slice[node_refs[seg_start].0 as usize].pos;
                        let to_pos = nodes_slice[node_refs[i].0 as usize].pos;

                        let fwd_country = country_lookup
                            .as_ref()
                            .map(|cl| cl.lookup(from_pos.lat, from_pos.lon))
                            .unwrap_or(router_types::country::CountryId::UNKNOWN);
                        let bwd_country = country_lookup
                            .as_ref()
                            .map(|cl| cl.lookup(to_pos.lat, to_pos.lon))
                            .unwrap_or(router_types::country::CountryId::UNKNOWN);

                        if let Some(fw) = fwd_way_idx {
                            out.push(EdgeNode::new(
                                fw as u64,
                                dist_m,
                                fwd_country,
                                seg_geom_from,
                                seg_len,
                            ));
                        }
                        if let Some(bw) = bwd_way_idx {
                            out.push(EdgeNode::new(
                                bw as u64,
                                dist_m,
                                bwd_country,
                                seg_geom_from,
                                -seg_len,
                            ));
                        }

                        seg_start = i;
                    }

                    if !out.is_empty() {
                        edge_nodes_appender.append(out);
                    }
                    Ok(())
                })?;

            edge_nodes_appender
                .join()
                .expect("edge-node writer panicked");
            edge_nodes.flush().map_err(Error::WriteError)?;
            tracing::info!(edge_nodes = edge_nodes.len(), "edge_nodes.bin written");
        }
        drop(country_lookup); // country lookup not needed beyond this point

        // ── Phase 4: Morton-sort EdgeNodes ────────────────────────────────────
        {
            let _span = tracing::info_span!("sort_edge_nodes").entered();
            let reordered_path = self.target_dir.join("edge_nodes_reordered.bin");
            let count;
            {
                let edge_nodes_ref = edge_nodes.get_all().map_err(Error::WriteError)?;
                let edge_nodes_slice: &[EdgeNode] = &edge_nodes_ref;
                count = edge_nodes_slice.len();
                let scratch = self.target_dir.join("edge_nodes.sort.tmp");
                TableFile::<EdgeNode>::create_with_capacity(&reordered_path, count, |entries| {
                    let mut new_idx: usize = 0;
                    sort_by_morton(
                        count,
                        DEFAULT_CHUNK_SIZE,
                        |i| {
                            let pos = geometry_slice[edge_nodes_slice[i].geometry_from_idx()];
                            (pos.lat, pos.lon)
                        },
                        &scratch,
                        |old_idx| {
                            entries[new_idx] =
                                unsafe { std::ptr::read(&edge_nodes_slice[old_idx as usize]) };
                            new_idx += 1;
                            Ok(())
                        },
                    )
                })
                .map_err(Error::WriteError)?;
            }
            drop(edge_nodes);
            std::fs::rename(&reordered_path, &edge_nodes_path).map_err(Error::WriteError)?;
            tracing::info!(count, "edge_nodes Morton-sorted");
        }
        // re-open edge_nodes (now in Morton order) for subsequent phases
        let edge_nodes =
            TableFile::<EdgeNode>::open(&edge_nodes_path).map_err(Error::WriteError)?;
        let edge_nodes_ref = edge_nodes.get_all().map_err(Error::WriteError)?;
        let edge_nodes_slice: &[EdgeNode] = &edge_nodes_ref;

        // ── Phase 4b: link sorted EdgeNodes into per-node lists ───────────────
        // Builds `edge_node_chains.bin`: one EdgeNodeChain per EdgeNode holding
        // the "next" chain pointers for the two per-node linked lists.
        // The Node's first_outgoing/incoming_edge_node_idx heads are set here.
        let chains_path = self.target_dir.join("edge_node_chains.bin");
        let chains;
        {
            let _span = tracing::info_span!("link_edge_nodes_to_nodes").entered();
            let count = edge_nodes_slice.len();

            // Use a separate file handle for nodes so the outer nodes_ref read
            // lock is not in conflict (both are MAP_SHARED, writes are visible).
            chains =
                TableFile::<EdgeNodeChain>::create_with_capacity(&chains_path, count, |entries| {
                    entries
                        .par_iter_mut()
                        .enumerate()
                        .for_each(|(en_idx, chain)| {
                            *chain = EdgeNodeChain::default();
                            let en = &edge_nodes_slice[en_idx];
                            storage_data::link_edge_node_to_nodes(
                                &nodes_slice[way_nodes_slice[en.geometry_from_idx()].0 as usize],
                                &nodes_slice[way_nodes_slice[en.geometry_to_idx()].0 as usize],
                                en_idx as u64,
                                chain,
                            );
                        });
                    Ok(())
                })
                .map_err(Error::WriteError)?;
            nodes.flush().map_err(Error::WriteError)?;
            chains.flush().map_err(Error::WriteError)?;
            tracing::info!(count, "edge_nodes linked into node lists");
        }

        // ── Phase 5: build TurnEdges ──────────────────────────────────────────
        // Iterate EdgeNodes in parallel (already Morton-sorted).
        // For each outgoing EdgeNode `y`, collect all incoming EdgeNodes `x` at
        // `y`'s from-node and emit one TurnEdge per (x, y) pair.
        // TurnEdges are written in EdgeNode (Morton) order via spawn_with_index.
        let turn_edges_path = self.target_dir.join("turn_edges.bin");
        let mut turn_edges;
        {
            let _span = tracing::info_span!("build_turn_edges").entered();
            let chains_ref = chains.get_all().map_err(Error::WriteError)?;
            let chains_slice: &[EdgeNodeChain] = &chains_ref;

            let restriction_by_via: HashMap<i64, Vec<usize>> = {
                let mut m: HashMap<i64, Vec<usize>> = HashMap::new();
                for (i, r) in restrictions.iter().enumerate() {
                    m.entry(r.via_node_id).or_default().push(i);
                }
                m
            };

            turn_edges = TableFile::<TurnEdge>::open_override(&turn_edges_path)
                .map_err(Error::WriteError)?;
            let job = turn_edges
                .appender()
                .map_err(Error::WriteError)?
                .spawn_with_index()
                .map_err(Error::WriteError)?;

            edge_nodes_slice.par_iter().enumerate().for_each(|(yi, y)| {
                // from-node of y = via-node for all (x → y) turns
                let via_node = &nodes_slice[way_nodes_slice[y.geometry_from_idx()].0 as usize];
                let turn_flags = node_flags_to_turn_flags(via_node.node_flags());
                let applicable = restriction_by_via.get(&via_node.id.0);

                // Collect all EdgeNodes incoming to the via-node.
                let mut batch: Vec<TurnEdge> = Vec::new();
                let mut xi = via_node.first_incoming_edge_node_idx();
                while xi != usize::MAX {
                    if xi != yi {
                        let x = &edge_nodes_slice[xi];
                        let mask = restriction_mask(&restrictions, applicable, x, y, ways_slice);
                        let angle = turn_angle(x, y, geometry_slice);
                        batch.push(TurnEdge::new(xi as u64, yi as u64, angle, mask, turn_flags));
                    }
                    xi = chains_slice[xi].next_incoming();
                }

                if batch.is_empty() {
                    return;
                }

                // Reserve a contiguous index range, link (updating te.next_*),
                // then send the batch with correct next pointers to the writer.
                let base_idx = job.reserve(batch.len());
                for (offset, te) in batch.iter().enumerate() {
                    storage_data::link_turn_edge(
                        &edge_nodes_slice[te.from_edge_node_idx()],
                        y,
                        (base_idx + offset) as u64,
                        te,
                    );
                }
                job.append_reserved(base_idx, batch);
            });

            let turn_count = job.len();
            job.join().map_err(Error::WriteError)?;
            turn_edges.flush().map_err(Error::WriteError)?;
            edge_nodes.flush().map_err(Error::WriteError)?;
            tracing::info!(turn_edges = turn_count, "turn_edges.bin written");
        }

        std::fs::remove_file(&chains_path).map_err(Error::WriteError)?;

        // ── Phase 6: edge spatial index ───────────────────────────────────────
        {
            let _span = tracing::info_span!("build_edge_spatial_index").entered();
            let count = edge_nodes_slice.len();
            // EdgeNodes are already Morton-sorted, so build_presorted avoids a
            // redundant external sort.
            router_storage::spatial::SpatialIndexBuilder::new()
                .build_presorted(
                    count,
                    |i| {
                        let en = &edge_nodes_slice[i];
                        let range = en.geometry_range();
                        let pts = &geometry_slice[range];
                        let (mut min_lat, mut min_lon) = (f32::INFINITY, f32::INFINITY);
                        let (mut max_lat, mut max_lon) = (f32::NEG_INFINITY, f32::NEG_INFINITY);
                        for p in pts {
                            min_lat = min_lat.min(p.lat);
                            min_lon = min_lon.min(p.lon);
                            max_lat = max_lat.max(p.lat);
                            max_lon = max_lon.max(p.lon);
                        }
                        (min_lat, min_lon, max_lat, max_lon)
                    },
                    self.target_dir.join("edge_node_spatial.bin"),
                )
                .map_err(Error::WriteError)?;
            tracing::info!(count, "edge_node_spatial.bin written");
        }

        // ── Cleanup: remove import-only files not needed at query time ────────
        std::fs::remove_file(&way_nodes_path).map_err(Error::WriteError)?;
        std::fs::remove_file(self.target_dir.join("nodes.bin")).map_err(Error::WriteError)?;

        Ok(ImportResult {
            storage_dir: self.target_dir,
            restrictions,
        })
    }
}

// ── Import helpers ────────────────────────────────────────────────────────────

fn node_flags_to_turn_flags(flags: NodeFlags) -> TurnFlags {
    let mut tf = TurnFlags::empty();
    if flags.contains(NodeFlags::TRAFFIC_SIGNALS) {
        tf |= TurnFlags::TRAFFIC_SIGNALS;
    }
    if flags.contains(NodeFlags::TOLL) {
        tf |= TurnFlags::TOLL;
    }
    tf
}

fn restriction_mask(
    restrictions: &[RawRestriction],
    applicable: Option<&Vec<usize>>,
    x: &EdgeNode,
    y: &EdgeNode,
    ways: &[Way],
) -> EdgeFlags {
    const ALL: EdgeFlags = EdgeFlags::NO_MOTOR
        .union(EdgeFlags::NO_HGV)
        .union(EdgeFlags::NO_BICYCLE)
        .union(EdgeFlags::NO_FOOT);
    let Some(indices) = applicable else {
        return EdgeFlags::empty();
    };
    let x_way_id = ways[x.way_idx as usize].id.0;
    let y_way_id = ways[y.way_idx as usize].id.0;
    let mut mask = EdgeFlags::empty();
    for &ri in indices {
        let r = &restrictions[ri];
        let effective = if r.vehicle_mask.is_empty() {
            ALL
        } else {
            r.vehicle_mask
        };
        if r.only {
            if r.from_way_id == x_way_id && r.to_way_id != y_way_id {
                mask |= effective;
            }
        } else if r.from_way_id == x_way_id && r.to_way_id == y_way_id {
            mask |= effective;
        }
    }
    mask
}

fn bearing(from: LatLon, to: LatLon) -> f32 {
    let lat1 = from.lat.to_radians();
    let lat2 = to.lat.to_radians();
    let dlon = (to.lon - from.lon).to_radians();
    let y = dlon.sin() * lat2.cos();
    let x = lat1.cos() * lat2.sin() - lat1.sin() * lat2.cos() * dlon.cos();
    y.atan2(x).to_degrees()
}

fn turn_angle(x: &EdgeNode, y: &EdgeNode, geom: &[LatLon]) -> i16 {
    let xg = x.geometry_from_idx();
    let xc = x.geometry_count();
    let (xa, xb) = if x.geometry_len > 0 {
        (geom[xg + xc - 2], geom[xg + xc - 1])
    } else {
        (geom[xg + 1], geom[xg])
    };
    let yg = y.geometry_from_idx();
    let yc = y.geometry_count();
    let (ya, yb) = if y.geometry_len > 0 {
        (geom[yg], geom[yg + 1])
    } else {
        (geom[yg + yc - 1], geom[yg + yc - 2])
    };
    let diff = bearing(ya, yb) - bearing(xa, xb);
    ((diff + 540.0) % 360.0 - 180.0) as i16
}
