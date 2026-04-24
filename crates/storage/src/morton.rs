/// Z-order (Morton) curve encoding and sorting utilities.
use std::{fs::OpenOptions, io, path::Path};

/// Compute the Z-order (Morton) curve key for a geographic coordinate.
///
/// Maps `(lat, lon)` uniformly onto a `u64` such that nearby points on the
/// globe have nearby keys. Used to sort spatial tables for page-cache locality.
pub fn morton_world(lat: f32, lon: f32) -> u64 {
    let x = ((lat + 90.0) / 180.0 * u32::MAX as f32) as u32;
    let y = ((lon + 180.0) / 360.0 * u32::MAX as f32) as u32;
    encode(x, y)
}

fn encode(x: u32, y: u32) -> u64 {
    spread(x as u64) | (spread(y as u64) << 1)
}

fn spread(mut v: u64) -> u64 {
    v = (v | (v << 16)) & 0x0000_ffff_0000_ffff;
    v = (v | (v << 8)) & 0x00ff_00ff_00ff_00ff;
    v = (v | (v << 4)) & 0x0f0f_0f0f_0f0f_0f0f;
    v = (v | (v << 2)) & 0x3333_3333_3333_3333;
    v = (v | (v << 1)) & 0x5555_5555_5555_5555;
    v
}

/// Default sort-chunk size: 16 M entries × 16 bytes ≈ 256 MB RAM per chunk.
pub const DEFAULT_CHUNK_SIZE: usize = 16 * 1024 * 1024;

/// External-sort `count` items by Morton key, streaming sorted old indices to
/// `output` in ascending Morton order.
///
/// `get_pos(i)` returns `(lat, lon)` for item `i`. `scratch` is created,
/// used as the external-sort run file, and deleted on completion (even on
/// error, best-effort). `chunk_size` controls how many entries are sorted
/// in-memory per rayon chunk; see [`DEFAULT_CHUNK_SIZE`].
pub fn sort_by_morton(
    count: usize,
    chunk_size: usize,
    get_pos: impl Fn(usize) -> (f32, f32) + Sync,
    scratch: &Path,
    output: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<()> {
    let run_file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(scratch)?;

    let result = crate::extsort::sort_and_merge(
        count,
        chunk_size,
        |i| {
            let (lat, lon) = get_pos(i);
            morton_world(lat, lon)
        },
        run_file,
        output,
    );
    let _ = std::fs::remove_file(scratch);
    result
}
