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

### Way (extended) — **implemented**

`Way` gains per-direction access flags, max speed, and direction flags. In the common case — a bidirectional road with identical properties in both directions — a single `Way` entry covers both. When the directions differ (different speed, different access restrictions, or bicycle contraflow on a oneway), **two consecutive `Way` entries** are emitted with the same OSM `WayId`: one for the forward direction, one for backward. The `HAS_PAIR` flag marks that a sibling entry exists.

```rust
struct Way {
    id: WayId,
    highway: HighwayClass,
    flags: WayFlags,           // ONEWAY, TOLL, TUNNEL, BRIDGE,
                               // DIRECTION_FORWARD, DIRECTION_BACKWARD, HAS_PAIR
    surface_quality: SurfaceQuality,
    access: EdgeFlags,         // NO_MOTOR, NO_HGV, NO_BICYCLE, NO_FOOT
    max_speed: u8,             // km/h; 0 = use profile default for highway class
    node_refs_count: u16,      // number of entries in way_nodes.bin for this way
    dim: DimRestriction,
    node_refs_idx: u64,        // start index into way_nodes.bin
}
```

`DIRECTION_FORWARD | HAS_PAIR` means "this entry covers the forward direction; the backward sibling is the next entry". `DIRECTION_BACKWARD | HAS_PAIR` means "this entry covers the backward direction; the forward sibling is the previous entry". Neither flag set means the entry covers both directions identically.

`node_refs_idx` and `node_refs_count` are import-time fields, used in Phase 3 to build geometry and `EdgeNode`s. They are not needed at query time.

**Cases that require two entries:**
- `maxspeed:forward` ≠ `maxspeed:backward`
- Bicycle contraflow on a oneway (backward direction: `NO_MOTOR | NO_HGV`; forward: unrestricted)
- Any explicit directional access tag (`vehicle:forward=no`, etc.)

The old `Edge` fields `flags` and `max_speed` are eliminated — this information is now read from `Way` when building `EdgeNode`s.

### EdgeNode (replaces Edge + geometry Node) — **implemented**

Each directed compressed segment between two intersection nodes becomes an `EdgeNode`. This is the **routing node** in the edge-based graph. `way_idx` points to the directional `Way` entry directly, so access flags, speed, and dimensions are looked up without any direction logic at query time.

```rust
struct EdgeNode {
    way_idx: u64,                          // index of the directional Way entry
    dist_m: u32,                           // total Haversine distance of this segment
    country_id: CountryId,
    _pad: u8,
    geometry_len: i16,                     // signed: positive=forward, negative=backward; |len| ≥ 2
    first_outbound_turn_idx: AtomicU64,    // head of outbound TurnEdge linked list (forward search)
    first_inbound_turn_idx: AtomicU64,     // head of inbound TurnEdge linked list (backward search)
    geometry_from_idx: u64,                // index of first geometry point in geometry.bin
}
```

The sign of `geometry_len` encodes traversal direction within the shared geometry slice:
- positive → forward: read `geometry[from_idx .. from_idx + len]` left to right
- negative → backward: read `geometry[from_idx .. from_idx + |len|]` right to left

Forward and backward `EdgeNode`s for the same segment share the same `geometry_from_idx` and point to the same slice, just traversed in opposite directions.

Cost of traversal = travel time from `dist_m`, `Way::max_speed`, `Way::highway`, and `Way::surface_quality`.

### TurnEdge (new: encodes a legal turn) — **implemented**

Each legal transition `X→Y` is stored as a single `TurnEdge` that participates in **two linked lists simultaneously**: the outbound list of `X` (for forward search) and the inbound list of `Y` (for backward search). This mirrors exactly how the current `Edge` struct uses `next_edge` and `next_edge_reverse`.

```rust
struct TurnEdge {
    from_edge_node_idx: u64,       // source EdgeNode (X)
    to_edge_node_idx: u64,         // destination EdgeNode (Y)
    turn_angle: i16,               // signed degrees: negative=left, positive=right, ±180=U-turn
    restriction_mask: EdgeFlags,   // vehicles prohibited by OSM restriction (0 = unrestricted)
    turn_flags: TurnFlags,         // TRAFFIC_SIGNALS, TOLL (from via-node)
    _pad: [u8; 4],
    next_outbound_idx: AtomicU64,  // next in X's outbound list
    next_inbound_idx: AtomicU64,   // next in Y's inbound list
}  // 40 bytes
```

Both lists are built in Phase 5 using lock-free CAS prepend, identical to how the current importer builds node→edge adjacency. `from` is needed when traversing Y's inbound list (to know the source); `to` is needed when traversing X's outbound list (to know the destination).

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

- **Position** — stored as the first/last entry of the geometry slice. The A\* heuristic reads the last geometry entry; turn angle computation reads the last two entries for exit bearing.
- **Traffic signal / toll penalty** — moved to `TurnFlags` on `TurnEdge`; every turn through that via-node carries the flag, and `CostModel::turn_cost` applies the time penalty.
- **Barrier / access restrictions** — in `TurnEdge::restriction_mask`.
- **NodeId lookup and node snapping** — not needed at query time; dropped.

Intersection nodes exist only as a **temporary structure during import** (`nodes.bin` is kept through Phase 5), used to compute reference counts, resolve turn restrictions, and build segment geometry. After Phase 5 the file can be discarded.

**`geometry.bin`** — a flat array of `LatLon`, one entry per geometry point (both endpoints and intermediates), in the order they are emitted per way segment. Referenced by `EdgeNode::geometry_from_idx` + `|geometry_len|`. No IDs or flags stored.

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
Phase 1:  Parse PBF (single pass, parallel per blob)           ✓ Done
  1a: nodes     → nodes.bin    (NodeId, LatLon, NodeFlags, num_refs; sorted by NodeId)
                  NodeFlags and num_refs are updated atomically during 1b.
  1b: ways      → ways.bin     (one or two Way entries per OSM way)
                  way_nodes.bin (flat resolved node-table-index lists; Pod64 per ref)
                  For each node ref: increment num_refs; set ENDPOINT flag on first/last;
                  set INTERSECTION flag when num_refs reaches 2.
  1c: relations → Vec<RawRestriction> in memory (simple via-node restrictions only)

Phase 2:  WayId index + Morton-sort ways                       ✓ Done
  2a: build way_id_index.bin from ways.bin (already sorted by WayId from PBF)
  2b: Morton-sort ways.bin by first geometry point → overwrite ways.bin
      (update way_id_index.bin entries to reflect new positions)

Phase 3:  Geometry + EdgeNodes                                 TODO
  Walk ways.bin + way_nodes.bin sequentially.
  For each way, split node-ref sequence at intersection/endpoint nodes into segments.
  3a: write all segment LatLon points to geometry.bin in way order
  3b: emit one forward EdgeNode per segment; emit backward EdgeNode for bidirectional ways
      (both share the same geometry_from_idx; backward uses negative geometry_len)
      resolve country_id from from-position via CountryLookup

Phase 4:  Morton-sort EdgeNodes                                TODO
  Sort edge_nodes.bin by from-position (geometry[from_idx] for forward,
  geometry[from_idx + |len| - 1] for backward).
  Build edge spatial index (bounding box per EdgeNode).

Phase 5:  TurnEdges                                            TODO
  Group EdgeNodes by shared to-position (intersection node).
  For each intersection, emit TurnEdges for all legal (incoming, outgoing) pairs.
  Resolve OSM restrictions from Vec<RawRestriction> + way_id_index.
  Set TurnFlags from NodeFlags (TRAFFIC_SIGNALS, TOLL) read from nodes.bin.
  CAS-prepend each TurnEdge into both outbound and inbound linked lists.
```

---

### What replaces the current edge/node structures

| Current | New |
|---------|-----|
| `Node` (all OSM nodes) | import-time only (`nodes.bin` discarded after Phase 5) |
| `Edge` (one per directed node pair) | `EdgeNode` (one per directed compressed segment) |
| `Edge::flags`, `Edge::max_speed` | `Way::access`, `Way::max_speed` (per-direction via `HAS_PAIR`) |
| `Edge::next_edge` / `next_edge_reverse` | `TurnEdge::next_outbound_idx` / `next_inbound_idx` |
| `Node::first_edge_idx_outbound/inbound` | `EdgeNode::first_outbound_turn_idx` / `first_inbound_turn_idx` |

All node positions (endpoints and intermediates) live in `geometry.bin` as a flat `LatLon` array.

---

## Phase 1: PBF Parsing — Done

Single pass over the PBF, parallel per blob (unchanged parallelism model).

**Phase 1a — nodes:** Parse all OSM nodes into `nodes.bin` sorted by `NodeId` (guaranteed by PBF `Sort.Type_then_ID`). Store `(NodeId, LatLon, NodeFlags, num_refs)`. `NodeFlags` and `num_refs` are zero-initialised here; they are updated atomically during Phase 1b.

**Phase 1b — ways:** For each OSM way:
- Parse tags and emit one or two `Way` entries to `ways.bin` (see Way struct above for when two are needed).
- Resolve each node ref against `nodes.bin` (binary search by NodeId); write the resolved table index as `Pod64` to `way_nodes.bin`. The first entry for each way is at `way.node_refs_idx`.
- For each node ref: `node.num_refs.fetch_add(1)`. When `num_refs` reaches 2, set `NodeFlags::INTERSECTION`. Mark `NodeFlags::ENDPOINT` for the first and last ref of each way.

**Phase 1c — relations:** Parse `type=restriction` relations into an in-memory `Vec<RawRestriction>`. Only simple via-node restrictions are captured (via-way restrictions are ignored for now). Each entry records `(from_way_id, via_node_id, to_way_id, only: bool, vehicle_mask)`.

---

## Phase 2: WayId index + Morton-sort ways — Done

**Phase 2a:** Build `way_id_index.bin` (sparse lookup index) over `ways.bin`, which is already in WayId order from PBF parsing.

**Phase 2b:** Morton-sort `ways.bin` by the position of each way's first node. Index entries in `way_id_index.bin` are remapped to the new positions. The sorted file overwrites `ways.bin` in-place (via a temp rename).

---

## Phase 3: Geometry + EdgeNodes — TODO

Walk `ways.bin` and `way_nodes.bin` sequentially. For each way, iterate the node-ref sequence and identify segment boundaries: any node with `NodeFlags::INTERSECTION` or `NodeFlags::ENDPOINT` (this includes the way's own first and last node, since they were marked ENDPOINT during Phase 1b).

**Phase 3a — geometry.bin:** For each segment `[i..=j]` in a way, append the `LatLon` of every node in order to `geometry.bin`. Record the start offset `geometry_from_idx` for use in 3b. `dist_m` is computed as the sum of Haversine distances along the segment.

**Phase 3b — edge_nodes.bin:** For each segment:
- Emit a **forward** `EdgeNode { way_idx, dist_m, country_id, geometry_from_idx, geometry_len > 0 }` pointing to the forward `Way` entry.
- If the way is bidirectional (has a backward direction), emit a **backward** `EdgeNode` with the same `geometry_from_idx` but `geometry_len < 0`, pointing to the backward `Way` entry (same `way_idx` for identical bidirectional ways, `way_idx + 1` for `HAS_PAIR` pairs).
- `country_id` is resolved from the from-position via `CountryLookup`.

For `HAS_PAIR` ways: when the current way has `DIRECTION_FORWARD | HAS_PAIR`, the next entry in `ways.bin` is `DIRECTION_BACKWARD | HAS_PAIR` and shares the same `node_refs_idx`. Process both together and skip the backward entry in the outer loop.

---

## Phase 4: Morton-Sort EdgeNodes — TODO

Sort `edge_nodes.bin` by the from-position of each `EdgeNode`:
- forward: `geometry[geometry_from_idx]`
- backward: `geometry[geometry_from_idx + |geometry_len| - 1]`

Build edge spatial index (bounding box per `EdgeNode` from first to last geometry point). Remap `TurnEdge` linked-list heads after the sort (none exist yet at this point — heads are initialised to `NO_TURN` and filled in Phase 5).

---

## Phase 5: Build TurnEdges — TODO

Group `EdgeNode`s by their to-position (= `geometry[from_idx + |len| - 1]` for forward, `geometry[from_idx]` for backward). `EdgeNode`s sharing the same to-position form an intersection.

**Restriction resolution:** For each `RawRestriction (from_way_id, via_node_id, to_way_id, ...)`, find the matching `EdgeNode`s via `way_id_index` and verify the to-position matches `via_node_id`'s position (looked up from `nodes.bin`).

For each intersection and each pair `(incoming EdgeNode X, outgoing EdgeNode Y)`:

1. **Check restriction:** if `(X, Y)` is prohibited by a `no_*` restriction for any vehicle, set `restriction_mask` accordingly. If prohibited for all vehicles, skip entirely (no `TurnEdge` emitted).
2. **Check mandatory:** if a `only_*` restriction covers X at this intersection and Y is not the mandated target, prohibit all vehicles.
3. **Compute turn angle:** exit bearing of X from second-to-last → last geometry point; entry bearing of Y from first → second geometry point. Angle is the signed difference.
4. **Set `TurnFlags`:** look up the via-node in `nodes.bin`; if `NodeFlags::TRAFFIC_SIGNALS` or `NodeFlags::TOLL`, set the corresponding `TurnFlags` bit.
5. Emit `TurnEdge { from, to, turn_angle, restriction_mask, turn_flags, ... }` and CAS-prepend into X's outbound list and Y's inbound list.

### U-turns and sharp reversals

Any turn with `|turn_angle| > 150°` is a sharp reversal. The `CostModel` applies a configurable penalty curve: zero for straight-on, increasing for sharper turns, and a very high (or infinite) penalty at ±180°. This handles both same-way U-turns and dual-carriageway reversals without special-casing.

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
| `nodes.bin` | import-time only | Kept through Phase 5 for flag/position lookups; not needed at query time |
| `node_id_index.bin` | **removed** | Node lookup dropped |
| `node_spatial.bin` | **removed** | Node snapping dropped |
| `way_nodes.bin` | **new (import-time)** | Flat `Pod64` array of resolved node-table indices per way; used in Phase 3 |
| `edges.bin` | → `edge_nodes.bin` | One per directed compressed segment |
| *(new)* | `turn_edges.bin` | One per legal turn at each intersection |
| *(new)* | `geometry.bin` | Flat `LatLon` array — all segment geometry points |
| `ways.bin` | Extended | Gains `access`, `max_speed`, direction flags (`HAS_PAIR` etc.) |
| `way_id_index.bin` | Unchanged | WayId → Way index |
| `edge_spatial.bin` | Adapted | Spatial index over EdgeNodes (bounding box of geometry) |
