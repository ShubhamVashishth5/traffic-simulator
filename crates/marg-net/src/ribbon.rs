//! One ribbon: polyline reference line plus a piecewise-linear width profile.

use marg_params::spec::{RibbonSpec, WidthPoint};

/// A built ribbon.
#[derive(Debug, Clone, PartialEq)]
pub struct Ribbon {
    pub id: String,
    /// Vertices of the reference line (world metres).
    pts: Vec<[f64; 2]>,
    /// Arc length at each vertex; `cum[0] = 0`.
    cum: Vec<f64>,
    /// Unit direction of each segment.
    dir: Vec<[f64; 2]>,
    /// Width profile, sorted by `s`.
    width: Vec<WidthPoint>,
    /// Index of the successor ribbon in the network.
    pub next: Option<usize>,
}

impl Ribbon {
    /// Build from a spec. The caller (the network) validates the spec first.
    pub(crate) fn from_spec(spec: &RibbonSpec) -> Self {
        let pts = spec.polyline.clone();
        let mut cum = vec![0.0];
        let mut dir = Vec::with_capacity(pts.len() - 1);
        for w in pts.windows(2) {
            let (dx, dy) = (w[1][0] - w[0][0], w[1][1] - w[0][1]);
            let len = dx.hypot(dy);
            dir.push([dx / len, dy / len]);
            cum.push(cum[cum.len() - 1] + len);
        }
        let mut width = spec.width.clone();
        width.sort_by(|a, b| a.s.total_cmp(&b.s));
        Self {
            id: spec.id.clone(),
            pts,
            cum,
            dir,
            width,
            next: None,
        }
    }

    /// Length of the reference line.
    pub fn length(&self) -> f64 {
        self.cum[self.cum.len() - 1]
    }

    /// Stations where the outline changes direction or width: every vertex
    /// (twice, just before and after it, so bends draw cleanly) and every
    /// width-profile station inside the ribbon, sorted. For drawing.
    pub fn outline_stations(&self) -> Vec<f64> {
        let len = self.length();
        // A small fraction of the shortest segment keeps both sides of a vertex on their own segment.
        let eps = self
            .cum
            .windows(2)
            .map(|w| w[1] - w[0])
            .fold(f64::INFINITY, f64::min)
            * 1e-6;
        let mut st: Vec<f64> = vec![0.0, len];
        for &c in &self.cum[1..self.cum.len() - 1] {
            st.push(c - eps);
            st.push(c + eps);
        }
        st.extend(
            self.width
                .iter()
                .map(|w| w.s)
                .filter(|&s| s > 0.0 && s < len),
        );
        st.sort_by(f64::total_cmp);
        st.dedup();
        st
    }

    /// Lateral bounds `(y_min, y_max)` at station `s` (constant beyond the ends).
    pub fn y_bounds(&self, s: f64) -> (f64, f64) {
        let w = &self.width;
        if s <= w[0].s {
            return (w[0].y_min, w[0].y_max);
        }
        let last = w[w.len() - 1];
        if s >= last.s {
            return (last.y_min, last.y_max);
        }
        let i = w.partition_point(|p| p.s <= s);
        let (a, b) = (w[i - 1], w[i]);
        let f = (s - a.s) / (b.s - a.s);
        (
            a.y_min + f * (b.y_min - a.y_min),
            a.y_max + f * (b.y_max - a.y_max),
        )
    }

    fn segment(&self, s: f64) -> usize {
        let i = self.cum.partition_point(|&c| c <= s);
        i.clamp(1, self.cum.len() - 1) - 1
    }

    /// World position and heading (radians, from the x axis) of `(s, y)`.
    /// Outside `[0, length]` the end segments are extended straight.
    pub fn to_world(&self, s: f64, y: f64) -> (f64, f64, f64) {
        let k = self.segment(s);
        let [ux, uy] = self.dir[k];
        let base = self.pts[k];
        let ds = s - self.cum[k];
        // Left normal of the direction (ux, uy) is (-uy, ux).
        (
            base[0] + ux * ds - uy * y,
            base[1] + uy * ds + ux * y,
            uy.atan2(ux),
        )
    }

    /// Project a world point onto the ribbon: `(s, y)` of the nearest point
    /// on the reference line (end segments extended).
    pub fn to_local(&self, x: f64, y: f64) -> (f64, f64) {
        let mut best = (f64::INFINITY, 0.0, 0.0);
        let last = self.dir.len() - 1;
        for (k, &[ux, uy]) in self.dir.iter().enumerate() {
            let p = self.pts[k];
            let seg_len = self.cum[k + 1] - self.cum[k];
            let (rx, ry) = (x - p[0], y - p[1]);
            let mut t = rx * ux + ry * uy;
            if k > 0 {
                t = t.max(0.0);
            }
            if k < last {
                t = t.min(seg_len);
            }
            let (px, py) = (p[0] + ux * t, p[1] + uy * t);
            let d2 = (x - px).powi(2) + (y - py).powi(2);
            if d2 < best.0 {
                let lateral = -uy * (x - px) + ux * (y - py);
                best = (d2, self.cum[k] + t, lateral);
            }
        }
        (best.1, best.2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ribbon(poly: Vec<[f64; 2]>) -> Ribbon {
        Ribbon::from_spec(&RibbonSpec {
            id: "r".into(),
            polyline: poly,
            width: vec![
                WidthPoint {
                    s: 0.0,
                    y_min: -5.0,
                    y_max: 5.0,
                },
                WidthPoint {
                    s: 100.0,
                    y_min: -3.0,
                    y_max: 4.0,
                },
            ],
            next: None,
        })
    }

    #[test]
    fn straight_ribbon_geometry() {
        let r = ribbon(vec![[0.0, 0.0], [200.0, 0.0]]);
        assert_eq!(r.length(), 200.0);
        let (x, y, h) = r.to_world(50.0, 2.0);
        assert!((x - 50.0).abs() < 1e-12 && (y - 2.0).abs() < 1e-12 && h.abs() < 1e-12);
        assert_eq!(r.y_bounds(-10.0), (-5.0, 5.0));
        assert_eq!(r.y_bounds(50.0), (-4.0, 4.5));
        assert_eq!(r.y_bounds(500.0), (-3.0, 4.0));
    }

    proptest::proptest! {
        /// (s, y) → world → (s, y) round trip on a bent ribbon, away from the
        /// inside of the bend where the projection is ambiguous.
        #[test]
        fn round_trip(s in 1.0f64..299.0, y in -4.0f64..4.0) {
            let r = ribbon(vec![[0.0, 0.0], [100.0, 0.0], [200.0, 50.0], [250.0, 150.0]]);
            let s = s.min(r.length() - 1.0);
            // Skip points within |y| of a vertex, where projection can jump to the other segment.
            let near_vertex = r.cum.iter().any(|&c| (s - c).abs() < y.abs() + 0.5);
            proptest::prop_assume!(!near_vertex);
            let (x, wy, _) = r.to_world(s, y);
            let (s2, y2) = r.to_local(x, wy);
            proptest::prop_assert!((s - s2).abs() < 1e-3 && (y - y2).abs() < 1e-3, "s {s} -> {s2}, y {y} -> {y2}");
        }
    }
}
