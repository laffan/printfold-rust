//! Polygon geometry for text-flow regions (port of `textFlow/polygonPath.ts`).

use crate::model::PolygonPoint;

/// Samples per curved edge when flattening.
const FLATTEN_STEPS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

fn point_abs(p: &PolygonPoint, w: f64, h: f64) -> Pt {
    Pt { x: p.x * w, y: p.y * h }
}

fn handle_in_abs(p: &PolygonPoint, w: f64, h: f64) -> Pt {
    match (p.is_smooth(), p.handle_in) {
        (true, Some(hdl)) => Pt { x: hdl.x * w, y: hdl.y * h },
        _ => point_abs(p, w, h),
    }
}

fn handle_out_abs(p: &PolygonPoint, w: f64, h: f64) -> Pt {
    match (p.is_smooth(), p.handle_out) {
        (true, Some(hdl)) => Pt { x: hdl.x * w, y: hdl.y * h },
        _ => point_abs(p, w, h),
    }
}

fn cubic_at(p0: Pt, p1: Pt, p2: Pt, p3: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
    Pt {
        x: w0 * p0.x + w1 * p1.x + w2 * p2.x + w3 * p3.x,
        y: w0 * p0.y + w1 * p1.y + w2 * p2.y + w3 * p3.y,
    }
}

/// One edge of the closed path: straight or cubic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Segment {
    Line(Pt),
    Cubic(Pt, Pt, Pt),
}

/// The closed path as a start point plus edges (absolute coordinates).
pub fn polygon_segments(points: &[PolygonPoint], w: f64, h: f64) -> Option<(Pt, Vec<Segment>)> {
    if points.len() < 2 {
        return None;
    }
    let start = point_abs(&points[0], w, h);
    let mut segs = Vec::with_capacity(points.len());
    for i in 0..points.len() {
        let cur = &points[i];
        let next = &points[(i + 1) % points.len()];
        let target = point_abs(next, w, h);
        if !cur.is_smooth() && !next.is_smooth() {
            segs.push(Segment::Line(target));
        } else {
            segs.push(Segment::Cubic(handle_out_abs(cur, w, h), handle_in_abs(next, w, h), target));
        }
    }
    Some((start, segs))
}

/// Dense polyline approximation of the closed path.
pub fn flatten_polygon(points: &[PolygonPoint], w: f64, h: f64) -> Vec<Pt> {
    let mut flat = Vec::new();
    for i in 0..points.len() {
        let cur = &points[i];
        let next = &points[(i + 1) % points.len()];
        let cur_abs = point_abs(cur, w, h);
        flat.push(cur_abs);
        if !cur.is_smooth() && !next.is_smooth() {
            continue;
        }
        let c1 = handle_out_abs(cur, w, h);
        let c2 = handle_in_abs(next, w, h);
        let next_abs = point_abs(next, w, h);
        for step in 1..FLATTEN_STEPS {
            flat.push(cubic_at(cur_abs, c1, c2, next_abs, step as f64 / FLATTEN_STEPS as f64));
        }
    }
    flat
}

/// Miter-joined polygon offset; positive grows outward. The miter is
/// clamped at `miter_limit * |offset|` so sharp corners don't spike.
pub fn offset_flat_polygon(points: &[Pt], offset: f64, miter_limit: f64) -> Vec<Pt> {
    let n = points.len();
    if n < 3 || offset == 0.0 {
        return points.to_vec();
    }
    let mut signed_area = 0.0;
    for i in 0..n {
        let a = points[i];
        let b = points[(i + 1) % n];
        signed_area += a.x * b.y - b.x * a.y;
    }
    let is_cw = signed_area > 0.0;
    let perp_out = |dx: f64, dy: f64| if is_cw { Pt { x: dy, y: -dx } } else { Pt { x: -dy, y: dx } };
    let max_dist = offset.abs() * miter_limit;

    (0..n)
        .map(|i| {
            let prev = points[(i + n - 1) % n];
            let cur = points[i];
            let next = points[(i + 1) % n];
            let (idx, idy) = (cur.x - prev.x, cur.y - prev.y);
            let il = idx.hypot(idy);
            let il = if il == 0.0 { 1.0 } else { il };
            let (odx, ody) = (next.x - cur.x, next.y - cur.y);
            let ol = odx.hypot(ody);
            let ol = if ol == 0.0 { 1.0 } else { ol };
            let in_n = perp_out(idx / il, idy / il);
            let out_n = perp_out(odx / ol, ody / ol);
            let (mut bx, mut by) = (in_n.x + out_n.x, in_n.y + out_n.y);
            let bl = bx.hypot(by);
            if bl < 1e-6 {
                return Pt { x: cur.x + in_n.x * offset, y: cur.y + in_n.y * offset };
            }
            bx /= bl;
            by /= bl;
            let cos_half = bx * in_n.x + by * in_n.y;
            let mut d = offset / if cos_half == 0.0 { 1.0 } else { cos_half };
            if d.abs() > max_dist {
                d = d.signum() * max_dist;
            }
            Pt { x: cur.x + bx * d, y: cur.y + by * d }
        })
        .collect()
}

/// Leftmost/rightmost scanline intersections at `y` (notches ignored).
pub fn horizontal_extent(points: &[Pt], y: f64) -> Option<(f64, f64)> {
    let mut xs = Vec::new();
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];
        if (p1.y <= y && p2.y > y) || (p2.y <= y && p1.y > y) {
            let t = (y - p1.y) / (p2.y - p1.y);
            xs.push(p1.x + t * (p2.x - p1.x));
        }
    }
    if xs.len() < 2 {
        return None;
    }
    let min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<PolygonPoint> {
        [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)]
            .iter()
            .map(|&(x, y)| PolygonPoint { x, y, ..Default::default() })
            .collect()
    }

    #[test]
    fn extent_of_square() {
        let flat = flatten_polygon(&square(), 100.0, 50.0);
        assert_eq!(flat.len(), 4);
        assert_eq!(horizontal_extent(&flat, 25.0), Some((0.0, 100.0)));
        assert_eq!(horizontal_extent(&flat, 60.0), None);
    }

    #[test]
    fn offset_grows_square() {
        let flat = flatten_polygon(&square(), 10.0, 10.0);
        let grown = offset_flat_polygon(&flat, 1.0, 8.0);
        let (l, r) = horizontal_extent(&grown, 5.0).unwrap();
        assert!((l + 1.0).abs() < 1e-9 && (r - 11.0).abs() < 1e-9);
    }

    #[test]
    fn smooth_points_flatten_densely() {
        let mut pts = square();
        pts[1].corner_type = Some("smooth".into());
        pts[1].handle_in = Some(crate::model::Vec2 { x: 0.8, y: 0.0 });
        pts[1].handle_out = Some(crate::model::Vec2 { x: 1.0, y: 0.2 });
        assert!(flatten_polygon(&pts, 10.0, 10.0).len() > 4);
    }
}
