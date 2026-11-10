//! Oriented rectangles in ribbon coordinates `(s, y)`: overlap, and time to
//! contact under constant velocity (ADR-0010 §5–6). Uses the separating-axis
//! theorem; the four candidate axes are the two bodies' own axes.

/// Overlaps shallower than this (m) count as touching, not overlapping.
pub const TOUCH_TOLERANCE: f64 = 1e-9; // allow-literal: numerical tolerance, not behaviour

/// A vehicle body: a rectangle centred at `(s, y)`, turned by its heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Body {
    pub s: f64,
    pub y: f64,
    pub half_length: f64,
    pub half_width: f64,
    /// Cosine and sine of the heading relative to the ribbon axis.
    pub cos: f64,
    pub sin: f64,
}

/// Which of a body's axes two bodies last closed on: its long axis (one runs
/// into the other's end) or its short axis (their sides meet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approach {
    Longitudinal,
    Lateral,
}

impl Body {
    pub fn new(s: f64, y: f64, length: f64, width: f64, heading: f64) -> Self {
        Self {
            s,
            y,
            half_length: 0.5 * length,
            half_width: 0.5 * width,
            cos: libm::cos(heading),
            sin: libm::sin(heading),
        }
    }

    /// A body whose heading is given by its tangent.
    pub fn from_tan(s: f64, y: f64, length: f64, width: f64, heading_tan: f64) -> Self {
        let cos = 1.0 / (1.0 + heading_tan * heading_tan).sqrt();
        Self {
            s,
            y,
            half_length: 0.5 * length,
            half_width: 0.5 * width,
            cos,
            sin: heading_tan * cos,
        }
    }

    /// Half of the body's extent along the unit axis `(ax, ay)`.
    #[inline]
    fn radius(&self, ax: f64, ay: f64) -> f64 {
        self.half_length * (self.cos * ax + self.sin * ay).abs()
            + self.half_width * (self.cos * ay - self.sin * ax).abs()
    }

    #[inline]
    fn axes(&self) -> [(f64, f64, Approach); 2] {
        [
            (self.cos, self.sin, Approach::Longitudinal),
            (-self.sin, self.cos, Approach::Lateral),
        ]
    }
}

/// Depth by which two bodies overlap (m): the smallest overlap of their
/// projections over the four axes. Not positive when they are apart.
pub fn penetration(a: &Body, b: &Body) -> f64 {
    let (ds, dy) = (b.s - a.s, b.y - a.y);
    let mut depth = f64::INFINITY;
    for (ax, ay, _) in a.axes().into_iter().chain(b.axes()) {
        let d = (ds * ax + dy * ay).abs();
        depth = depth.min(a.radius(ax, ay) + b.radius(ax, ay) - d);
        if depth <= 0.0 {
            break;
        }
    }
    depth
}

/// Whether two bodies overlap by more than [`TOUCH_TOLERANCE`].
pub fn overlap(a: &Body, b: &Body) -> bool {
    penetration(a, b) > TOUCH_TOLERANCE
}

/// Time until `a` and `b` first touch if `b` keeps moving at `(dvs, dvy)`
/// relative to `a` and neither turns, with the axis they close on last.
/// `Some((0, _))` if they already overlap; `None` if they never touch.
pub fn time_to_contact(a: &Body, b: &Body, dvs: f64, dvy: f64) -> Option<(f64, Approach)> {
    let (ds, dy) = (b.s - a.s, b.y - a.y);
    let (mut t_in, mut t_out) = (f64::NEG_INFINITY, f64::INFINITY);
    let mut approach = Approach::Longitudinal;
    for (ax, ay, kind) in a.axes().into_iter().chain(b.axes()) {
        let d = ds * ax + dy * ay;
        let r = a.radius(ax, ay) + b.radius(ax, ay);
        let vr = dvs * ax + dvy * ay;
        if vr == 0.0 {
            if d.abs() >= r {
                return None;
            }
            continue;
        }
        let (t0, t1) = ((-r - d) / vr, (r - d) / vr);
        let (lo, hi) = if t0 < t1 { (t0, t1) } else { (t1, t0) };
        if lo > t_in {
            t_in = lo;
            approach = kind;
        }
        t_out = t_out.min(hi);
        if t_in > t_out {
            return None;
        }
    }
    if t_out <= 0.0 {
        return None;
    }
    Some((t_in.max(0.0), approach))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn car(s: f64, y: f64, heading: f64) -> Body {
        Body::new(s, y, 4.0, 1.8, heading)
    }

    #[test]
    fn aligned_bodies_overlap_like_boxes() {
        assert!(!overlap(&car(0.0, 0.0, 0.0), &car(4.0, 0.0, 0.0)));
        assert!(overlap(&car(0.0, 0.0, 0.0), &car(3.9, 0.0, 0.0)));
        assert!(!overlap(&car(0.0, 0.0, 0.0), &car(0.0, 1.8, 0.0)));
        assert!(overlap(&car(0.0, 0.0, 0.0), &car(0.0, 1.7, 0.0)));
        assert!((penetration(&car(0.0, 0.0, 0.0), &car(3.5, 0.0, 0.0)) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn turning_swings_the_corners_out() {
        // Side by side with 0.2 m between them: clear when straight, but a
        // 10 m bus turned by 0.2 rad swings its corners 1 m sideways.
        let bus = |h: f64| Body::new(0.0, 0.0, 10.0, 2.4, h);
        let bike = Body::new(4.0, 1.2 + 0.2 + 0.3, 1.9, 0.6, 0.0);
        assert!(!overlap(&bus(0.0), &bike));
        assert!(overlap(&bus(0.2), &bike));
        assert!(!overlap(&bus(-0.2), &bike));
    }

    #[test]
    fn overlap_is_symmetric() {
        let (a, b) = (car(0.0, 0.0, 0.15), car(3.0, 1.5, -0.1));
        assert_eq!(overlap(&a, &b), overlap(&b, &a));
        assert!((penetration(&a, &b) - penetration(&b, &a)).abs() < 1e-12);
    }

    /// Rear-end: follower at 15 m/s, leader at 5 m/s, 20 m bumper to bumper:
    /// TTC = 20 / 10 = 2 s.
    #[test]
    fn rear_end_ttc() {
        let (f, l) = (car(0.0, 0.0, 0.0), car(24.0, 0.3, 0.0));
        let (t, kind) = time_to_contact(&f, &l, 5.0 - 15.0, 0.0).unwrap();
        assert!((t - 2.0).abs() < 1e-12);
        assert_eq!(kind, Approach::Longitudinal);
        // The leader pulling away: never.
        assert!(time_to_contact(&f, &l, 3.0, 0.0).is_none());
    }

    /// Head-on: 10 m/s each way, 40 m between the fronts: TTC = 40 / 20 = 2 s.
    #[test]
    fn head_on_ttc() {
        let (a, b) = (car(0.0, 0.0, 0.0), car(44.0, 0.0, 0.0));
        let (t, kind) = time_to_contact(&a, &b, -20.0, 0.0).unwrap();
        assert!((t - 2.0).abs() < 1e-12);
        assert_eq!(kind, Approach::Longitudinal);
    }

    /// Sideswipe: side by side at the same speed, 1.2 m between the sides,
    /// one drifting towards the other at 0.8 m/s: TTC = 1.5 s.
    #[test]
    fn sideswipe_ttc() {
        let (a, b) = (car(0.0, 0.0, 0.0), car(1.0, 3.0, 0.0));
        let (t, kind) = time_to_contact(&a, &b, 0.0, -0.8).unwrap();
        assert!((t - 1.5).abs() < 1e-12);
        assert_eq!(kind, Approach::Lateral);
        // Drifting apart: never.
        assert!(time_to_contact(&a, &b, 0.0, 0.8).is_none());
    }

    /// A lateral drift that would arrive after the faster body has passed:
    /// the bodies are level for only 0.9 s, the sides meet after 1.5 s.
    #[test]
    fn passing_before_the_sides_meet_is_no_contact() {
        let (a, b) = (car(0.0, 0.0, 0.0), car(1.0, 3.0, 0.0));
        assert!(time_to_contact(&a, &b, 10.0, -0.8).is_none());
        // Slow enough to still be level: contact at 1.5 s.
        let (t, _) = time_to_contact(&a, &b, 1.0, -0.8).unwrap();
        assert!((t - 1.5).abs() < 1e-12);
    }

    #[test]
    fn overlapping_bodies_have_zero_ttc() {
        let (a, b) = (car(0.0, 0.0, 0.0), car(3.0, 0.5, 0.0));
        assert_eq!(time_to_contact(&a, &b, -1.0, 0.0).unwrap().0, 0.0);
    }

    /// With headings, contact comes earlier than for the axis-aligned boxes:
    /// a car turned towards its neighbour leads with its corner.
    #[test]
    fn oriented_contact_uses_the_corner() {
        let straight = time_to_contact(&car(0.0, 0.0, 0.0), &car(0.0, 3.0, 0.0), 0.0, -1.0)
            .unwrap()
            .0;
        let turned = time_to_contact(&car(0.0, 0.0, 0.2), &car(0.0, 3.0, 0.0), 0.0, -1.0)
            .unwrap()
            .0;
        assert!((straight - 1.2).abs() < 1e-12);
        // Corner offset: 2 sin(0.2) + 0.9 cos(0.2) − 0.9 = 0.379 m.
        let corner = 2.0 * 0.2f64.sin() + 0.9 * 0.2f64.cos() - 0.9;
        assert!((turned - (1.2 - corner)).abs() < 1e-9, "{turned}");
    }
}
