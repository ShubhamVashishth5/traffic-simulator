//! Strips beyond a ribbon's width (ADR-0020): footpaths beside the kerb,
//! and the ribbons that carry the other direction over the same ground (an
//! undivided road's other carriageway, a wrong-way path).
//!
//! A ribbon `o` that is the opposite of `r` has `r`'s polyline reversed, so
//! a point `(s, y)` of `r` is `(L − s, −y)` of `o`: the mapping is exact.
//! Vehicles on the two see each other through [`partner_window`], mapped
//! into their own frame and coming towards them.

use crate::agents::{AgentStore, STRIP_FOOTPATH, STRIP_OPPOSITE};
use marg_net::Network;

/// What the network has of strips, and this step's reach of each partner's
/// vehicles into its ribbons.
#[derive(Debug, Clone, Default)]
pub struct Strips {
    /// Any footpath or pairing in the network.
    pub any: bool,
    pub any_opposite: bool,
    /// Per ribbon, aligned with `Ribbon::opposite`: the partner's
    /// carriageway overlaps this one's (a wrong-way path): every pair of
    /// vehicles counts, not only those across the line between them.
    pub overlap: Vec<[bool; 2]>,
    /// Per ribbon, aligned with `Ribbon::opposite`, this step: the furthest
    /// a vehicle of that partner reaches into this ribbon's frame (the
    /// largest `−y + half width`); −∞ with none.
    pub reach_in: Vec<[f64; 2]>,
    /// Per ribbon: it is a wrong-way path (an arm with `wrong_way` leaves
    /// by it).
    pub wrong_way_path: Vec<bool>,
}

impl Strips {
    pub fn new(net: &Network) -> Self {
        let n = net.len();
        let mut overlap = vec![[false; 2]; n];
        for (r, ov) in overlap.iter_mut().enumerate() {
            let rib = net.ribbon(r);
            for (k, &o) in rib.opposite.iter().enumerate().take(2) {
                let ob = net.ribbon(o);
                let len = rib.length();
                // Sample the width profiles where they can change.
                let stations = [0.0, 0.25 * len, 0.5 * len, 0.75 * len, len];
                ov[k] = stations.iter().any(|&s| {
                    let (lo, hi) = rib.y_bounds(s);
                    let (olo, ohi) = ob.y_bounds(len - s);
                    hi.min(-olo) - lo.max(-ohi) > 0.0
                });
            }
        }
        let mut wrong_way_path = vec![false; n];
        for j in &net.junctions {
            for arm in &j.arms {
                if arm.wrong_way_legal.is_some()
                    && let Some(r) = arm.outbound
                {
                    wrong_way_path[r] = true;
                }
            }
        }
        let any_opposite = (0..n).any(|r| !net.ribbon(r).opposite.is_empty());
        Self {
            any: any_opposite || (0..n).any(|r| net.ribbon(r).footpath.is_some()),
            any_opposite,
            overlap,
            reach_in: vec![[f64::NEG_INFINITY; 2]; n],
            wrong_way_path,
        }
    }

    /// How far each partner's vehicles reach into each ribbon this step.
    pub fn update(&mut self, net: &Network, a: &AgentStore, orders: &[Vec<u32>]) {
        if !self.any_opposite {
            return;
        }
        for r in 0..net.len() {
            for (k, &o) in net.ribbon(r).opposite.iter().enumerate().take(2) {
                self.reach_in[r][k] = orders[o]
                    .iter()
                    .map(|&j| -a.y[j as usize] + a.half_width[j as usize])
                    .fold(f64::NEG_INFINITY, f64::max);
            }
        }
    }

    /// The partner of ribbon `r` whose carriageway only meets its own at a
    /// line (an undivided road's other carriageway), if any.
    pub fn abutting(&self, net: &Network, r: usize) -> Option<(usize, usize)> {
        net.ribbon(r)
            .opposite
            .iter()
            .enumerate()
            .take(2)
            .find(|&(k, _)| !self.overlap[r][k])
            .map(|(k, &o)| (k, o))
    }
}

/// The road's edges for a vehicle using strip `strip` (one of `STRIP_*`)
/// at station `s` of ribbon `r` (ADR-0020 part 1).
pub fn edges(net: &Network, strips: &Strips, r: usize, s: f64, strip: u8) -> (f64, f64) {
    let rib = net.ribbon(r);
    let (lo, hi) = rib.y_bounds(s);
    match strip {
        STRIP_FOOTPATH => match &rib.footpath {
            Some(f) if f.open_at(s) => (lo, hi + f.width),
            _ => (lo, hi),
        },
        STRIP_OPPOSITE => match strips.abutting(net, r) {
            Some((_, o)) => {
                let ob = net.ribbon(o);
                let (_, ohi) = ob.y_bounds(ob.length() - s);
                (lo.min(-ohi), hi)
            }
            None => (lo, hi),
        },
        _ => (lo, hi),
    }
}

/// Where the strip a vehicle uses ends ahead of station `s` of ribbon `r`:
/// the footpath's open stretch, or the ribbon's end.
pub fn strip_end(net: &Network, r: usize, s: f64, strip: u8) -> f64 {
    let rib = net.ribbon(r);
    let end = rib.length();
    match (strip, &rib.footpath) {
        (STRIP_FOOTPATH, Some(f)) => f.open_until(s).min(end),
        _ => end,
    }
}

/// Every vehicle on partner `o` of ribbon `r` whose mirrored centre is
/// within `[s − back, s + ahead]` of `r`, from the furthest ahead back:
/// `f(j, ds, y)` with `ds` its centre's distance ahead (negative: behind)
/// and `y` its lateral position, both in `r`'s frame. It moves towards the
/// vehicle at its own speed.
#[inline]
#[allow(clippy::too_many_arguments)]
pub fn partner_window(
    net: &Network,
    a: &AgentStore,
    orders: &[Vec<u32>],
    o: usize,
    s: f64,
    back: f64,
    ahead: f64,
    mut f: impl FnMut(usize, f64, f64),
) {
    let len = net.ribbon(o).length();
    let order = &orders[o];
    // Ascending in the partner's station is descending in ours.
    let from = order.partition_point(|&j| a.s[j as usize] < len - s - ahead);
    for &j in &order[from..] {
        let j = j as usize;
        let mirrored = len - a.s[j];
        if mirrored < s - back {
            break;
        }
        f(j, mirrored - s, -a.y[j]);
    }
}

/// The standing queue ahead of a vehicle at `s` (length `l`) on ribbon `r`
/// within `horizon` m: the lengths and standstill gaps of the standing
/// vehicles ahead whose bodies reach into the band `[lo, hi]` (m).
#[allow(clippy::too_many_arguments)]
pub fn standing_ahead(
    a: &AgentStore,
    orders: &[Vec<u32>],
    stop_speed: f64,
    r: usize,
    s: f64,
    l: f64,
    lo: f64,
    hi: f64,
    horizon: f64,
) -> f64 {
    let order = &orders[r];
    let from = order.partition_point(|&k| a.s[k as usize] <= s);
    let mut queue = 0.0;
    for &k in &order[from..] {
        let k = k as usize;
        if a.s[k] - 0.5 * a.length[k] - (s + 0.5 * l) > horizon {
            break;
        }
        let hk = a.half_width[k];
        if a.v[k] < stop_speed && a.y[k] + hk > lo && a.y[k] - hk < hi {
            queue += a.length[k] + a.standstill_gap[k];
        }
    }
    queue
}
