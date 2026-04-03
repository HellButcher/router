/// External merge sort over a single mmap'd run file.
///
/// # Algorithm
///
/// ## Chunk phase  (parallel)
/// The item range `0..count` is divided into chunks of `chunk_size`.
/// The run file is pre-allocated to `count × 16 bytes` and mapped with
/// `MmapMut`. `rayon::par_chunks_mut` gives each worker thread a disjoint
/// sub-slice; threads fill their region with `(key, index)` pairs and sort
/// in-place — no copying, no inter-chunk synchronisation.
///
/// ## Merge phase  (single-threaded k-way heap merge)
/// The run file is re-opened as a read-only `Mmap`. Each chunk's sorted region
/// is a `&[SortEntry]` view into that single mapping. A min-heap merges all
/// chunks in one pass, calling `output(original_index)` in ascending key order.
///
/// ## Why one file, why no flip?
/// All chunk sizes are known before any I/O, so the whole scratch space can be
/// pre-allocated upfront and addressed by offset — no separate file per chunk.
/// A *flip* (alternating between two files) is only needed for repeated 2-way
/// passes; here a single k-way merge finishes in one pass.
use std::{cmp::Reverse, collections::BinaryHeap, fs::File, io};

use memmap2::{Mmap, MmapMut};
use rayon::prelude::*;

/// One record stored per item in the run file (16 bytes, 8-byte aligned).
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SortEntry {
    /// Sort key (ascending).
    pub key: u64,
    /// Original item index; returned by `output`.
    pub index: u64,
}

const ENTRY_SIZE: usize = std::mem::size_of::<SortEntry>(); // 16

/// Sort `count` items by key using external merge sort.
///
/// * `get_key(i)` — returns the sort key for item `i`. Must be `Sync` so it
///   can be shared across rayon threads. Typically a closure over a mmap'd
///   node slice — no bulk copy of node data is performed.
/// * `run_file` — scratch file opened for read+write; allocated to
///   `count × 16 bytes` at the start of this function.
/// * `output(index)` — called once per item in ascending key order with the
///   original item index.
pub(crate) fn sort_and_merge(
    count: usize,
    chunk_size: usize,
    get_key: impl Fn(usize) -> u64 + Sync,
    run_file: File,
    mut output: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<()> {
    assert!(chunk_size >= 1);

    if count == 0 {
        return Ok(());
    }

    // ── pre-allocate ──────────────────────────────────────────────────────────
    run_file.set_len((count * ENTRY_SIZE) as u64)?;

    // ── chunk phase (parallel) ────────────────────────────────────────────────
    // Map the whole file for writing. `par_chunks_mut` splits the slice into
    // disjoint regions — no locking needed.
    {
        let _span = tracing::info_span!("extsort_chunk_phase", count, chunk_size).entered();
        let mut mmap = unsafe { MmapMut::map_mut(&run_file) }?;
        let all =
            unsafe { std::slice::from_raw_parts_mut(mmap.as_mut_ptr() as *mut SortEntry, count) };

        all.par_chunks_mut(chunk_size)
            .enumerate()
            .for_each(|(chunk_idx, chunk)| {
                let base = chunk_idx * chunk_size;
                for (ki, i) in (base..base + chunk.len()).enumerate() {
                    chunk[ki] = SortEntry {
                        key: get_key(i),
                        index: i as u64,
                    };
                }
                chunk.sort_unstable();
            });

        mmap.flush()?;
    } // MmapMut dropped; file position is independent

    // ── merge phase (k-way heap) ──────────────────────────────────────────────
    // Re-open read-only; each chunk is a &[SortEntry] view — no BufReaders,
    // the OS pages in only the entries the heap touches.
    let _span = tracing::info_span!("extsort_merge_phase", count).entered();
    let mmap = unsafe { Mmap::map(&run_file) }?;
    let all = unsafe { std::slice::from_raw_parts(mmap.as_ptr() as *const SortEntry, count) };

    let num_chunks = count.div_ceil(chunk_size);
    let chunk_slice = |i: usize| -> &[SortEntry] {
        let start = i * chunk_size;
        let end = ((i + 1) * chunk_size).min(count);
        &all[start..end]
    };

    // Min-heap item: (Reverse(key), chunk_id, position_within_chunk).
    let mut heap: BinaryHeap<(Reverse<u64>, usize, usize)> = BinaryHeap::with_capacity(num_chunks);
    for i in 0..num_chunks {
        let s = chunk_slice(i);
        if !s.is_empty() {
            heap.push((Reverse(s[0].key), i, 0));
        }
    }

    while let Some((_, chunk_id, pos)) = heap.pop() {
        output(chunk_slice(chunk_id)[pos].index)?;
        let next = pos + 1;
        if next < chunk_slice(chunk_id).len() {
            heap.push((Reverse(chunk_slice(chunk_id)[next].key), chunk_id, next));
        }
    }

    Ok(())
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn run_sort(keys_and_vals: &[(u64, u64)], chunk_size: usize) -> Vec<u64> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.tmp");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        let mut out = Vec::new();
        sort_and_merge(
            keys_and_vals.len(),
            chunk_size,
            |i| keys_and_vals[i].0,
            file,
            |idx| {
                out.push(keys_and_vals[idx as usize].1);
                Ok(())
            },
        )
        .unwrap();
        out
    }

    #[test]
    fn test_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("run.tmp");
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .unwrap();
        sort_and_merge(0, 4, |_| 0, file, |_| Ok(())).unwrap();
    }

    #[test]
    fn test_single_chunk() {
        let items = [(3u64, 30u64), (1, 10), (2, 20)];
        assert_eq!(run_sort(&items, 100), [10, 20, 30]);
    }

    #[test]
    fn test_multi_chunk() {
        // chunk_size=2 → 3 runs for 5 items
        let items = [(5u64, 50u64), (3, 30), (1, 10), (4, 40), (2, 20)];
        assert_eq!(run_sort(&items, 2), [10, 20, 30, 40, 50]);
    }

    #[test]
    fn test_large_multi_chunk() {
        let n = 10_000u64;
        // keys in reverse order; chunk_size=500 → 20 chunks
        let items: Vec<(u64, u64)> = (0..n).rev().map(|i| (i, i)).collect();
        let result = run_sort(&items, 500);
        // items[j]=(key=n-1-j, val=n-1-j); ascending key order → values 0,1,...,n-1
        let expected: Vec<u64> = (0..n).collect();
        assert_eq!(result, expected);
    }
}
