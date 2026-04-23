# OSM Tag Processing — Observations, Current State & Plan

---

## Current State

Tags are parsed in `crates/import-pbf/src/tags.rs` into `WayTags` and `NodeTags`.
Derived data is written to `Way`, `Edge`, and `Node` structs in `lib.rs`.

### What we handle correctly

| Tag(s) | Notes |
|--------|-------|
| `highway` | Full set of routable classes; unknown/no/construction/proposed → excluded |
| `junction` | `roundabout`, `circular` → implies oneway |
| `oneway`, `oneway:<mode>` | Forward, reverse (`-1`), bicycle contraflow |
| `toll` | Stored as `WayFlags::TOLL` |
| `tunnel`, `bridge` | Stored as way flags |
| `route=ferry` | Detected; way kept as `HighwayClass::Ferry` |
| `maxspeed`, `maxspeed:forward`, `maxspeed:backward` | Parsed to km/h; named values (e.g. `DE:urban`) resolved |
| `maxheight`, `maxwidth`, `maxweight` | Stored as `DimRestriction` |
| `surface`, `smoothness`, `tracktype` | Mapped to `SurfaceQuality` |
| `motorroad` | Flags NO_BICYCLE + NO_FOOT |
| `area` | Excluded |
| `disused`, `abandoned` | Excluded |
| `access`, `<mode>:access`, `<mode>` | Full hierarchy; multi-mode conditional; direction-specific |
| `<mode>:lanes` | Ignored (was mistakenly treated as way-level access) |
| `foot:left/right` | Correctly ignored for way-level exclusion |
| Node `barrier` | bollard/gate → NO_MOTOR+NO_HGV; kissing_gate → +NO_BICYCLE; cycle_barrier → NO_BICYCLE |
| Node `highway=traffic_signals` | `NodeFlags::TRAFFIC_SIGNALS` |
| Node `toll` | `NodeFlags::TOLL` |

---

## Gaps & Planned Improvements

### 1. Missing exclusion conditions

**Status:** Not implemented.

**Observation:** The tags `impassable=yes` and `status=impassable` mark roads that are physically
blocked or permanently closed but haven't been given a `highway=no` or `access=no` tag by the
mapper.

**Plan:**
- Add `impassable: bool` and `raw_status: Option<&str>` fields to `WayTags`.
- Extend `is_excluded()` to return `true` when `impassable` or `status=impassable`.

---

### 2. Missing physical restriction: `maxlength`

**Status:** Not implemented. We store `maxheight`, `maxwidth`, `maxweight` but not length.

**Observation:** `maxlength` restricts oversized vehicles (articulated trucks, buses).
This is a legal restriction in many countries and matters for HGV routing.

**Plan:**
- Add `raw_max_length: Option<&str>` to `WayTags`, mirroring the existing height/width/weight fields.
- Add `max_length_dm: u8` to `DimRestriction` (or a new field with a suitable unit).
- Parse in `dim_restriction_from_tags` using the existing `parse_dim_m` helper.

---

### 3. Missing speed-affecting tags

#### 3a. `service` subtype

**Status:** `highway=service` is recognised as a class, but the `service` tag value is not read.

**Observation:** Service roads differ significantly in speed by subtype:
- `driveway` → very slow (e.g. 10 km/h)
- `parking_aisle` → very slow
- `alley` → moderate
- default → moderate

**Plan:**
- Add a `Service` tag enum (`driveway`, `parking_aisle`, `alley`, `drive_through`) to `tags.rs`.
- Add `service: Option<Service>` to `WayTags`.
- In `lib.rs`, use the service value to cap `max_speed` on `highway=service` ways, or store it
  in a new `WayFlags` bit or as part of the `Way` struct for later use by the routing profile.

#### 3b. `maxspeed:advisory`

**Status:** Not read.

**Observation:** Some mappers use `maxspeed:advisory` for recommended speeds (e.g. highway
on-ramps) when no legal limit exists. Useful as a fallback when `maxspeed` is absent.

**Plan:**
- Add `raw_max_speed_advisory: Option<&str>` to `WayTags`.
- In `lib.rs`, use it as fallback when `raw_max_speed` is `None`.

#### 3c. `maxheight:physical` / `maxwidth:physical`

**Status:** Not read; we only parse `maxheight` / `maxwidth`.

**Observation:** The `:physical` variants encode the structural clearance (e.g. actual bridge
opening) as opposed to the legal/posted limit. On bridges the physical clearance is often the
binding constraint.

**Plan:**
- Add `raw_max_height_physical: Option<&str>` and `raw_max_width_physical: Option<&str>` to `WayTags`.
- In `dim_restriction_from_tags`, take the minimum of the physical and plain values when both
  are present.

---

### 4. Missing navigation/guidance data

These require schema changes (new fields on `Way` or a separate string table).

#### 4a. `name` and `ref`

**Status:** Not extracted.

**Observation:** Way names and route references are essential for turn-by-turn instructions and
map display.

**Plan:**
- Decide on a string storage strategy (e.g. a separate string table with offsets stored on `Way`,
  or an interned string pool).
- Extract `name` and `ref` tags in `lib.rs` and store references on the `Way` struct.

#### 4b. `destination`, `destination:ref`, `destination:street`

**Status:** Not extracted.

**Observation:** These tags encode the sign text shown on direction signs. Directional variants
(`destination:forward`, `destination:backward`) allow per-direction signs. A combined display
string can be formed by joining `destination:ref` and `destination` (e.g. `"A 45: Frankfurt"`).

**Plan:**
- Depends on 4a (string storage).
- Extract destination tags and store per-direction strings on `Edge` or `Way`.

#### 4c. `turn:lanes`, `turn:lanes:forward`, `turn:lanes:backward`

**Status:** Not extracted.

**Observation:** The raw lane string (e.g. `"left|through|through|right"`) encodes per-lane turn
restrictions and is needed for lane guidance at intersections. Parsing into individual lane
descriptors is a query-time or post-import step.

**Plan:**
- Depends on 4a (string storage).
- Extract `turn:lanes`, `turn:lanes:forward`, `turn:lanes:backward` as raw strings.
- Store per-direction on `Way` or `Edge`.

#### 4d. `lanes`, `lanes:forward`, `lanes:backward`

**Status:** Not extracted.

**Observation:** Lane counts are needed to correctly interpret `turn:lanes` and can also inform
routing weights (e.g. wider roads have higher free-flow capacity).

**Plan:**
- Add `lanes: Option<u8>`, `lanes_forward: Option<u8>`, `lanes_backward: Option<u8>` to `WayTags`.
- Store the effective lane count on `Way` (forward + backward separately).

---

## Implementation Order

| Priority | Item | Effort | Rationale |
|----------|------|--------|-----------|
| 1 | `impassable` / `status=impassable` exclusion | Small | Correctness: prevents routing through blocked roads |
| 2 | `maxlength` | Small | Correctness: HGV routing |
| 3 | `maxheight:physical` / `maxwidth:physical` | Small | Correctness: bridge clearance |
| 4 | `maxspeed:advisory` fallback | Small | Speed accuracy |
| 5 | `service` subtype speed penalties | Medium | Speed accuracy for urban routing |
| 6 | `lanes` counts | Medium | Needed for turn:lanes interpretation |
| 7 | String storage design | Medium (design) | Prerequisite for 8–10 |
| 8 | `name` and `ref` | Medium | Navigation display |
| 9 | `destination` / `destination:ref` | Medium | Sign text for guidance |
| 10 | `turn:lanes` | Medium | Lane guidance at intersections |
