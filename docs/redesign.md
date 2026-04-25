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

### EdgeNode (replaces Edge + geometry Node)

Each directed compressed segment between two intersection nodes becomes an `EdgeNode`. This is the **routing node** in the edge-based graph.

```rust
struct EdgeNode {
    from_node_idx: u32,    // intersection node index (segment start)
    to_node_idx: u32,      // intersection node index (segment end)
    way_idx: u32,
    dist_m: u32,           // total Haversine distance along segment (sum of all node-pair distances)
    flags: EdgeFlags,      // vehicle access restrictions
    country_id: CountryId,
    first_turn_idx: u32,   // head of outgoing TurnEdge linked list
    geometry_offset: u32,  // offset into geometry table (intermediate node coordinates)
    geometry_len: u16,     // number of intermediate coordinates (0 for direct segments)
    max_speed: u8,
}
```

Cost of traversing an `EdgeNode` = travel time computed from `dist_m`, `max_speed`, and `Way` metadata.

### TurnEdge (new: encodes a legal turn)

Each legal transition from one `EdgeNode` to another at a shared intersection node is a `TurnEdge`. This is the **routing edge** in the edge-based graph.

```rust
struct TurnEdge {
    to_edge_node_idx: u32, // destination EdgeNode
    turn_cost_ms: u16,     // penalty for this turn (0 = straight-on, higher for sharp turns)
    next_turn_idx: u32,    // linked-list pointer: next TurnEdge for the same source EdgeNode
}
```

Turn restrictions = simply do not emit a `TurnEdge` for that (from, via, to) triple.  
Turn penalties = assign a non-zero `turn_cost_ms`.

### IntersectionNode (replaces Node for routing)

After compression, only intersection nodes remain as first-class data. Geometry nodes are stored as coordinates in a flat geometry table, referenced by `EdgeNode::geometry_offset`.

```rust
struct IntersectionNode {
    id: NodeId,            // OSM node ID (for turn restriction resolution)
    pos: LatLon,
    flags: NodeFlags,      // traffic signals, toll, access restrictions
}
```

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
Phase 1: Parse nodes     → intersection_nodes.bin (after compression), geometry.bin
Phase 2: Parse ways      → way reference counting → segment compression → edge_nodes.bin
Phase 3: Parse relations → turn_restrictions (in-memory)
Phase 4: Build turns     → turn_edges.bin (from intersection pairs + restrictions + penalties)
Phase 5: Sort & index    → Morton sort, spatial index, ID indices
```

---

## Phase 1: Node Parsing

Identical to current: parse all OSM nodes into a temporary flat array sorted by NodeId (guaranteed by `Sort.Type_then_ID`). Store `(NodeId, LatLon, NodeFlags)`.

At this stage **all** nodes are stored — we don't yet know which are intersection nodes. The intersection determination happens during way processing.

---

## Phase 2: Way Processing and Graph Compression

This is the most significant change. Ways must be processed in **two sub-passes**.

### Sub-pass 2a: Reference counting

Walk every way's node ref list and increment a counter for each node ID encountered. Also mark the first and last node ref of every way as an endpoint.

A node is an **intersection node** if:
- Its reference count across all ways ≥ 2, **or**
- It is a way endpoint (first or last node of any way)

Since way blobs may be processed in parallel, use a concurrent counter structure (e.g. a sorted array of `(NodeId, count)` merged after parallel collection, or a lock-free hashmap).

The result is a set of intersection node IDs. All other referenced node IDs are geometry nodes.

### Sub-pass 2b: Compressed segment building

For each way (applying the same tag parsing as today), walk the node ref list and build compressed segments:

```
state: current_intersection_node = first node ref (always an endpoint → intersection)
       accumulated_geometry = []
       accumulated_dist_m = 0

for each subsequent node ref N in the way:
    pos_N = node_positions[N]
    accumulated_dist_m += haversine(prev_pos, pos_N)

    if N is an intersection node:
        emit EdgeNode {
            from: current_intersection_node,
            to: N,
            way_idx: current_way,
            dist_m: accumulated_dist_m,
            geometry: accumulated_geometry,   → write to geometry.bin
            flags, max_speed, country_id, ...
        }
        if way is bidirectional (not oneway):
            emit EdgeNode {
                from: N,
                to: current_intersection_node,
                way_idx: current_way,
                dist_m: accumulated_dist_m,
                geometry: reverse(accumulated_geometry),
                flags, max_speed (backward), ...
            }
        current_intersection_node = N
        accumulated_geometry = []
        accumulated_dist_m = 0
    else:
        accumulated_geometry.push(pos_N)
```

Oneway and bicycle contraflow logic is unchanged from the current importer.

### What replaces the current edge/node structures

| Current | New |
|---------|-----|
| `Node` (all OSM nodes) | `IntersectionNode` (intersection nodes only) |
| `Edge` (one per directed node pair) | `EdgeNode` (one per directed compressed segment) |
| `Edge::next_edge` linked list | `TurnEdge::next_turn_idx` linked list |
| `Node::first_edge_idx_outbound` | `EdgeNode::first_turn_idx` |

Geometry nodes exist only in the flat `geometry.bin` table; they are not routing entities.

### Distance on compressed edges

`EdgeNode::dist_m` is the sum of Haversine distances across all node pairs in the segment. This is more accurate than computing a single Haversine from `from` to `to` node (which would be a chord, not the road length).

`u32` is used instead of `u16` for `dist_m` — a compressed segment can span many kilometres on a rural road.

---

## Phase 3: Relation Parsing (Turn Restrictions)

OSM relations come after ways in the PBF. At this point all `EdgeNode` indices are known.

### OSM relation structure

```
type=restriction (or type=restriction:<vehicle>)
  member role=from   type=way    → from_way_id
  member role=via    type=node   → via_node_id  (most common)
  member role=to     type=way    → to_way_id
tag restriction = no_right_turn | no_left_turn | no_u_turn | no_straight_on
                | only_right_turn | only_left_turn | only_straight_on
```

`only_*` restrictions mean: all turns from `from` at `via` **except** `to` are prohibited.

### Resolution

Build an index: `(WayId, NodeId) → EdgeNode` — i.e., given a way ID and an intersection node, find the EdgeNode whose way is that way and whose `to_node` is that node (for the incoming `from` way) or `from_node` is that node (for the outgoing `to` way).

This index is small (one entry per EdgeNode) and fits in memory.

For each restriction relation:
1. Resolve `from_way_id` + `via_node_id` → incoming `EdgeNode` index
2. Resolve `to_way_id` + `via_node_id` → outgoing `EdgeNode` index
3. Store as `TurnRestriction { from_edge_node, to_edge_node, kind, vehicle_mask }`

Collect into a table indexed by `via_node_idx` for efficient lookup in Phase 4.

---

## Phase 4: Turn Edge Construction

For each `IntersectionNode V`, gather:
- All `EdgeNode`s whose `to_node_idx == V` → **incoming set**
- All `EdgeNode`s whose `from_node_idx == V` → **outgoing set**

For each pair `(incoming EdgeNode X, outgoing EdgeNode Y)`:

1. **Check access:** if `Y.flags` blocks the vehicle, skip (no TurnEdge emitted for that vehicle — or store in flags).
2. **Check restriction:** look up `TurnRestriction` table at `V` for `(X, Y)`. If prohibited, skip.
3. **Check mandatory:** if any `only_*` restriction at `V` covers `X` and `Y` is not the mandated target, skip.
4. **Compute turn angle** from the exit bearing of `X` and entry bearing of `Y`:
   - Exit bearing of `X`: bearing from second-to-last geometry point to `V`
   - Entry bearing of `Y`: bearing from `V` to first geometry point (or `Y.to_node` if no geometry)
5. **Compute turn cost** from angle: 0 for straight-on, configurable curve for left/right, very high for U-turns (or skip entirely).
6. Emit `TurnEdge { to_edge_node_idx: Y_idx, turn_cost_ms, next_turn_idx }` and link into `X`'s linked list.

### U-turns

An incoming `EdgeNode X` (way W, A→B) and outgoing `EdgeNode Y` (way W, B→A) at node B represents a U-turn. Detected by: `X.way_idx == Y.way_idx && X.from_node_idx == Y.to_node_idx`. Emit with a large penalty or skip entirely (configurable per road class).

---

## Phase 5: Sorting and Indexing

Unchanged in structure from today, adapted to new table names:

- Morton-sort `IntersectionNode`s by position → spatial locality for A\* heuristic and spatial queries
- Sort `EdgeNode`s by `from_node_idx` (Morton order) → outbound turns cluster for cache efficiency
- Sort `TurnEdge`s by source `EdgeNode` index (already in linked-list order after Phase 4)
- Build spatial index over `EdgeNode`s (bounding box of `from_pos` to `to_pos`) for snap-to-road queries
- Build `NodeId → IntersectionNode` ID index (for turn restriction resolution and API lookups)
- Build `WayId → Way` ID index (unchanged)

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
| `nodes.bin` | → `intersection_nodes.bin` | Only intersection nodes |
| `edges.bin` | → `edge_nodes.bin` | One per directed compressed segment |
| *(new)* | `turn_edges.bin` | One per legal turn at each intersection |
| *(new)* | `geometry.bin` | Flat array of `LatLon` for intermediate node coords |
| `ways.bin` | Unchanged | Way metadata |
| `node_id_index.bin` | Adapted | NodeId → IntersectionNode index |
| `way_id_index.bin` | Unchanged | WayId → Way index |
| `node_spatial.bin` | → `intersection_node_spatial.bin` | Spatial index over intersection nodes |
| `edge_spatial.bin` | Adapted | Spatial index over EdgeNodes (compressed segments) |
