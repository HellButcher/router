# Storage Layer — Data Structures

## Data Structures

### TableFile\<D\>

Every binary table (nodes, edges, ways) is a flat memory-mapped file:

```
[Header: sizeof(D::Header) rounded up to 512-byte alignment]
[Record 0 | Record 1 | ... | Record N]   each record is sizeof(D) bytes
```

`D` must implement `TableData`, which requires:
- `D: TablePod` — safe to interpret any bit pattern (allows `AtomicU64` fields)
- `D::Header: TableDataHeader` — header type that optionally carries sparse index metadata

The mmap is opened with `Advice::Random`. Writing during import uses `AppenderJob`: parallel blob-parsing threads push ordered chunks through a channel; a background thread reassembles them via `BTreeMap<usize, Vec<D>>` and flushes in order, preserving the global ID sort across parallel workers.

`TableFile::filter()` compacts in-place: scans forward, copies passing items back into freed slots, then truncates. Used to remove unreachable nodes after adjacency linking.

`TableFile::create_with_capacity(path, count, fill)` pre-allocates the full file, mmaps the data region, and calls `fill(&mut [D])` to populate it — no heap allocation. Used for Morton-sort output and ID index construction.

#### Header types

**`SimpleHeader<I>`** — base header (160 bytes on disk, padded to 512). Stores a type-name hash, format version, `header_size`, and `data_size` for format verification. No index support.

**`HeaderWithIndex<I>`** — extends `SimpleHeader<I>` with three additional `u64` fields: `num_data_entries`, `entries_per_block`, `num_index_entries` (184 bytes, still padded to 512). Used when the table file should support a sparse lookup index. Implements `SupportsIndex`.

#### Sparse lookup index (optional)

When `build_index_sorted` is called on a `TableFile<D>` where `D: Item` and `D::Header: SupportsIndex`, a sparse key array is appended to the file after a 512-byte-aligned gap:

```
[Header: 512 bytes — HeaderWithIndex<D>, index metadata fields set]
[Data: N × sizeof(D)]
[Padding: 0..511 bytes to align to 512]
[Index keys: Y × u64  ← first key of every X-th block]
```

`Y` and `X` are chosen so the key array is approximately 4 MB. The index is skipped entirely when `Y ≤ 1` (table too small to benefit). On open, `new_intern` reads the header via a temporary small mmap; if `entries_per_block > 0` the key array is loaded into memory and future `find` calls use it.

**`find(key: u64)`** — available on `D: Item`. When the index is loaded: binary-searches the in-memory key array to narrow to a block of `X` entries, then binary-searches within that block. Without an index: binary-searches the full table. Both paths require the table to be sorted by `Item::key()`.

**`build_index_sorted()`** — available on `D: Item` where `D::Header: SupportsIndex`. Collects every X-th key, appends the key array to the file, and writes the three metadata fields into the header via the existing mmap.

#### Traits

| Trait | Purpose |
|-------|---------|
| `TablePod` | Safe to mmap and interpret as `&D`; allows `AtomicU64` |
| `TableData` | Associates a `TableDataHeader`-implementing header type with `D` |
| `TableDataHeader` | Header provides optional `index_info() -> Option<IndexInfo>` |
| `SupportsIndex` | Header additionally provides `set_index_info()`; gates `build_index_sorted` |
| `Item` | `D` has a `u64` sort key; enables `find` and `build_index_sorted` |

### IdEntry

A 16-byte `(key: u64, idx: u64)` record used as the element type of ID-index files (`TableFile<IdEntry>`). `key` is an OSM ID cast to `u64`; `idx` is the corresponding position in the primary (Morton-reordered) table file. Looked up via `TableFile::find`.

`node_id_index.bin` and `way_id_index.bin` are built in OSM ID order (matching the original table order), so `id_entries[old_pos].idx` can be written directly during Morton reordering without a binary search.

### PageFile (B-tree backing)

A fixed-4096-byte-page file, mmap'd. The on-disk B-tree (`BTreeMut`) stores packed `(key, value)` pairs per leaf. Note: the write path currently contains `todo!()` and is not part of the active import pipeline.

### Adjacency Lists (intrusive linked lists)

Edge connectivity is stored directly inside the record structs:

- `Node::first_edge_idx_outbound / first_edge_idx_inbound` — head pointer (table index); `u64::MAX` = empty.
- `Edge::next_edge / next_edge_reverse` — next pointer in the outbound / inbound chain.

Built lock-free during Phase 2 import using CAS loops on `AtomicU64` fields. At routing time, traversal is a pointer-chase through `Edge::next_edge`.

After any edge reorder, `remap_adjacency_lists(nodes, edges, ways, remap)` translates all stored edge indices through `remap[old] = new` in parallel, preserving the list structure without relinking from scratch.

### Spatial Index (packed R-tree / Flatbush style)

```
[Header: 512 bytes]
[Level 0: N leaf entries, sorted by Morton code of bbox centre]
[Level 1: ceil(N / node_size) internal entries]
...
[Root level: 1..node_size entries]
```

Each `RTreeEntry` (24 bytes): `{ min_lat, min_lon, max_lat, max_lon: f32, index: u64 }`. Build: external-sort leaves by Morton code via `extsort`, then build upper levels in one pass over the mmap'd output. Query: min-heap traversal ordered by `min_dist_to_bbox`.

When the input is already Morton-sorted (nodes after Phase 5, edges after Phase 6), `build_presorted` fills level-0 via `par_iter_mut` and skips the external sort entirely.

Two files: `node_spatial.bin` and `edge_spatial.bin`.

### External Sort (extsort / morton)

`storage::morton::sort_by_key(count, chunk_size, get_key, scratch, output)` external-sorts `count` items by an arbitrary `u64` key, streaming old indices to a callback in ascending order. Internally: rayon chunk-sort to a scratch file, then single-threaded k-way min-heap merge. One temp file, one merge pass.

`sort_by_morton(count, chunk_size, get_pos, scratch, output)` wraps `sort_by_key` with a Morton key derived from `(lat, lon)`.

### Morton Encoding

Standard 2D Z-order (Morton) curve mapping `(lat, lon)` → `u64` via bit-interleaving. Nearby geographic points map to nearby keys, clustering spatially related records onto the same memory-mapped pages.

### ID Field Dual Use

`Edge::from_node_idx`, `to_node_idx`, and `way_idx` store raw OSM IDs after Phase 1, replaced by table indices after Phase 4.

---

## Import Pipeline

```
Phase 1   Read PBF blobs in parallel
          → nodes.bin  (sorted by NodeId)
          → ways.bin   (sorted by WayId)
          → edges.bin  (sorted by appearance order)

Phase 2   Link adjacency lists
          For each edge: binary_search NodeId → prepend to Node linked lists (CAS)

Phase 3   Filter nodes
          nodes.filter(is_connected) — in-place compaction, remove isolated nodes

Phase 4   Build ID index files
          node_id_index.bin: TableFile<IdEntry>, key=NodeId, idx=node table position
          way_id_index.bin:  TableFile<IdEntry>, key=WayId,  idx=way  table position
          Written via create_with_capacity + rayon par_iter_mut.
          build_index_sorted appends the sparse key array for fast find().
          Used by the inspect service for OSM ID → primary table lookups.

Phase 5a  Resolve way_idx
          Linear O(n+m) cursor scan; also sets Way::first_edge_idx

Phase 5b  Resolve node indices, compute dist_m, country_id
          binary_search on filtered nodes (parallel, rayon)

Phase 6   Morton-sort nodes
          sort_by_morton → nodes_reordered.bin (written via create_with_capacity)
          node_id_index.idx column updated in-place: id_entries[old_pos].idx = new_pos
          Remap edge.from_node_idx / to_node_idx in parallel via id_entries

Phase 7   Morton-sort edges by from_node_idx
          sort_by_key → edges_reordered.bin
          Anonymous mmap remap[old] = new for adjacency pointer translation
          remap_adjacency_lists: parallel fetch_update on all AtomicU64 edge pointers
            (Node::first_edge_idx_outbound/inbound, Edge::next_edge/next_edge_reverse,
             Way::first_edge_idx)

Phase 8   Morton-sort ways by first_edge_idx
          sort_by_key → ways_reordered.bin
          way_id_index.idx column updated in-place (no build_index_sorted needed —
          keys are unchanged, only payload updated)
          Remap edge.way_idx in parallel via way_id_index entries

Phase 9   Build spatial indexes
          node_spatial.bin — build_presorted (nodes already Morton-sorted)
          edge_spatial.bin — build_presorted (edges already Morton-sorted by from_node)
```

### ID Index at Runtime

`Service` loads `node_id_index.bin` and `way_id_index.bin` as `TableFile<IdEntry>` on startup. The inspect service resolves OSM IDs via `id_index.find(id)` → `entry.idx` → `primary_table.get(idx)`.

### CSR Adjacency (Future Option)

Once Morton-ordered, an alternative to intrusive linked lists is CSR (Compressed Sparse Row): `outbound_offsets[N+1]` + `outbound_edges[E]`, one pair per direction. Benefits: sequential array scan instead of pointer chase; removes `AtomicU64` from node/edge structs. Space is asymptotically identical to linked lists with bidirectional traversal (`2(N+1)×8 + 2E×8` vs `16N + 16E` bytes). This is a larger follow-on refactor.
