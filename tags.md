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
| `maxheight`, `maxheight:physical` | Stored as `DimRestriction::max_height_dm` (minimum of both) |
| `maxwidth`, `maxwidth:physical` | Stored as `DimRestriction::max_width_dm` (minimum of both) |
| `maxlength` | Stored as `DimRestriction::max_length_dm` |
| `maxweight` | Stored as `DimRestriction::max_weight_250kg` |
| `maxspeed:advisory` | Used as fallback when `maxspeed` is absent |
| `impassable`, `status=impassable` | Excluded |
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

**Status:** Done.

`impassable=yes` and `status=impassable` are now parsed and cause `is_excluded()` to return `true`.

---

### 2. Missing physical restriction: `maxlength`

**Status:** Done.

`maxlength` is parsed and stored as `DimRestriction::max_length_dm`. `VehicleDim` gained a matching
`length_dm` field. Field order in both structs is height / width / length / weight.

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

**Status:** Done.

Parsed and used as fallback in `lib.rs` when `maxspeed` is absent.

#### 3c. `maxheight:physical` / `maxwidth:physical`

**Status:** Done.

Both `:physical` variants are parsed. `dim_restriction_from_tags` takes the minimum of the
physical and plain values, so the binding constraint (structural or legal) always wins.

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

| Priority | Item | Effort | Status |
|----------|------|--------|--------|
| 1 | `impassable` / `status=impassable` exclusion | Small | Done |
| 2 | `maxlength` | Small | Done |
| 3 | `maxheight:physical` / `maxwidth:physical` | Small | Done |
| 4 | `maxspeed:advisory` fallback | Small | Done |
| 5 | `service` subtype speed penalties | Medium | Open |
| 6 | `lanes` counts | Medium | Open |
| 7 | String storage design | Medium (design) | Open — prerequisite for 8–10 |
| 8 | `name` and `ref` | Medium | Open |
| 9 | `destination` / `destination:ref` | Medium | Open |
| 10 | `turn:lanes` | Medium | Open |
