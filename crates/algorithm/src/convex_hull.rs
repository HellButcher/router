/// Compute the convex hull of `points` (any `[f32; 2]` pairs) using Andrew's
/// Monotone Chain algorithm.  Returns vertices in counter-clockwise order,
/// closed (last element equals first).
///
/// Returns an empty vec for 0 points, or a 2-element closed vec when all
/// points are collinear.
///
/// Points are translated relative to their bounding-box centre before
/// computing so that cross-product arithmetic operates on small differences
/// rather than large absolute coordinates, improving f32 precision.
pub fn convex_hull(mut points: Vec<[f32; 2]>) -> Vec<[f32; 2]> {
    // Sort lexicographically by (x, y) — required by the monotone chain.
    // Use total_cmp so NaN/inf don't break the sort.
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup_by(|a, b| a[0] == b[0] && a[1] == b[1]);

    let n = points.len();
    if n == 0 {
        return points;
    }
    if n < 3 {
        let first = points[0];
        points.push(first);
        return points;
    }

    // Translate to bounding-box midpoint so differences in cross2d are computed
    // on small values instead of large absolute coordinates.  For geographic
    // lat/lon inputs (e.g. lat ≈ 48, lon ≈ 11) this restores the precision lost
    // when subtracting similar large f32 values.
    let min = points[0]; // already sorted; first point has min x (and min y for equal x)
    let max = points[n - 1];
    let cx = (min[0] + max[0]) * 0.5;
    let cy = (min[1] + max[1]) * 0.5;
    let pts: Vec<[f32; 2]> = points.iter().map(|p| [p[0] - cx, p[1] - cy]).collect();

    // Andrew's monotone chain: build lower hull (left-to-right) then upper hull
    // (right-to-left).  `cross2d <= 0` removes right turns and collinear triples,
    // producing a strict convex hull (no collinear edge points).
    let build_half = |iter: &mut dyn Iterator<Item = &[f32; 2]>| -> Vec<[f32; 2]> {
        let mut hull: Vec<[f32; 2]> = Vec::new();
        for &p in iter {
            while hull.len() >= 2 {
                let n = hull.len();
                if cross2d(hull[n - 2], hull[n - 1], p) <= 0.0 {
                    hull.pop();
                } else {
                    break;
                }
            }
            hull.push(p);
        }
        hull
    };

    let mut lower = build_half(&mut pts.iter());
    let mut upper = build_half(&mut pts.iter().rev());

    // Each half includes both endpoints; remove the duplicates before merging.
    lower.pop();
    upper.pop();
    lower.extend_from_slice(&upper);

    // Close the polygon.
    if lower.len() >= 2 {
        let first = lower[0];
        lower.push(first);
    }

    // Translate back.
    for p in &mut lower {
        p[0] += cx;
        p[1] += cy;
    }

    lower
}

/// Z-component of the cross product of vectors (o→a) and (o→b).
/// Positive = CCW turn, negative = CW turn, zero = collinear.
#[inline]
fn cross2d(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn square_with_interior_point() {
        let pts = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]];
        let hull = convex_hull(pts);
        assert_eq!(hull.first(), hull.last(), "polygon must be closed");
        assert!(
            !hull.contains(&[0.5, 0.5]),
            "interior point must not appear"
        );
        assert_eq!(hull.len(), 5); // 4 corners + closing repeat
    }

    #[test]
    fn collinear_points() {
        // All points on a horizontal line → degenerate hull: only the two endpoints.
        let pts = vec![[0.0, 0.0], [1.0, 0.0], [2.0, 0.0], [3.0, 0.0]];
        let hull = convex_hull(pts);
        assert_eq!(hull.first(), hull.last(), "polygon must be closed");
        assert!(hull.contains(&[0.0, 0.0]));
        assert!(hull.contains(&[3.0, 0.0]));
        assert!(
            !hull.contains(&[1.0, 0.0]),
            "intermediate collinear point must not appear"
        );
    }

    #[test]
    fn collinear_on_last_edge() {
        // Rectangle with a midpoint on the top edge — collinear on the last
        // angular segment from the pivot.
        let pts = vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0], [1.0, 2.0], [0.0, 2.0]];
        let hull = convex_hull(pts);
        assert_eq!(hull.first(), hull.last());
        // Strict hull: midpoint on top edge must not appear.
        assert!(!hull.contains(&[1.0, 2.0]));
        assert_eq!(hull.len(), 5); // 4 corners + close
    }

    #[test]
    fn empty() {
        assert!(convex_hull(vec![]).is_empty());
    }

    #[test]
    fn single_point() {
        let hull = convex_hull(vec![[1.0, 2.0]]);
        assert_eq!(hull, vec![[1.0, 2.0], [1.0, 2.0]]);
    }

    #[test]
    fn duplicates_removed() {
        let pts = vec![[0.0, 0.0], [0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
        let hull = convex_hull(pts);
        assert_eq!(hull.first(), hull.last());
        assert_eq!(hull.len(), 4); // triangle: 3 corners + close
    }

    #[test]
    fn geographic_coordinates() {
        // Simulate isochrone points near Munich — large absolute values, tiny
        // differences.  Graham scan on raw f32 can flip cross-product signs here;
        // the translation step in monotone chain prevents that.
        let pts = vec![
            [48.1370, 11.5750],
            [48.1380, 11.5760],
            [48.1360, 11.5760],
            [48.1370, 11.5770],
            [48.1370, 11.5730],
            [48.1350, 11.5750], // should be on hull
            [48.1390, 11.5750], // should be on hull
        ];
        let hull = convex_hull(pts);
        assert_eq!(hull.first(), hull.last(), "polygon must be closed");
        assert!(hull.contains(&[48.1350, 11.5750]));
        assert!(hull.contains(&[48.1390, 11.5750]));
        // Interior points must not appear on the strict hull.
        assert!(!hull.contains(&[48.1370, 11.5760]));
    }
}
