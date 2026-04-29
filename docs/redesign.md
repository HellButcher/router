# Router Redesign: Edge-Based Graph with Turn Restrictions

## Motivation

Three things need to change before Contraction Hierarchies (CH) can be added:

1. **Geometry nodes inflate the graph.** Every consecutive node pair on every OSM way becomes an `Edge`. Interior nodes that carry no routing significance bloat the graph 5–10×.
2. **No turn awareness.** Search state is `node_idx`. Turn restrictions and penalties cannot be expressed, and CH requires a turn-aware graph to produce correct shortcuts.
3. **OSM relations are not parsed.** The importer skips `type=restriction` relations.

The target is a **materialized edge-based graph**: directed road segments become the routing nodes, and legal turns between them become routing edges. This is the same model OSRM uses, and it is the correct foundation for CH — no virtual adapter phase is needed.

---

## The Directed Graph and OSM Ways

### How the current importer creates directed edges

For each OSM way, the importer emits one `Edge` per consecutive node pair:

```
Way [1, 2, 3, 4], bidirectional:
  1→2, 2→3, 3→4   (forward)
  4→3, 3→2, 2→1   (backward)

Way [1, 2, 3, 4], oneway=yes:
  1→2, 2→3, 3→4   (forward only)
```

Bicycle contraflow on a oneway produces an extra backward edge with `NO_MOTOR | NO_HGV`.

This means for a bidirectional road, each intermediate node (2 and 3 above) has:
- 2 inbound edges: one from each direction
- 2 outbound edges: one in each direction
- All 4 edges belong to the same way

These nodes carry no routing decision — they are pure geometry.

### Intersection nodes

A node is an **intersection node** if it is referenced by two or more distinct ways, or if it is a way endpoint (first or last node of any way). All other nodes are geometry-only and will be collapsed into edge geometry.

The criterion is OSM-structural: **shared node between ways = routing connection**. Two ways that cross geometrically but share no node are grade-separated (bridge over tunnel) and have no routing connection.

---

## Target Data Model

### Way (extended)

`Way` gains per-direction access flags, max speed, and direction flags. In the common case — a bidirectional road with identical properties in both directions — a single `Way` entry covers both. When the directions differ (different speed, different access restrictions, different dimension limits, or bicycle contraflow on a oneway), **two consecutive `Way` entries** are emitted with the same OSM `WayId`: one for the forward direction, one for backward. The `HAS_PAIR` flag marks that a sibling entry exists.

```rust
struct Way {
    id: WayId,
    highway: HighwayClass,
    flags: WayFlags,           // ONEWAY, TOLL, TUNNEL, BRIDGE,
                               // DIRECTION_FORWARD, DIRECTION_BACKWARD, HAS_PAIR
    surface_quality: SurfaceQuality,
    access: EdgeFlags,         // NO_MOTOR, NO_HGV, NO_BICYCLE, NO_FOOT
    max_speed: u8,
    dim: DimRestriction,
    first_edge_node_idx: AtomicU64,
}
```

`DIRECTION_FORWARD | HAS_PAIR` means "this entry covers the forward direction; the backward sibling is the next entry". `DIRECTION_BACKWARD | HAS_PAIR` means "this entry covers the backward direction; the forward sibling is the previous entry". Neither flag set means the entry covers both directions identically.

**Cases that require two entries:**
- `maxspeed:forward` ≠ `maxspeed:backward`
- Bicycle contraflow on a oneway (backward direction: `NO_MOTOR | NO_HGV`; forward: unrestricted)
- oneway (backward direction: `NO_MOTOR | NO_HGV | NO_BICYCLE` (FOOT allowed); forward: unrestricted)
- `maxheight:forward` ≠ `maxheight:backward` (or any other directional dimension)
- Any explicit directional access tag (`vehicle:forward=no`, etc.)

The old `Edge` fields `flags` and `max_speed` are eliminated — this information is now read from `Way` when building `EdgeNode`s.

### EdgeNode (replaces Edge + geometry Node)

Each directed compressed segment between two intersection nodes becomes an `EdgeNode`. This is the **routing node** in the edge-based graph. `way_idx` points to the directional `Way` entry directly, so access flags, speed, and dimensions are looked up without any direction logic at query time.

```rust
struct EdgeNode {
    way_idx: u64,                          // index of the directional Way entry
    dist_m: u32,                           // total Haversine distance (sum over all node pairs in segment)
    country_id: CountryId,
    _pad: [u8; 3],
    first_outbound_turn_idx: AtomicU64,    // head of outbound TurnEdge linked list (forward search)
    first_inbound_turn_idx: AtomicU64,     // head of inbound TurnEdge linked list (backward search)
    geometry_offset: u64,                  // index of first geometry point (= from_pos)
    geometry_len: u32,                     // always ≥ 2; geometry[offset+len-1] = to_pos
}
```

Cost of traversal = travel time from `dist_m`, `Way::max_speed`, `Way::highway`, and `Way::surface_quality`.

### TurnEdge (new: encodes a legal turn)

Each legal transition `X→Y` is stored as a single `TurnEdge` that participates in **two linked lists simultaneously**: the outbound list of `X` (for forward search) and the inbound list of `Y` (for backward search). This mirrors exactly how the current `Edge` struct uses `next_edge` and `next_edge_reverse`.

```rust
struct TurnEdge {
    from_edge_node_idx: u64,       // source EdgeNode (X)
    to_edge_node_idx: u64,         // destination EdgeNode (Y)
    turn_angle: i16,               // signed degrees: negative=left, positive=right, ±180=U-turn
    restriction_mask: VehicleFlags,// vehicles prohibited by OSM restriction (0 = unrestricted)
    _pad: [u8; 5],
    next_outbound_idx: AtomicU64,  // next in X's outbound list
    next_inbound_idx: AtomicU64,   // next in Y's inbound list
}  // 40 bytes
```

Both lists are built in Phase 4 using lock-free CAS prepend, identical to how the current importer builds node→edge adjacency. `from` is needed when traversing Y's inbound list (to know the source); `to` is needed when traversing X's outbound list (to know the destination).

**Turn cost is computed dynamically by the `CostModel`**, consistent with how edge traversal cost is computed from `dist_m` + `max_speed` rather than stored precomputed. The `CostModel` gains:

```rust
fn turn_cost(&self, turn: &TurnEdge, from: &EdgeNode, to: &EdgeNode) -> Option<usize>
```

Returning `None` if the turn is blocked for this vehicle (via `restriction_mask`), otherwise a penalty in milliseconds derived from `turn_angle` using a configurable curve. The `from` and `to` `EdgeNode`s give access to their `Way` metadata (road class, speed), enabling profile-specific behaviour such as penalising turns from major onto minor roads.

**Turn restrictions** are handled at two levels:
- Turns prohibited for *all* vehicles → `TurnEdge` is not emitted at all
- Turns prohibited for *some* vehicles → emitted with `restriction_mask` set; `CostModel` returns `None` for blocked vehicles

**CH note:** since edge costs are already profile-specific, CH preprocessing is per-profile regardless. Dynamic turn costs add no new constraint here.

**Note on dense arrays:** After CH preprocessing freezes the graph, linked lists can be flattened to dense offset arrays `(outbound_offset, outbound_len)` per `EdgeNode` for better cache performance during queries. Linked lists are kept through CH because shortcut insertion is O(1) CAS prepend; dense arrays would require O(n) rebuilds per contraction step.

### No intersection node table at runtime

Intersection nodes do not need to be a permanent data structure. Everything they provide can be moved elsewhere:

- **Position** — not stored on `EdgeNode` at all. Endpoint positions are the first and last entries of the geometry sequence (see below). The A\* heuristic reads the last geometry entry; turn angle computation reads the last two entries for exit bearing.
- **Traffic signal / toll penalty** — moved to a `TurnFlags` field on `TurnEdge`; every turn through that via-node carries the flag, and `CostModel::turn_cost` applies the time penalty.
- **Barrier / access restrictions** — already in `TurnEdge::restriction_mask`.
- **NodeId lookup and node snapping** — not needed at query time; dropped.

Intersection nodes exist only as a **temporary in-memory structure during import** (Phases 2–8), used to compute reference counts, resolve turn restrictions, and compute segment geometry. They are never written to a permanent file.

**`geometry.bin`** — a flat array of `LatLon`, one entry per intermediate (geometry-only) node, in the order they appear within each way segment. Referenced by `EdgeNode::geometry_offset` + `geometry_len`. No IDs or flags needed.

### Graph trait stays unchanged

The `Graph` trait operates on abstract node indices and yields `Neighbour { node, cost }`. In the edge-based graph, "nodes" are `EdgeNode` indices. The `outbound` iterator walks the `TurnEdge` linked list of a given `EdgeNode`.

`Neighbour.cost` for a turn to `EdgeNode Y` = `turn_cost(X→Y) + travel_time(Y)`. Storing it this way means `dist[Y]` already includes Y's traversal cost when Y is settled — no separate accumulation needed.

Dijkstra, A\*, and bidirectional variants are **unchanged**.

---

## Import Pipeline

### Current pipeline (summary)

```
PBF blobs (parallel) → nodes.bin, edges.bin, ways.bin
→ link_nodes_and_edges (adjacency linked lists)
→ filter unconnected nodes
→ resolve edge node/way indices, dist_m, country_id
→ Morton-sort nodes → remap edge node indices
→ sort edges by from_node_idx → remap adjacency lists
→ Morton-sort ways → remap edge way indices
→ build spatial indices
```

### New pipeline

```
Phase 1:  Parse PBF (single pass, parallel per blob)
  1a: nodes     → temp_nodes.tmp  (NodeId, LatLon, NodeFlags, AtomicU8; sorted by NodeId from PBF)
                  AtomicU8 is used for reference counting,
  1b: ways      → ways.bin        (one or two Way entries per OSM way)
                  node_refs.tmp   (flat resolved node-index lists per way, with way boundaries)
                  increment ref_counts during parallel blob processing
                    Endpoints add ENDPOINT flag,
                    When count has reached 2, add INTERSECTION flag
  1c: relations → turn_restrictions (Vec in memory; globally ~3–5 M entries)

Phase 2:  Morton-sort ways by morton-code of first point → ways_reordered.bin
          build WayId index

Phase 3: Walk Ways and coresponding node_refs.tmp and Construct geometry and EdgeNodes.
     3a:  Write LatLon of Way-Geometry into geometry.bin in order of Ways.
          update way geometry offset,
          record old offset in a mapping.
     3b:  Construct EdgeNodes → edge_nodes.bin
          emit one EdgeNode per directed compressed segment (forward + backward where applicable)

Phase 4:  Morton-sort EdgeNodes by morton-code of first point → edge_nodes_reordered.bin
          build edge spatial index (bounding box of geometry)

Phase 5:  Build TurnEdges → turn_edges.bin
          Take flags from temp_nodes (use the mapping, to get the old node offset)
          
```

---

### What replaces the current edge/node structures

| Current | New |
|---------|-----|
| `Node` (all OSM nodes) | eliminated — positions stored in `geometry.bin`, flags on `TurnEdge` |
| `Edge` (one per directed node pair) | `EdgeNode` (one per directed compressed segment) |
| `Edge::flags`, `Edge::max_speed` | `Way::access`, `Way::max_speed` (per-direction via `HAS_PAIR`) |
| `Edge::next_edge` / `next_edge_reverse` | `TurnEdge::next_outbound_idx` / `next_inbound_idx` |
| `Node::first_edge_idx_outbound/inbound` | `EdgeNode::first_outbound_turn_idx` / `first_inbound_turn_idx` |

All node positions (endpoints and intermediates) live in `geometry.bin` as a flat `LatLon` array. Intersection nodes exist only as a temporary in-memory structure during import.

---

## Phase 1: PBF Parsing

Single pass over the PBF, parallel per blob (unchanged parallelism model).

**Phase 1a — nodes:** Parse all OSM nodes into `temp_nodes.tmp` sorted by `NodeId` (guaranteed by `Sort.Type_then_ID`). Store `(NodeId, LatLon, NodeFlags)`. Allocate two parallel arrays indexed by node position: `ref_counts: AtomicU8[]` (saturating at 2 — we only need to distinguish 0, 1, ≥2) and `endpoint_flags: AtomicBool[]`.

**Phase 1b — ways:** For each OSM way:
- Parse tags and emit one or two `Way` entries to `ways.bin` (see Way struct above for when two are needed).
- Write the raw node-ID list to `node_refs.tmp` (flat: `[id0: i64, id1: i64, ...]` per way).
- For each node ref: binary-search `temp_nodes.tmp`, then `tmp_nodes[idx].ref_count.fetch_add(1, saturating)`. Mark ENDPOINT flag for the first and last ref of each way.

**Phase 1c — relations:** Parse `type=restriction` relations into an in-memory `Vec<RawRestriction>`. Relations are globally ~3–5 million entries and fit comfortably in memory. Each entry records `(from_way_id, via_node_id, to_way_id, restriction_kind, vehicle_mask)`.

---

## Phase 2: Sort ways by Morton-Codes

---

## Phase 3a: Resolve Nodes → geometry.bin

Walk `ways_reordered.tmp` sequentially. For each way, walk its node-index list and build compressed segments:
Coordinates of ways are written to `geometry.bin` in sequence, and offset is updated in way.
Record old offsets as pair `(new_index: usize, old_index: usize)` in a temporary file. this maps index `new_index + i` to `old_index + i` where `0 >= i > way.offsets_len`.

`geometry_len` is always ≥ 2 (from_pos + to_pos, with zero or more intermediates). `dist_m` is the sum of Haversine distances along the segment — more accurate than a single chord distance.

Each way records the `geometry_offset` of its first segment for use in Phase 4.

---

## Phase 3b: Construct EdgeNodes → edge_nodes.bin

Walk `ways.bin` and the geometry offset table from Phase 3. For each compressed segment:

- Emit a **forward** `EdgeNode { way_idx, dist_m, geometry_offset, geometry_len, ... }` referencing the forward `Way` entry.
- If the way is bidirectional (not oneway), emit a **backward** `EdgeNode` referencing the backward `Way` entry (same geometry slice, reversed traversal — `geometry[offset + len - 1]` is the from-pos).

Access flags, max speed, and dimensions come from the directional `Way` entry pointed to by `way_idx`. No direction logic is needed at query time.

`country_id` is resolved here by looking up the from-position in the country boundary index.

---

## Phase 4: Morton-Sort EdgeNodes → edge_nodes_reordered.bin

Sort `edge_nodes.bin` by the position of each EdgeNode's first geometry point (= from_pos = `geometry[geometry_offset]`). Build edge spatial index (bounding box per EdgeNode from `geometry[offset]` to `geometry[offset + len - 1]`). Remap `TurnEdge` linked-list heads after the sort.

---

## Phase 5: Build TurnEdges → turn_edges.bin

Group EdgeNodes by their shared endpoint: `geometry[geometry_offset + geometry_len - 1]` is the `to_pos` / via-node position. EdgeNodes sharing the same endpoint position form an intersection.

**Restriction resolution:** For each `RawRestriction (from_way_id, via_node_id, to_way_id, ...)`, find the matching EdgeNodes by scanning the EdgeNodes whose `Way::id == from_way_id` and whose last geometry point matches `via_node_id`'s position (looked up from `temp_nodes.tmp`). This scan is proportional to the number of restrictions (~millions), not the number of EdgeNodes (~hundreds of millions) — no large index is needed.

For each intersection and each pair `(incoming EdgeNode X, outgoing EdgeNode Y)`:

1. **Check restriction:** if `(X, Y)` is prohibited by a `no_*` restriction for any vehicle, set `restriction_mask` accordingly. If prohibited for all vehicles, skip entirely (no `TurnEdge` emitted).
2. **Check mandatory:** if a `only_*` restriction covers X at this intersection and Y is not the mandated target, prohibit all vehicles.
3. **Compute turn angle:** exit bearing of X from `geometry[X.offset + X.len - 2]` → `geometry[X.offset + X.len - 1]`; entry bearing of Y from `geometry[Y.offset]` → `geometry[Y.offset + 1]`. Angle is the signed difference.
4. **Set `TurnFlags`:** if the via-node has `NodeFlags::TRAFFIC_SIGNALS` or `NodeFlags::TOLL`, set the corresponding `TurnFlags` bit.
5. Emit `TurnEdge { from, to, turn_angle, restriction_mask, turn_flags, next_outbound, next_inbound }` and CAS-prepend into X's outbound list and Y's inbound list.

### U-turns and sharp reversals

A same-way U-turn (X and Y share `way_idx`, X arrives at the node Y departs from) is one special case. More generally, any turn with `|turn_angle| > 150°` is a sharp reversal — this covers dual-carriageway U-turns where the two directions are separate ways with different `way_idx`. The `CostModel` applies a configurable penalty curve: zero for straight-on, increasing for sharper turns, and a very high (or infinite) penalty at ±180°. This single mechanism handles both same-way U-turns and dual-carriageway reversals without special-casing.

---

## Routing Query Adaptation

### Snap to road

A query point snaps to the nearest `EdgeNode` (directed segment) via the edge spatial index, returning an `EdgeNode` index and a fractional position along the segment. This is identical to today's edge snap, just operating on compressed edges.

### Source and target injection

From a snapped position on `EdgeNode S` at fraction `f`:
- Forward search starts from `S` with initial cost `(1 - f) × travel_time(S)`
- Backward search starts from `S` with initial cost `f × travel_time(S)`

For bidirectional search, the "backward" graph inverts TurnEdge direction: `from Y to X` if there exists a TurnEdge `X → Y`.

### Path unpacking

A found path is a sequence of `EdgeNode` indices. Unpacking a route:
1. Collect `EdgeNode` geometry for each step → full polyline
2. Report road names, distances, turn instructions from `TurnEdge` angles

---

## CH Preparation

With a materialized edge-based graph, CH is straightforward:

- **Contract `EdgeNode`s** in order of importance
- **Shortcuts** are new `TurnEdge`s spanning a contracted `EdgeNode`: `(X → contracted → Y)` becomes a shortcut TurnEdge `X → Y` with cost `cost(X→C) + cost(C→Y)`
- **Turn costs are already baked in** to TurnEdge costs — no special handling needed during contraction
- **Shortcut unpacking** follows TurnEdge chains recursively

Graph compression (Phase 2) must precede CH. CH on an uncompressed graph would be 5–10× larger and produce shortcuts through geometry nodes that carry no routing significance.

---

## Summary of File Changes

| File | Status | Description |
|------|--------|-------------|
| `nodes.bin` | **removed** | No runtime node table; positions in `geometry.bin`, flags on `TurnEdge` |
| `node_id_index.bin` | **removed** | Node lookup dropped |
| `node_spatial.bin` | **removed** | Node snapping dropped |
| `edges.bin` | → `edge_nodes.bin` | One per directed compressed segment |
| *(new)* | `turn_edges.bin` | One per legal turn at each intersection |
| *(new)* | `geometry.bin` | Flat `LatLon` array — all node positions (endpoints + intermediates) |
| `ways.bin` | Extended | Gains `access`, `max_speed`, direction flags (`HAS_PAIR` etc.) |
| `way_id_index.bin` | Unchanged | WayId → Way index |
| `edge_spatial.bin` | Adapted | Spatial index over EdgeNodes (bounding box of geometry) |
