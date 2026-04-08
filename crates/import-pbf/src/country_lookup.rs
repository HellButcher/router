use std::path::Path;

use geo::{BoundingRect, Contains, Point, Polygon};
use geojson::{GeoJson, Value as GeoJsonValue};
use router_types::country::CountryId;
use rstar::{AABB, RTree, RTreeObject};

// ── grid constants ────────────────────────────────────────────────────────────

/// Resolution of the raster cache in degrees.
const GRID_RES: f64 = 0.1;
const GRID_LAT: usize = 1800; // 180 / 0.1
const GRID_LON: usize = 3600; // 360 / 0.1

/// Sentinel stored in the grid when a cell straddles multiple countries —
/// fall back to the polygon check for these cells.
const AMBIGUOUS: u8 = u8::MAX;

#[inline]
fn grid_idx(lat: f64, lon: f64) -> usize {
    let lat_i = ((lat + 90.0) / GRID_RES) as usize;
    let lon_i = ((lon + 180.0) / GRID_RES) as usize;
    lat_i.min(GRID_LAT - 1) * GRID_LON + lon_i.min(GRID_LON - 1)
}

// ── RTree entries ─────────────────────────────────────────────────────────────

struct CountryEntry {
    country_id: CountryId,
    geometry: Polygon<f64>,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for CountryEntry {
    type Envelope = AABB<[f64; 2]>;
    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

impl CountryEntry {
    fn new(country_id: CountryId, geometry: Polygon<f64>) -> Self {
        let envelope = geometry
            .bounding_rect()
            .map(|r| AABB::from_corners([r.min().x, r.min().y], [r.max().x, r.max().y]))
            .unwrap_or_else(|| AABB::from_corners([0.0, 0.0], [0.0, 0.0]));
        Self {
            country_id,
            geometry,
            envelope,
        }
    }

    fn contains(&self, lon: f64, lat: f64) -> bool {
        self.geometry.contains(&Point::new(lon, lat))
    }
}

// ── CountryLookup ─────────────────────────────────────────────────────────────

pub struct CountryLookup {
    /// Raster cache: one byte per 0.1° cell. Most lookups hit this directly.
    /// `AMBIGUOUS` means the cell is on a border — fall through to `tree`.
    grid: Box<[u8]>,
    tree: RTree<CountryEntry>,
}

impl CountryLookup {
    /// Load country boundaries from a GeoJSON file.
    /// The file must contain a `FeatureCollection` where each feature has an
    /// `ISO_A2` string property and a Polygon or MultiPolygon geometry.
    pub fn from_file(path: &Path) -> Result<Self, LookupError> {
        let content = std::fs::read_to_string(path).map_err(LookupError::Io)?;
        let geojson: GeoJson = content
            .parse()
            .map_err(|e| LookupError::GeoJson(Box::new(e)))?;

        let GeoJson::FeatureCollection(fc) = geojson else {
            return Err(LookupError::NotAFeatureCollection);
        };

        let mut entries: Vec<CountryEntry> = Vec::new();

        for feature in fc.features {
            // Extract ISO alpha-2 code from properties.
            let iso = feature
                .property("ISO_A2")
                .and_then(|v| v.as_str())
                .unwrap_or("-99");

            let country_id = CountryId::from_iso2(iso);
            if country_id.is_unknown() {
                // Skip features without a recognised ISO code (e.g. disputed territories).
                continue;
            }

            let Some(geometry) = feature.geometry else {
                continue;
            };

            // Each sub-polygon gets its own RTree entry with a tight envelope,
            // so the fallback check only tests polygons that actually overlap
            // the query point's bounding box.
            let polygons: Vec<Polygon<f64>> = match geometry.value {
                GeoJsonValue::Polygon(_) => {
                    let poly: Polygon<f64> = geometry
                        .try_into()
                        .map_err(|_| LookupError::GeometryConversion)?;
                    vec![poly]
                }
                GeoJsonValue::MultiPolygon(_) => {
                    let multi: geo::MultiPolygon<f64> = geometry
                        .try_into()
                        .map_err(|_| LookupError::GeometryConversion)?;
                    multi.0
                }
                _ => continue,
            };

            for poly in polygons {
                entries.push(CountryEntry::new(country_id, poly));
            }
        }

        tracing::info!(polygons = entries.len(), "loaded country boundaries");

        // Scanline-rasterize all country polygons onto the grid.
        let _span = tracing::info_span!("build_country_grid").entered();
        let mut grid = vec![CountryId::UNKNOWN.0; GRID_LAT * GRID_LON].into_boxed_slice();
        for entry in &entries {
            rasterize_polygon(&entry.geometry, entry.country_id.0, &mut grid);
        }
        let ambiguous = grid.iter().filter(|&&v| v == AMBIGUOUS).count();
        tracing::info!(cells = GRID_LAT * GRID_LON, ambiguous, "country grid built");

        let tree = RTree::bulk_load(entries);
        Ok(Self { grid, tree })
    }

    /// Returns the `CountryId` for the given coordinates, or `CountryId::UNKNOWN`.
    pub fn lookup(&self, lat: f32, lon: f32) -> CountryId {
        let lat = lat as f64;
        let lon = lon as f64;
        let cell = self.grid[grid_idx(lat, lon)];
        if cell != AMBIGUOUS {
            return CountryId(cell);
        }
        // Border cell — fall back to exact polygon check.
        let query = AABB::from_corners([lon, lat], [lon, lat]);
        for entry in self.tree.locate_in_envelope_intersecting(&query) {
            if entry.contains(lon, lat) {
                return entry.country_id;
            }
        }
        CountryId::UNKNOWN
    }
}

/// Scanline-rasterize a single polygon onto the grid using the even-odd rule.
///
/// For each grid row covered by the polygon's bounding box, the algorithm finds
/// all edge intersections at the row's centre latitude, sorts them, and fills
/// between alternating pairs — this handles holes automatically via even-odd.
/// Cells already filled by a different country are marked `AMBIGUOUS`.
/// A column-direction pass marks cells crossed by east-west edges as `AMBIGUOUS`.
fn rasterize_polygon(poly: &Polygon<f64>, id: u8, grid: &mut [u8]) {
    let Some(bbox) = poly.bounding_rect() else {
        return;
    };

    let lat_i_min = ((bbox.min().y + 90.0) / GRID_RES) as usize;
    let lat_i_max = (((bbox.max().y + 90.0) / GRID_RES) as usize).min(GRID_LAT - 1);
    let lon_i_min = ((bbox.min().x + 180.0) / GRID_RES) as usize;
    let lon_i_max = (((bbox.max().x + 180.0) / GRID_RES) as usize).min(GRID_LON - 1);

    // Collect all rings (exterior + holes) once.
    let rings: Vec<&geo::LineString<f64>> = std::iter::once(poly.exterior())
        .chain(poly.interiors())
        .collect();

    let mut xs: Vec<f64> = Vec::new();

    // ── Row scanlines: fill interior ──────────────────────────────────────────
    for lat_i in lat_i_min..=lat_i_max {
        let lat = (lat_i as f64 + 0.5) * GRID_RES - 90.0;
        xs.clear();

        for ring in &rings {
            let coords = ring.0.as_slice();
            for w in coords.windows(2) {
                let (ay, ax) = (w[0].y, w[0].x);
                let (by, bx) = (w[1].y, w[1].x);
                // Edge crosses the scanline (half-open interval to avoid double-counting vertices).
                if (ay <= lat && by > lat) || (by <= lat && ay > lat) {
                    let t = (lat - ay) / (by - ay);
                    xs.push(ax + t * (bx - ax));
                }
            }
        }

        xs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

        // Fill between alternating pairs (even-odd rule).
        for chunk in xs.chunks_exact(2) {
            let [x0, x1] = [chunk[0], chunk[1]];
            let lon_i_start = ((x0 + 180.0) / GRID_RES) as usize;
            let lon_i_end = (((x1 + 180.0) / GRID_RES) as usize).min(GRID_LON - 1);
            for lon_i in lon_i_start..=lon_i_end {
                let cell = &mut grid[lat_i * GRID_LON + lon_i];
                if *cell == CountryId::UNKNOWN.0 {
                    *cell = id;
                } else if *cell != id {
                    *cell = AMBIGUOUS;
                }
            }
        }
    }

    // ── Column scanlines: mark border cells ───────────────────────────────────
    // For each column, find where polygon edges cross the column's centre
    // longitude and mark those cells AMBIGUOUS.  This catches edges that run
    // mostly east-west and would not be detected by the row scanline fill alone.
    for lon_i in lon_i_min..=lon_i_max {
        let lon = (lon_i as f64 + 0.5) * GRID_RES - 180.0;

        for ring in &rings {
            let coords = ring.0.as_slice();
            for w in coords.windows(2) {
                let (ay, ax) = (w[0].y, w[0].x);
                let (by, bx) = (w[1].y, w[1].x);
                if (ax <= lon && bx > lon) || (bx <= lon && ax > lon) {
                    let t = (lon - ax) / (bx - ax);
                    let y = ay + t * (by - ay);
                    let lat_i = (((y + 90.0) / GRID_RES) as usize).min(GRID_LAT - 1);
                    grid[lat_i * GRID_LON + lon_i] = AMBIGUOUS;
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LookupError {
    #[error("failed to read country boundaries file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse GeoJSON: {0}")]
    GeoJson(#[from] Box<geojson::Error>),
    #[error("expected a GeoJSON FeatureCollection")]
    NotAFeatureCollection,
    #[error("failed to convert geometry to geo types")]
    GeometryConversion,
}
