/// Packed R-tree (Flatbush-style) spatial index for nearest-node queries.
///
/// Built once at import time, stored on disk, queried via `mmap`.
///
/// # Memory model during build
///
/// Building delegates to [`extsort::sort_and_merge`] which holds only
/// `chunk_size × 16 bytes` in RAM at once. Chunk filling and sorting run in
/// parallel via rayon. Upper tree levels are built by iterating over the
/// already-written output mapping — no extra allocations.
///
/// # File layout
///
/// ```text
/// [Header:   512 bytes on disk (struct is smaller; rest is zeros)]
/// [Level 0:  num_items leaf entries × 24 bytes]   ← sorted by Z-order curve
/// [Level 1:  ceil(num_items / node_size) × 24 bytes]
/// …
/// [Level N:  1 … node_size root entries]
/// ```
///
/// Each entry: `[min_lat, min_lon, max_lat, max_lon: f32; index: u64]` = 24 bytes.
/// Leaf `index` = position in the node table.
/// Internal `index` = 0; children are implicit by position.
use std::{
    cmp::Reverse,
    collections::BinaryHeap,
    fs::{self, File, OpenOptions},
    io,
    mem::size_of,
    path::Path,
};

use memmap2::{Advice, MmapMut, MmapOptions, MmapRaw};

use crate::pod::TablePod;

// ── constants ─────────────────────────────────────────────────────────────────

const MAGIC: u64 = 0x5350_4154_5f49_4458; // b"SPAT_IDX"
// Data format version. Increment on any non-compatible change (eg: adding fields).
const VERSION: u32 = 1;
const MAX_LEVELS: usize = 16;
/// Default branching factor.
pub const DEFAULT_NODE_SIZE: u32 = 16;
/// On-disk header size; struct is smaller, remainder is zeros.
const HEADER_DISK_SIZE: usize = 512;
pub use crate::morton::DEFAULT_CHUNK_SIZE;

// ── on-disk types ─────────────────────────────────────────────────────────────

/// One node in the flat tree array (24 bytes, 8-byte aligned).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct RTreeEntry {
    pub min_lat: f32,
    pub min_lon: f32,
    pub max_lat: f32,
    pub max_lon: f32,
    /// Leaf: payload index (node table index for node spatial index,
    /// way table index for edge spatial index).  Internal: 0 (unused).
    pub index: u64,
}

unsafe impl TablePod for RTreeEntry {}

const _: () = {
    assert!(size_of::<RTreeEntry>() == 24);
    assert!(size_of::<RTreeEntry>().is_multiple_of(std::mem::align_of::<RTreeEntry>()));
};

/// File header (no trailing padding field; zeros appended during write).
#[repr(C)]
struct SpatialIndexHeader {
    magic: u64,
    version: u32,
    node_size: u32,
    num_items: u64,
    num_levels: u32,
    _reserved: u32,
    /// Absolute byte offset of level `i`'s first entry in the file.
    level_offsets: [u64; MAX_LEVELS],
}

unsafe impl TablePod for SpatialIndexHeader {}

const _: () = {
    assert!(size_of::<SpatialIndexHeader>() == 8 + 4 + 4 + 8 + 4 + 4 + MAX_LEVELS * 8);
    assert!(size_of::<SpatialIndexHeader>() <= HEADER_DISK_SIZE);
};

// ── read-only index ───────────────────────────────────────────────────────────

pub struct SpatialIndex {
    mmap: MmapRaw,
    num_items: usize,
    node_size: usize,
    num_levels: usize,
    level_starts: [usize; MAX_LEVELS],
}

impl SpatialIndex {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        Self::from_file(OpenOptions::new().read(true).open(path)?)
    }

    pub fn from_file(file: File) -> io::Result<Self> {
        let file_len = file.metadata()?.len() as usize;
        if file_len < HEADER_DISK_SIZE {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        let mmap = MmapOptions::new().len(file_len).map_raw_read_only(&file)?;
        mmap.advise(Advice::Random)?;

        let hdr = unsafe { &*(mmap.as_ptr() as *const SpatialIndexHeader) };
        if hdr.magic != MAGIC || hdr.version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bad spatial index header",
            ));
        }
        let num_levels = hdr.num_levels as usize;
        if num_levels > MAX_LEVELS {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid num_levels",
            ));
        }

        let mut level_starts = [0usize; MAX_LEVELS];
        for (i, &off) in hdr.level_offsets[..num_levels].iter().enumerate() {
            let byte_off = off as usize;
            if byte_off < HEADER_DISK_SIZE
                || !(byte_off - HEADER_DISK_SIZE).is_multiple_of(size_of::<RTreeEntry>())
            {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "bad level offset",
                ));
            }
            level_starts[i] = (byte_off - HEADER_DISK_SIZE) / size_of::<RTreeEntry>();
        }

        Ok(Self {
            mmap,
            num_items: hdr.num_items as usize,
            node_size: hdr.node_size as usize,
            num_levels,
            level_starts,
        })
    }

    fn entries(&self) -> &[RTreeEntry] {
        let ptr = unsafe { self.mmap.as_ptr().add(HEADER_DISK_SIZE) as *const RTreeEntry };
        let len = (self.mmap.len() - HEADER_DISK_SIZE) / size_of::<RTreeEntry>();
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }

    fn level_start(&self, level: usize) -> usize {
        self.level_starts[level]
    }

    fn level_len(&self, level: usize) -> usize {
        if level + 1 < self.num_levels {
            self.level_starts[level + 1] - self.level_starts[level]
        } else {
            self.entries().len() - self.level_starts[level]
        }
    }

    /// Find the nearest item within `max_radius_m` metres of `(lat, lon)`.
    ///
    /// The R-tree traversal uses [`min_dist_to_bbox_m`] as an admissible lower
    /// bound on the true distance to any item inside a bounding box.  When a
    /// leaf entry is dequeued, `refine(entry, bbox_dist)` is called to compute
    /// the exact distance and produce the result payload.  Return `None` from
    /// `refine` to skip the entry.
    ///
    /// Because bbox distance ≤ true distance, the search continues until the
    /// heap minimum exceeds the best true distance found so far — guaranteeing
    /// the global nearest result even when leaf bboxes are larger than points
    /// (e.g. way segments).
    pub fn nearest_refined<T>(
        &self,
        lat: f32,
        lon: f32,
        max_radius_m: f32,
        refine: impl Fn(&RTreeEntry, f32) -> Option<(f32, T)>,
    ) -> Option<T> {
        if self.num_items == 0 || self.num_levels == 0 {
            return None;
        }
        let entries = self.entries();
        // Min-heap keyed on IEEE 754 distance bits (positive floats compare
        // correctly as u32 bit patterns).
        let mut heap: BinaryHeap<(Reverse<u32>, usize, usize)> = BinaryHeap::new();
        let mut best: Option<(f32, T)> = None;

        let root = self.num_levels - 1;
        let root_start = self.level_start(root);
        for (i, e) in entries[root_start..root_start + self.level_len(root)]
            .iter()
            .enumerate()
        {
            let d = min_dist_to_bbox_m(lat, lon, e);
            if d <= max_radius_m {
                heap.push((Reverse(d.to_bits()), root_start + i, root));
            }
        }

        while let Some((Reverse(bits), idx, level)) = heap.pop() {
            let bbox_dist = f32::from_bits(bits);
            // Lower bound already exceeds the best true distance: done.
            let cutoff = best.as_ref().map_or(max_radius_m, |(d, _)| *d);
            if bbox_dist > cutoff {
                break;
            }
            let e = &entries[idx];
            if level == 0 {
                if let Some((true_dist, payload)) = refine(e, bbox_dist)
                    && true_dist <= cutoff
                {
                    best = Some((true_dist, payload));
                }
                continue;
            }
            let child_level = level - 1;
            let local = idx - self.level_start(level);
            let child_start = self.level_start(child_level) + local * self.node_size;
            let child_end = (child_start + self.node_size).min(self.level_start(level));
            for (ci, ce) in entries[child_start..child_end].iter().enumerate() {
                let d = min_dist_to_bbox_m(lat, lon, ce);
                if d <= cutoff {
                    heap.push((Reverse(d.to_bits()), child_start + ci, child_level));
                }
            }
        }
        best.map(|(_, payload)| payload)
    }

    /// Find the nearest **node** within `max_radius_m` metres of `(lat, lon)`.
    ///
    /// Returns `(node_table_index, snapped_lat, snapped_lon, distance_m)`.
    /// Node leaf bboxes are points (`min == max`), so bbox distance equals true
    /// distance and the first leaf dequeued is always the nearest.
    pub fn nearest(&self, lat: f32, lon: f32, max_radius_m: f32) -> Option<(u64, f32, f32, f32)> {
        self.nearest_refined(lat, lon, max_radius_m, |e, d| {
            Some((d, (e.index, e.min_lat, e.min_lon, d)))
        })
    }
}

// ── builder ───────────────────────────────────────────────────────────────────

/// Builds a packed R-tree using external merge sort with mmap I/O throughout.
pub struct SpatialIndexBuilder {
    node_size: u32,
    chunk_size: usize,
}

impl SpatialIndexBuilder {
    pub fn new() -> Self {
        Self::with_options(DEFAULT_NODE_SIZE, DEFAULT_CHUNK_SIZE)
    }

    pub fn with_options(node_size: u32, chunk_size: usize) -> Self {
        assert!(node_size >= 2);
        assert!(chunk_size >= 1);
        Self {
            node_size,
            chunk_size,
        }
    }

    /// Build the index and write it to `path`.
    ///
    /// `get_bbox(i)` returns `(min_lat, min_lon, max_lat, max_lon)` for item
    /// `i`.  For point items (nodes) pass `(lat, lon, lat, lon)`.  For line
    /// segments (ways) pass the segment's bounding box.  Morton-curve ordering
    /// uses the bbox centre, so spatially close items end up in the same leaf.
    ///
    /// The closure may be a reference to a mmap'd slice — no bulk copy of
    /// coordinates is performed.  Must be `Sync` so rayon can share it across
    /// chunk-sort threads.
    pub fn build<P, F>(&self, count: usize, get_bbox: F, path: P) -> io::Result<()>
    where
        F: Fn(usize) -> (f32, f32, f32, f32) + Sync,
        P: AsRef<Path>,
    {
        let path = path.as_ref();
        let out = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;
        let run_path = path.with_extension(format!("sort_{}.tmp", std::process::id()));
        build_impl(
            self.node_size as usize,
            self.chunk_size,
            count,
            &get_bbox,
            out,
            &run_path,
        )
    }
}

impl Default for SpatialIndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── build implementation ──────────────────────────────────────────────────────

fn build_impl<F>(
    node_size: usize,
    chunk_size: usize,
    count: usize,
    get_bbox: &F,
    file: File,
    scratch: &Path,
) -> io::Result<()>
where
    F: Fn(usize) -> (f32, f32, f32, f32) + Sync,
{
    // ── layout ────────────────────────────────────────────────────────────────
    let level_lens = compute_level_lens(count, node_size);
    let num_levels = level_lens.len();
    let mut level_offsets = [0u64; MAX_LEVELS];
    let mut off = HEADER_DISK_SIZE;
    for (i, &len) in level_lens.iter().enumerate() {
        level_offsets[i] = off as u64;
        off += len * size_of::<RTreeEntry>();
    }
    let file_size = off;

    // ── pre-allocate & mmap the output file ───────────────────────────────────
    file.set_len(file_size as u64)?;
    let mut out = unsafe { MmapMut::map_mut(&file) }?;

    // ── header ────────────────────────────────────────────────────────────────
    let hdr = SpatialIndexHeader {
        magic: MAGIC,
        version: VERSION,
        node_size: node_size as u32,
        num_items: count as u64,
        num_levels: num_levels as u32,
        _reserved: 0,
        level_offsets,
    };
    let hdr_bytes = unsafe {
        std::slice::from_raw_parts(
            &hdr as *const SpatialIndexHeader as *const u8,
            size_of::<SpatialIndexHeader>(),
        )
    };
    out[..size_of::<SpatialIndexHeader>()].copy_from_slice(hdr_bytes);
    out[size_of::<SpatialIndexHeader>()..HEADER_DISK_SIZE].fill(0);

    if count == 0 {
        return out.flush();
    }

    // ── phase 1 + 2: external sort → level-0 entries ─────────────────────────
    let _span_sort = tracing::info_span!("spatial_sort_and_merge", count).entered();
    {
        let level0_off = level_offsets[0] as usize;
        let level0 = unsafe {
            std::slice::from_raw_parts_mut(
                out[level0_off..].as_mut_ptr() as *mut RTreeEntry,
                level_lens[0],
            )
        };
        let mut out_idx = 0usize;
        crate::morton::sort_by_morton(
            count,
            chunk_size,
            |i| {
                let (min_lat, min_lon, max_lat, max_lon) = get_bbox(i);
                ((min_lat + max_lat) * 0.5, (min_lon + max_lon) * 0.5)
            },
            scratch,
            |idx| {
                let (min_lat, min_lon, max_lat, max_lon) = get_bbox(idx as usize);
                level0[out_idx] = RTreeEntry {
                    min_lat,
                    min_lon,
                    max_lat,
                    max_lon,
                    index: idx,
                };
                out_idx += 1;
                Ok(())
            },
        )?;
    }

    drop(_span_sort);
    // ── phase 3: upper levels from the mmap'd output ──────────────────────────
    let _span_upper = tracing::info_span!("spatial_build_upper_levels", num_levels).entered();
    // Two non-overlapping slices into `out`: prev (read) and curr (write).
    for lv in 1..num_levels {
        let prev_off = level_offsets[lv - 1] as usize;
        let prev_len = level_lens[lv - 1];
        let curr_off = level_offsets[lv] as usize;
        let curr_len = level_lens[lv];

        let (prev, curr) = unsafe {
            let prev =
                std::slice::from_raw_parts(out[prev_off..].as_ptr() as *const RTreeEntry, prev_len);
            let curr = std::slice::from_raw_parts_mut(
                out[curr_off..].as_mut_ptr() as *mut RTreeEntry,
                curr_len,
            );
            (prev, curr)
        };

        for (i, chunk) in prev.chunks(node_size).enumerate() {
            let mut min_lat = f32::INFINITY;
            let mut min_lon = f32::INFINITY;
            let mut max_lat = f32::NEG_INFINITY;
            let mut max_lon = f32::NEG_INFINITY;
            for e in chunk {
                min_lat = min_lat.min(e.min_lat);
                min_lon = min_lon.min(e.min_lon);
                max_lat = max_lat.max(e.max_lat);
                max_lon = max_lon.max(e.max_lon);
            }
            curr[i] = RTreeEntry {
                min_lat,
                min_lon,
                max_lat,
                max_lon,
                index: 0,
            };
        }
    }

    out.flush()
}

// ── helpers ───────────────────────────────────────────────────────────────────

fn compute_level_lens(num_items: usize, node_size: usize) -> Vec<usize> {
    let mut lens = vec![num_items];
    while *lens.last().unwrap() > node_size {
        lens.push(lens.last().unwrap().div_ceil(node_size));
    }
    lens
}

// ── geometry ──────────────────────────────────────────────────────────────────

fn min_dist_to_bbox_m(lat: f32, lon: f32, e: &RTreeEntry) -> f32 {
    haversine_m(
        lat,
        lon,
        lat.clamp(e.min_lat, e.max_lat),
        lon.clamp(e.min_lon, e.max_lon),
    )
}

pub fn haversine_m(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    const R: f32 = 6_371_000.0;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    R * 2.0 * a.sqrt().asin()
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn build(items: &[(f32, f32, u64)], chunk_size: usize) -> (SpatialIndex, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("spatial.bin");
        SpatialIndexBuilder::with_options(DEFAULT_NODE_SIZE, chunk_size)
            .build(
                items.len(),
                |i| (items[i].0, items[i].1, items[i].0, items[i].1),
                &path,
            )
            .unwrap();
        (SpatialIndex::open(&path).unwrap(), dir)
    }

    #[test]
    fn test_empty() {
        let (idx, _dir) = build(&[], 16);
        assert_eq!(idx.nearest(0.0, 0.0, 1_000_000.0), None);
    }

    #[test]
    fn test_single_hit() {
        let (idx, _dir) = build(&[(52.5, 13.4, 0)], 16);
        let r = idx.nearest(52.5, 13.4, 1.0).unwrap();
        assert_eq!(r.0, 0); // builder stores sequential position i as node-table index
    }

    #[test]
    fn test_single_miss() {
        let (idx, _dir) = build(&[(52.5, 13.4, 42)], 16);
        assert_eq!(idx.nearest(52.509, 13.4, 100.0), None);
    }

    #[test]
    fn test_nearest_among_many() {
        let items: Vec<(f32, f32, u64)> = (0u64..1000)
            .map(|i| (48.0 + i as f32 * 0.001, 11.0 + i as f32 * 0.001, i))
            .collect();
        let (idx, _dir) = build(&items, 16);
        assert_eq!(idx.nearest(48.5, 11.5, 1.0).map(|r| r.0), Some(500));
    }

    #[test]
    fn test_nearest_closest_chosen() {
        let items = [(48.0f32, 11.0f32, 0u64), (48.001, 11.0, 1)];
        let (idx, _dir) = build(&items, 16);
        assert_eq!(idx.nearest(48.0009, 11.0, 1000.0).map(|r| r.0), Some(1));
    }

    /// chunk_size=10 forces 20 runs for 200 items — exercises external sort.
    #[test]
    fn test_external_sort_path() {
        let items: Vec<(f32, f32, u64)> = (0u64..200)
            .map(|i| (48.0 + i as f32 * 0.001, 11.0 + i as f32 * 0.001, i))
            .collect();
        let (idx, _dir) = build(&items, 10);
        assert_eq!(idx.nearest(48.1, 11.1, 1.0).map(|r| r.0), Some(100));
    }
}
