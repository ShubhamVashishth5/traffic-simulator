//! Road objects and the carriageway ahead (plan/03-model-design.md §10,
//! phase P6.1).
//!
//! An object is a rectangle lying along a ribbon. Vehicles see it as a
//! standing body: the scan of the move phase treats it like a stopped
//! vehicle (a leader to brake for, a side neighbour to keep clear of), with
//! the lateral clearances of ADR-0010. Nothing here scales capacity or flow
//! (ADR-0005): an object only takes width.
//!
//! The same module reads the road's own width profile ahead of a vehicle
//! ([`EdgeAhead`]), so that a narrowing carriageway is seen before the
//! vehicle is in it.

use crate::params::N_CLASSES;
use marg_net::Network;
use marg_params::VehicleClass;
use marg_params::spec::{ObjectKind, ObjectResponse, ObjectSpec};

/// `decision_leader` of an agent whose most constraining leader is the
/// carriageway edge ahead.
pub const EDGE_LEADER: u64 = u64::MAX - 2;
/// `decision_leader` values of objects count down from here by serial number.
const OBJECT_LEADER_BASE: u64 = u64::MAX - 3;

/// One road object.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadObject {
    pub id: String,
    pub kind: ObjectKind,
    pub ribbon: usize,
    /// Centre of the footprint.
    pub s: f64,
    pub y: f64,
    pub length: f64,
    pub width: f64,
    /// Index of the vehicle class whose clearance line and following
    /// multipliers apply; `None` for the pooled line.
    pub clearance_class: Option<usize>,
    /// Whether each vehicle class treats the footprint as unusable.
    pub blocks: [bool; N_CLASSES],
    /// Periods `[from, to)` during which the object is on the road.
    pub present: Vec<(f64, f64)>,
    /// Whether it is on the road this step.
    pub active: bool,
    /// Order of creation: breaks ties and names the object as a leader.
    pub serial: u64,
}

impl RoadObject {
    #[inline]
    pub fn rear(&self) -> f64 {
        self.s - 0.5 * self.length
    }

    #[inline]
    pub fn front(&self) -> f64 {
        self.s + 0.5 * self.length
    }

    /// The value stored as an agent's `decision_leader` when this object is
    /// its most constraining leader.
    #[inline]
    pub fn leader_id(&self) -> u64 {
        OBJECT_LEADER_BASE - self.serial
    }

    fn present_at(&self, t: f64) -> bool {
        self.present.iter().any(|&(from, to)| t >= from && t < to)
    }
}

/// Every road object, with the active ones indexed by ribbon.
#[derive(Debug, Clone, Default)]
pub struct Objects {
    pub all: Vec<RoadObject>,
    /// Per ribbon: indices into `all` of the active objects, sorted by
    /// `(rear station, serial)`.
    by_ribbon: Vec<Vec<u32>>,
    /// Longest active object (m).
    l_max: f64,
    next_serial: u64,
    /// Counts the changes to the set of objects on the road.
    generation: u64,
}

impl Objects {
    pub fn new(n_ribbons: usize) -> Self {
        Self {
            by_ribbon: vec![Vec::new(); n_ribbons],
            ..Self::default()
        }
    }

    /// Whether any object is on the road now.
    #[inline]
    pub fn any_active(&self) -> bool {
        self.l_max > 0.0
    }

    /// A number that changes whenever an object appears on the road or
    /// leaves it.
    #[inline]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Validate `spec` and add it. `from` is when a live addition appears
    /// (`None`: the spec's own periods, or always).
    pub fn add(
        &mut self,
        spec: &ObjectSpec,
        net: &Network,
        from: Option<f64>,
    ) -> Result<(), String> {
        let bad = |m: String| Err(format!("object `{}`: {m}", spec.id));
        if spec.id.is_empty() {
            return Err("an object needs an id".into());
        }
        if self.all.iter().any(|o| o.id == spec.id) {
            return bad("duplicate id".into());
        }
        let Ok(ribbon) = net.index(&spec.ribbon) else {
            return bad(format!("unknown ribbon `{}`", spec.ribbon));
        };
        let finite = [spec.s, spec.y, spec.length, spec.width]
            .iter()
            .all(|x| x.is_finite());
        if !finite || spec.length <= 0.0 || spec.width <= 0.0 {
            return bad("s and y must be finite, length and width positive".into());
        }
        let len = net.ribbon(ribbon).length();
        if spec.s - 0.5 * spec.length < 0.0 || spec.s + 0.5 * spec.length > len {
            return bad(format!(
                "the footprint must lie within its ribbon (0 to {len} m)"
            ));
        }
        if spec
            .present
            .iter()
            .any(|c| c.to_s.is_nan() || c.to_s <= c.from_s)
        {
            return bad("every period needs to_s > from_s".into());
        }
        let mut blocks = [true; N_CLASSES];
        for (class, response) in &spec.response {
            blocks[class.index()] = *response == ObjectResponse::Block;
        }
        let present = match from {
            Some(t) => vec![(t, f64::INFINITY)],
            None if spec.present.is_empty() => vec![(f64::NEG_INFINITY, f64::INFINITY)],
            None => spec.present.iter().map(|c| (c.from_s, c.to_s)).collect(),
        };
        self.all.push(RoadObject {
            id: spec.id.clone(),
            kind: spec.kind,
            ribbon,
            s: spec.s,
            y: spec.y,
            length: spec.length,
            width: spec.width,
            clearance_class: spec.clearance_as.map(VehicleClass::index),
            blocks,
            present,
            active: false,
            serial: self.next_serial,
        });
        self.next_serial += 1;
        Ok(())
    }

    /// End the presence of object `id` at time `t`. Returns whether it was
    /// present (or due to be).
    pub fn remove(&mut self, id: &str, t: f64) -> Result<(), String> {
        let o = self
            .all
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or_else(|| format!("no object `{id}`"))?;
        if !o.present.iter().any(|&(_, to)| to > t) {
            return Err(format!("object `{id}` is already gone"));
        }
        o.present.retain_mut(|(from, to)| {
            *to = to.min(t);
            *to > *from
        });
        Ok(())
    }

    /// Set which objects are on the road at time `t`. Calls `changed` for
    /// every object that appeared or went, in order of creation.
    pub fn update(&mut self, t: f64, mut changed: impl FnMut(&RoadObject)) {
        let mut any = false;
        for o in &mut self.all {
            let now = o.present_at(t);
            if now != o.active {
                o.active = now;
                any = true;
                changed(o);
            }
        }
        if !any {
            return;
        }
        self.generation += 1;
        for list in &mut self.by_ribbon {
            list.clear();
        }
        self.l_max = 0.0;
        for (k, o) in self.all.iter().enumerate() {
            if o.active {
                self.by_ribbon[o.ribbon].push(k as u32);
                self.l_max = self.l_max.max(o.length);
            }
        }
        let all = &self.all;
        for list in &mut self.by_ribbon {
            list.sort_by(|&a, &b| {
                let (a, b) = (&all[a as usize], &all[b as usize]);
                a.rear().total_cmp(&b.rear()).then(a.serial.cmp(&b.serial))
            });
        }
    }

    /// Call `f(object, ds)` for every active object any part of which lies
    /// between `back` m behind and `ahead` m ahead of station `s` on ribbon
    /// `r`, following the chain downstream and one ribbon upstream. `ds` is
    /// the distance from `s` to the object's centre along the chain. Objects
    /// come in a fixed order.
    #[allow(clippy::too_many_arguments)]
    #[inline]
    pub fn near(
        &self,
        net: &Network,
        prev: &[Option<usize>],
        r: usize,
        s: f64,
        back: f64,
        ahead: f64,
        mut f: impl FnMut(&RoadObject, f64),
    ) {
        if !self.any_active() {
            return;
        }
        if s - back < 0.0
            && let Some(q) = prev[r]
        {
            let off = -net.ribbon(q).length();
            for &k in self.by_ribbon[q].iter().rev() {
                let o = &self.all[k as usize];
                if o.front() + off >= s - back {
                    f(o, o.s + off - s);
                } else if o.rear() + self.l_max + off < s - back {
                    break;
                }
            }
        }
        let (mut ribbon, mut off) = (r, 0.0);
        loop {
            let list = &self.by_ribbon[ribbon];
            let from = s - back - self.l_max - off;
            let start = list.partition_point(|&k| self.all[k as usize].rear() < from);
            for &k in &list[start..] {
                let o = &self.all[k as usize];
                if o.rear() + off - s > ahead {
                    return;
                }
                if o.front() + off >= s - back {
                    f(o, o.s + off - s);
                }
            }
            off += net.ribbon(ribbon).length();
            match net.ribbon(ribbon).next {
                Some(n) if off - s <= ahead => ribbon = n,
                _ => return,
            }
        }
    }
}

/// Stations ahead of a vehicle at which the carriageway's bounds are known,
/// most of them where the width profile has a point (fixed capacity, no
/// allocation). Between stations the bounds are linear; two entries at the
/// same station are a step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeAhead {
    n: usize,
    /// Distance ahead of the vehicle's centre (m), ascending from 0.
    ds: [f64; EDGE_POINTS],
    lo: [f64; EDGE_POINTS],
    hi: [f64; EDGE_POINTS],
}

/// Most profile points one look ahead keeps (the nearest).
pub const EDGE_POINTS: usize = 16;

impl EdgeAhead {
    fn push(&mut self, ds: f64, (lo, hi): (f64, f64)) -> bool {
        if self.n == EDGE_POINTS {
            return false;
        }
        self.ds[self.n] = ds;
        self.lo[self.n] = lo;
        self.hi[self.n] = hi;
        self.n += 1;
        true
    }

    /// The carriageway from station `s` of ribbon `r` to `reach` m ahead,
    /// along the chain.
    pub fn gather(net: &Network, r: usize, s: f64, reach: f64) -> Self {
        let mut e = Self {
            n: 0,
            ds: [0.0; EDGE_POINTS],
            lo: [0.0; EDGE_POINTS],
            hi: [0.0; EDGE_POINTS],
        };
        e.push(0.0, net.ribbon(r).y_bounds(s));
        let (mut ribbon, mut off, mut from) = (r, -s, s);
        loop {
            let rib = net.ribbon(ribbon);
            let len = rib.length();
            for w in rib.width_points() {
                if w.s <= from || w.s >= len {
                    continue;
                }
                if !e.push(w.s + off, (w.y_min, w.y_max)) || w.s + off > reach {
                    return e;
                }
            }
            if !e.push(len + off, rib.y_bounds(len)) || len + off > reach {
                return e;
            }
            match rib.next {
                Some(n) => {
                    off += len;
                    ribbon = n;
                    from = 0.0;
                    if !e.push(off, net.ribbon(n).y_bounds(0.0)) {
                        return e;
                    }
                }
                None => return e,
            }
        }
    }

    /// The first distance ahead, not before `from`, at which
    /// `margin(y_min, y_max)` is negative; `margin` must be linear in both.
    /// Beyond the last station known the bounds are taken as constant.
    #[inline]
    pub fn first(&self, from: f64, margin: impl Fn(f64, f64) -> f64) -> Option<f64> {
        let from = from.max(0.0);
        let last = self.n - 1;
        for k in 1..=last {
            let (d0, d1) = (self.ds[k - 1], self.ds[k]);
            if d1 < from {
                continue;
            }
            // The margin where this piece starts, or at `from` if that is later.
            let (start, m0) = if from > d0 {
                let f = (from - d0) / (d1 - d0);
                (
                    from,
                    margin(
                        self.lo[k - 1] + f * (self.lo[k] - self.lo[k - 1]),
                        self.hi[k - 1] + f * (self.hi[k] - self.hi[k - 1]),
                    ),
                )
            } else {
                (d0, margin(self.lo[k - 1], self.hi[k - 1]))
            };
            if m0 < 0.0 {
                return Some(start);
            }
            let m1 = margin(self.lo[k], self.hi[k]);
            if m1 < 0.0 {
                return Some(start + (d1 - start) * m0 / (m0 - m1));
            }
        }
        (margin(self.lo[last], self.hi[last]) < 0.0).then(|| from.max(self.ds[last]))
    }

    /// Distance ahead at which the carriageway stops containing the lateral
    /// interval `[lo_line, hi_line]` (its left edge comes inside `hi_line`
    /// or its right edge inside `lo_line`); `None` if it never does within
    /// the stations known. Zero if it does not contain it here.
    #[inline]
    pub fn contact(&self, lo_line: f64, hi_line: f64) -> Option<f64> {
        let right = self.first(0.0, |lo, _| lo_line - lo);
        let left = self.first(0.0, |_, hi| hi - hi_line);
        match (right, left) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    /// Distance ahead, not before `from`, at which the carriageway becomes
    /// narrower than `width`.
    #[inline]
    pub fn narrower(&self, from: f64, width: f64) -> Option<f64> {
        self.first(from, |lo, hi| hi - lo - width)
    }
}

/// Where each ribbon's carriageway narrows, so that vehicles far from any
/// narrowing skip the look ahead.
#[derive(Debug, Clone, Default)]
pub struct Narrowings {
    /// Per ribbon: station of the end of its last narrowing (−∞ if none).
    last_end: Vec<f64>,
    /// Per ribbon: distance from its end to the start of the next narrowing
    /// downstream (∞ if none).
    next_after: Vec<f64>,
    length: Vec<f64>,
}

impl Narrowings {
    pub fn new(net: &Network) -> Self {
        let n = net.len();
        let mut first_start = vec![f64::INFINITY; n];
        let mut last_end = vec![f64::NEG_INFINITY; n];
        let length: Vec<f64> = net.ribbons.iter().map(|r| r.length()).collect();
        for r in 0..n {
            let rib = net.ribbon(r);
            let len = length[r];
            let mut stations = vec![0.0];
            stations.extend(
                rib.width_points()
                    .iter()
                    .map(|w| w.s)
                    .filter(|&s| s > 0.0 && s < len),
            );
            stations.push(len);
            for w in stations.windows(2) {
                let ((lo0, hi0), (lo1, hi1)) = (rib.y_bounds(w[0]), rib.y_bounds(w[1]));
                if lo1 > lo0 || hi1 < hi0 {
                    first_start[r] = first_start[r].min(w[0]);
                    last_end[r] = w[1];
                }
            }
            if let Some(next) = rib.next {
                let ((lo0, hi0), (lo1, hi1)) = (rib.y_bounds(len), net.ribbon(next).y_bounds(0.0));
                if lo1 > lo0 || hi1 < hi0 {
                    first_start[r] = first_start[r].min(len);
                    last_end[r] = len;
                }
            }
        }
        let mut next_after = vec![f64::INFINITY; n];
        for (r, slot) in next_after.iter_mut().enumerate() {
            let (mut at, mut dist) = (r, 0.0);
            while let Some(next) = net.ribbon(at).next {
                if first_start[next].is_finite() {
                    *slot = dist + first_start[next];
                    break;
                }
                dist += length[next];
                at = next;
            }
        }
        Self {
            last_end,
            next_after,
            length,
        }
    }

    /// Whether the carriageway narrows within `reach` m ahead of station `s`
    /// of ribbon `r`.
    #[inline]
    pub fn within(&self, r: usize, s: f64, reach: f64) -> bool {
        s < self.last_end[r] || self.length[r] - s + self.next_after[r] <= reach
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marg_params::spec::{NetworkSpec, RibbonSpec, WidthPoint};

    fn wp(s: f64, y_min: f64, y_max: f64) -> WidthPoint {
        WidthPoint { s, y_min, y_max }
    }

    /// Two chained ribbons: `a` (200 m) tapers from ±5 to ±3 between 100 and
    /// 150 m; `b` (100 m) starts at ±2 (a step) and is constant.
    fn net() -> Network {
        let ribbon = |id: &str, x0: f64, x1: f64, width, next: Option<&str>| RibbonSpec {
            id: id.into(),
            polyline: vec![[x0, 0.0], [x1, 0.0]],
            width,
            next: next.map(Into::into),
        };
        Network::build(&NetworkSpec {
            ribbons: vec![
                ribbon(
                    "a",
                    0.0,
                    200.0,
                    vec![wp(100.0, -5.0, 5.0), wp(150.0, -3.0, 3.0)],
                    Some("b"),
                ),
                ribbon("b", 200.0, 300.0, vec![wp(0.0, -2.0, 2.0)], None),
            ],
            barriers: vec![],
            objects: vec![],
            parking: vec![],
        })
        .unwrap()
    }

    fn object(id: &str, ribbon: &str, s: f64, y: f64) -> ObjectSpec {
        ObjectSpec {
            id: id.into(),
            kind: ObjectKind::Obstacle,
            ribbon: ribbon.into(),
            s,
            y,
            length: 4.0,
            width: 2.0,
            clearance_as: None,
            response: Default::default(),
            present: vec![],
        }
    }

    #[test]
    fn edge_contact_on_tapers_and_steps() {
        let net = net();
        let e = EdgeAhead::gather(&net, 0, 50.0, 500.0);
        // Inside ±2 nothing is ever in the way.
        assert_eq!(e.contact(-2.0, 2.0), None);
        // The line y = 4 meets the taper half-way: station 125, 75 m ahead.
        assert!((e.contact(-1.0, 4.0).unwrap() - 75.0).abs() < 1e-9);
        assert!((e.contact(-4.0, 1.0).unwrap() - 75.0).abs() < 1e-9);
        // The line y = 2.5 passes the taper and meets the step at the ribbon boundary.
        assert!((e.contact(0.0, 2.5).unwrap() - 150.0).abs() < 1e-9);
        // Already outside here.
        assert_eq!(e.contact(0.0, 5.5), Some(0.0));
        // Only as far as asked (one profile point beyond the reach is kept).
        let near = EdgeAhead::gather(&net, 0, 50.0, 20.0);
        assert_eq!(near.contact(-1.0, 4.9), None);
        // Width: 8 m half-way down the taper, 4 m from the boundary on.
        assert!((e.narrower(0.0, 8.0).unwrap() - 75.0).abs() < 1e-9);
        assert!((e.narrower(0.0, 5.0).unwrap() - 150.0).abs() < 1e-9);
        assert_eq!(e.narrower(0.0, 3.9), None);
        // Starting inside the taper, and past everything that narrows.
        assert!((e.narrower(90.0, 8.0).unwrap() - 90.0).abs() < 1e-9);
        assert!((e.first(80.0, |_, hi| hi - 3.5).unwrap() - 87.5).abs() < 1e-9);
        assert_eq!(e.narrower(200.0, 4.5), Some(200.0));
        assert_eq!(e.narrower(400.0, 4.5), Some(400.0));
        assert_eq!(
            EdgeAhead::gather(&net, 1, 50.0, 500.0).narrower(0.0, 4.5),
            Some(0.0)
        );
    }

    #[test]
    fn narrowings_are_found_along_the_chain() {
        let net = net();
        let n = Narrowings::new(&net);
        assert!(n.within(0, 0.0, 10.0), "before the taper on its own ribbon");
        assert!(n.within(0, 199.0, 10.0), "the step at the boundary");
        assert!(!n.within(1, 0.0, 1000.0), "nothing narrows on or after b");
    }

    #[test]
    fn objects_are_validated_indexed_and_found() {
        let net = net();
        let prev = [None, Some(0)];
        let mut o = Objects::new(net.len());
        o.add(&object("p1", "a", 198.0, 1.0), &net, None).unwrap();
        o.add(&object("p2", "b", 30.0, -1.0), &net, None).unwrap();
        o.add(&object("late", "b", 60.0, 0.0), &net, Some(10.0))
            .unwrap();
        assert!(
            o.add(&object("p1", "a", 50.0, 0.0), &net, None).is_err(),
            "duplicate"
        );
        assert!(
            o.add(&object("x", "zz", 50.0, 0.0), &net, None).is_err(),
            "ribbon"
        );
        assert!(
            o.add(&object("x", "a", 199.0, 0.0), &net, None).is_err(),
            "off the end"
        );
        let mut seen = Vec::new();
        o.update(0.0, |x| seen.push(x.id.clone()));
        assert_eq!(seen, ["p1", "p2"]);
        let found = |o: &Objects, r: usize, s: f64, back: f64, ahead: f64| {
            let mut v = Vec::new();
            o.near(&net, &prev, r, s, back, ahead, |x, ds| {
                v.push((x.id.clone(), ds))
            });
            v
        };
        // From ribbon a, across the boundary.
        assert_eq!(
            found(&o, 0, 150.0, 0.0, 100.0),
            [("p1".to_string(), 48.0), ("p2".to_string(), 80.0)]
        );
        // From the start of b, the object at the end of a is beside or behind.
        assert_eq!(found(&o, 1, 1.0, 5.0, 10.0), [("p1".to_string(), -3.0)]);
        assert!(found(&o, 1, 10.0, 2.0, 10.0).is_empty());
        // Appearing and going.
        o.update(10.0, |_| {});
        assert_eq!(found(&o, 1, 40.0, 0.0, 100.0), [("late".to_string(), 20.0)]);
        o.remove("late", 20.0).unwrap();
        assert!(o.remove("late", 21.0).is_err());
        o.update(20.0, |_| {});
        assert!(found(&o, 1, 40.0, 0.0, 100.0).is_empty());
        assert_ne!(o.all[0].leader_id(), o.all[1].leader_id());
    }
}
