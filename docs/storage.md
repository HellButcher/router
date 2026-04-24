# Storage Layer — Data Structures and Refactoring Plan

## Current Data Structures

### TableFile\<D\>

Every binary table (nodes, edges, ways) is a flat memory-mapped file with this layout:

```
[Header: sizeof(D::Header) rounded up to 512-byte alignment]
[Record 0 | Record 1 | ... | Record N]   each record is sizeof(D) bytes
```

All three tables are **sorted by their primary ID** (NodeId, WayId, or EdgeId-derived order) after import. Runtime lookups use `binary_search_by_key` — no in-memory hash tables or B-trees. The mmap is opened with `Advice::Random` since routing hops around in arbitrary order.

Writing during import is done through an `AppenderJob`: parallel blob-parsing threads push chunks through an unbounded channel; the background thread reassembles them in sequential (chunk-order) order via a `BTreeMap<usize, Vec<D>>` and flushes in order. This preserves the global ID sort across parallel workers.

`TableFile::filter()` compacts the file in-place: it scans forward, copies passing items backward into freed slots, then truncates. Used after the adjacency-linking phase to remove unreachable nodes.

### PageFile (B-tree backing)

A fixed-4096-byte-page file, mmap'd. The on-disk B-tree (`BTreeMut`) pages use the standard layout of `{ magic, flags, num_entries }` followed by packed `(key, value)` pairs. Note: the write path currently contains `todo!()` and is not part of the active import pipeline.

### Adjacency Lists (intrusive linked lists)

There is no separate adjacency table. Edge connectivity is stored directly inside the record structs:

- `Node::first_edge_idx_outbound / first_edge_idx_inbound` — head pointer (table index) of the linked list for each direction; `u64::MAX` = empty.
- `Edge::next_edge / next_edge_reverse` — next pointer in the outbound / inbound chain respectively.

The lists are built lock-free during Phase 2 import using CAS loops on `AtomicU64` fields. At routing time, following outbound edges from a node is a pointer-chase through `Edge::next_edge`.

### Spatial Index (packed R-tree / Flatbush style)

```
[Header: 512 bytes]
[Level 0: N leaf entries, sorted by Morton code of bbox centre]
[Level 1: ceil(N / node_size) internal entries]
...
[Root level: 1..node_size entries]
```

Each `RTreeEntry` (24 bytes) is `{ min_lat, min_lon, max_lat, max_lon: f32, index: u64 }`. For nodes the bbox is a point (`min == max`), for edges the bbox covers both endpoints.

The build process:
1. Pre-allocates the output file to its exact final size (all level sizes are computable upfront).
2. External-sorts the leaf entries by Morton code (Z-order curve on bbox centre) via `extsort`.
3. Builds upper levels by reading level N−1 from the already-mmap'd output and writing level N — no extra memory allocation.

Nearest-neighbour queries use a min-heap ordered by `min_dist_to_bbox` (haversine from query point to the closest point on each bbox). For point items (nodes) the first leaf popped is always the exact answer; for segment items (edges) a user-provided `refine()` closure computes the true distance.

Two index files are produced: `node_spatial.bin` (leaf `index` = node table index) and `edge_spatial.bin` (leaf `index` = edge table index).

### External Sort (extsort)

Sorts up to hundreds of millions of `(key: u64, index: u64)` pairs entirely on disk:

1. **Chunk phase (parallel)**: scratch file pre-allocated to `count × 16 bytes`, mmap'd, divided into disjoint `chunk_size`-entry regions (default 16 M entries = 256 MB each). Each rayon worker fills and sorts its region in place — no synchronisation needed.
2. **Merge phase (single-threaded)**: k-way min-heap merge over all sorted chunk regions, yielding `original_index` values in ascending key order.

A single temp file, one merge pass — no flip files.

### Morton Encoding

```
morton_world(lat, lon):
  x = (lat + 90)  / 180  * 2^32
  y = (lon + 180) / 360  * 2^32
  return spread_bits(x) | (spread_bits(y) << 1)
```

Standard 2D Z-order (Morton) curve. Used exclusively in the spatial index build step to determine leaf sort order. No Hilbert curve code exists currently.

### ID Field Dual Use

`Edge::from_node_idx`, `to_node_idx`, and `way_idx` each have two meanings depending on import phase:

| Phase | Value stored |
|-------|-------------|
| After Phase 1 | Raw OSM NodeId / WayId bits |
| After Phase 4 | Table index (position in the mmap'd array) |

This avoids a separate ID-to-index map at runtime but requires the resolve step before the file is usable.

---

## Import Pipeline (Current)

```
Phase 1  Read PBF blobs in parallel
         → nodes.bin  (sorted by NodeId, all OSM nodes including isolated ones)
         → ways.bin   (sorted by WayId)
         → edges.bin  (one or two directed edges per OSM way segment, sorted by appearance order)

Phase 2  Link adjacency lists
         For each edge: binary_search NodeId → prepend to Node linked lists (CAS)

Phase 3  Filter nodes
         nodes.filter(is_connected) — in-place compaction, remove isolated nodes

Phase 4a Resolve way_idx
         Linear O(n+m) cursor scan (edges and ways both in WayId order)
         Also sets Way::first_edge_idx

Phase 4b Resolve node indices, compute dist_m, country_id
         binary_search on filtered nodes (parallel, rayon)

Phase 5  Build spatial indexes
         node_spatial.bin — point R-tree over filtered node positions
         edge_spatial.bin — segment R-tree over resolved edge endpoints
```

---

## Refactoring Plan: Morton-Order Storage for Cache Locality

### Motivation

Routing performs random walks over nodes and edges. With nodes and edges stored in OSM ID order the working set of a single route is scattered across many pages, causing many page faults on large maps. Reordering nodes and edges by their geographic Morton code clusters nearby elements onto the same pages, improving TLB / page-cache locality especially for long-distance routes.

The spatial index already exploits Morton order for its own leaf sequence; the goal is to extend that ordering to the primary node and edge tables themselves.

### High-Level Phases

```
Phase 1   (unchanged) Parse PBF, write nodes/ways/edges in ID order
Phase 2   Build ID → file-offset index for fast ID lookup
Phase 3   Link adjacency lists  [may move before Phase 2, see variants]
Phase 4   Filter unused nodes
Phase 5   Compute Morton codes, external-sort nodes to new order
Phase 6   Reorder edges to follow node order (and optionally sort by Morton code of edge midpoint)
Phase 7   Resolve all indices to final positions in the reordered tables
Phase 8   Build spatial indexes (largely unchanged — already Morton-ordered at leaf level)
```

### Phase 2 — ID Index File

After Phase 1 the table files are already sorted by ID, so no sort is needed. Build a compact binary index:

```
[NodeId₀, NodeId₁, ..., NodeId_{n-1}]   (just the sorted key column, 8 bytes each)
```

Given a target `NodeId`, binary-search this array for the position `i`; the record lives at table offset `i`. This is a read-only auxiliary file and can be kept as a simple flat array — one 8-byte read to get the key, then a direct offset computation.

**Optional acceleration (page directory)**: add a two-level directory on top — e.g. a small array of `(every 256th key, page index)` pairs that narrows the binary search to a 256-entry window in one comparison. This trades a few KB of memory for a halved average seek depth on very large files. Whether this is necessary depends on profiling.

The index file also serves as a translation table during the reorder step: entry `i` will gain a `new_position` field once Morton sorting is known.

### Phase 5 — Morton Reorder of Nodes

1. For each node, compute `morton_world(node.lat, node.lon)`.
2. External-sort `(morton_code, old_index)` pairs using the existing `extsort` infrastructure (same parallel chunk → k-way merge pattern).
3. Write `nodes_reordered.bin` by reading `nodes.bin[old_index]` in merged order. This scan is sequential on the output side; the input accesses are random but the file is mmap'd.
4. Update the ID-index to record `old_index → new_index` mapping, or simply rebuild it for the reordered file (keys are no longer in ID order, so a hash map or a separate sorted `(NodeId, new_index)` array is needed for later edge resolution).

Because node positions within the reordered file are now address-contiguous by geography, the linked-list edge traversal (`first_edge_idx_outbound → next_edge → …`) will tend to land in the same memory region.

### Phase 6 — Morton Reorder of Edges

Edges can be reordered by the Morton code of their `from_node` (or their midpoint). Two strategies:

**Option A — follow node order**: assign each edge the Morton code of its `from_node`. This clusters the outbound edge list of nearby nodes onto the same pages. The intrusive linked list structure is preserved; traversal order follows geography.

**Option B — independent edge Morton sort**: compute the midpoint Morton code of each edge, sort edges independently. Better for edge-centric workloads (e.g. map matching), slightly worse for graph traversal since an edge's `next_edge` pointer may still scatter.

Option A is simpler and better fits the routing use case; Option B can be done as a second pass if needed.

### Phase 7 — Index Resolution

After both reorders:

1. For each edge, resolve `from_node_idx` and `to_node_idx` from old node positions to new node positions using the `old → new` mapping computed in Phase 5.
2. Rebuild `Node::first_edge_idx_outbound/inbound` and `Edge::next_edge/next_edge_reverse` linked lists using the new edge positions (the linked list heads and next pointers all change when edges move).
3. Resolve `way_idx` using the existing linear cursor scan (ways are not reordered, so this is unchanged).

This Phase 7 essentially replaces the current Phase 4. The resolution is now an index-space remapping rather than an ID-space lookup, so `binary_search_by_key` is replaced by direct array indexing through the mapping table.

### Phase 8 — Spatial Indexes

The spatial index build (Phase 5 currently) is unaffected in principle: it reads node positions and edge endpoint positions and sorts them by Morton code internally. After the reorder, the `index` fields it writes will be Morton-ordered node/edge positions, which is exactly what we want. No structural change needed.

### Variant A — Link Before Building the ID Index

Move adjacency linking (current Phase 2) to before the ID index build:

```
Phase 1  Parse PBF → ID-sorted tables
Phase 2  Link adjacency lists  (binary search on ID-sorted nodes, as today)
Phase 3  Filter nodes           (in-place compaction, as today)
Phase 4  Build ID index on filtered nodes
Phase 5  Morton-sort nodes
Phase 6  Morton-sort edges
Phase 7  Resolve to new positions, rebuild linked lists
Phase 8  Spatial indexes
```

Advantage: filtering (Phase 3) eliminates isolated nodes before building the ID index, keeping the index smaller. Linking still uses binary search on the original ID-sorted file, which is fast and already implemented.

### Variant B — Eliminate Linked Lists, Use Edge Arrays

Once nodes and edges are Morton-ordered, an alternative adjacency representation becomes attractive: store a sorted array of `(from_node_idx, edge_idx)` pairs instead of intrusive linked lists. This is a CSR (Compressed Sparse Row) style adjacency:

```
node_edge_offsets[i]  = first index into edge_list[] for node i
edge_list[]           = edge indices in from_node order
```

Benefits: improves prefetch predictability (sequential array scan instead of pointer chase) and simplifies routing code (`edges[offsets[i]..offsets[i+1]]` vs following `next_edge` pointers). Also removes the `AtomicU64` requirement from node/edge structs (avoids atomic-load overhead at runtime, even with relaxed ordering).

Space-wise, CSR provides **no meaningful saving** with bidirectional traversal. Both directions require an offset array of size N+1 and an edge-index array of size E: `2(N+1)×8 + 2E×8` bytes total, versus the linked list's `16N + 16E` bytes — asymptotically identical.

Drawbacks: requires the routing code to use a different adjacency API; the dual-use `AtomicU64` trick during import must be restructured; bidirectional (inbound) adjacency needs a separate CSR.

This is a larger refactor and could be done as a follow-on after the Morton reorder lands.

### Considerations and Open Questions

1. **Temporary file space**: at peak, Phase 5 needs `nodes_reordered.bin` + the extsort scratch + the `(morton, old_index)` sort buffer simultaneously. Budget roughly 3× the node table size in scratch space.
2. **Way reordering**: ways are small and accessed rarely (only to look up metadata per edge). The `way_idx` resolution already uses a linear cursor scan that does not benefit from random access. Ways probably do not need reordering; if they did, the same pattern applies.
3. **Morton vs Hilbert**: Hilbert curve has better locality (no axis-aligned discontinuities) but is more complex to compute. Morton is already implemented; it provides most of the benefit with no new code. Consider Hilbert only if profiling shows Morton is insufficient.
4. **Backwards compatibility**: the reordered files are not compatible with the current format. Either bump all `VERSION` constants or add a format flag in `SimpleHeader`. The mmap'd files are read-only at runtime so no migration path is needed — reimport produces the new format.
5. **Incremental updates**: reordering makes incremental updates much harder (any node position change ripples through the linked lists and edge indices). This is not a concern as long as the import is always a full reimport from PBF.
