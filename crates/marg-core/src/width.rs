//! Effective-width observer (G§6.2, plan/03-model-design.md §10).
//!
//! `W_eff(x) = W − Σ w_i(x) − c_shy`: the width of the carriageway less
//! what road objects take, less the shy distance kept from them. Nothing
//! in the model uses `W_eff`; it is an emergent check. Per bin along the
//! road this observer reports the three terms as the model predicts them
//! (the shy distance is the lateral-clearance model's, for the vehicles that
//! actually passed) next to what the vehicles did: how close they came to
//! the objects, and the band of the carriageway they used.

use crate::agents::AgentStore;
use crate::model::lateral::edge_gap;
use crate::objects::Objects;
use crate::output::EffectiveWidthRecord;
use crate::params::{CoreParams, Drawn};
use marg_net::Network;
use marg_params::VehicleType;
use marg_params::spec::EffectiveWidthSpec;

/// The share of the time vehicles spent beside an object that defines the
/// observed shy distance: the gap that the closest 5% came within.
const CLOSEST_SHARE: f64 = 0.05;

#[derive(Debug, Clone)]
struct Observer {
    id: String,
    ribbon: usize,
    s_from: f64,
    s_to: f64,
    bin: f64,
    /// Mean carriageway width per bin.
    road_width: Vec<f64>,
    /// Width blocked by objects per bin now, and its integral over time.
    blocked_now: Vec<f64>,
    blocked: Vec<f64>,
    /// Time vehicles spent in the bin, and distance they covered there.
    time: Vec<f64>,
    distance: Vec<f64>,
    used_lo: Vec<f64>,
    used_hi: Vec<f64>,
    /// Time vehicles spent beside an object, and the time integrals of
    /// their desired and minimum clearance to it.
    beside: Vec<f64>,
    desired: Vec<f64>,
    minimum: Vec<f64>,
    gap_min: Vec<f64>,
    /// Per bin, time by lateral gap to the nearest object beside the
    /// vehicle, in cells of `cell` m (the last cell is open-ended).
    gap_hist: Vec<f64>,
}

/// Every effective-width observer of a scenario.
#[derive(Debug, Clone, Default)]
pub struct WidthObservers {
    observers: Vec<Observer>,
    /// Lateral resolution of the gap histograms (m).
    cell: f64,
    cells: usize,
    /// A strip narrower than this is of no use to any vehicle.
    narrowest: f64,
    /// Objects' generation the blocked widths were worked out for.
    generation: Option<u64>,
    measured_time: f64,
}

impl WidthObservers {
    pub fn new(
        specs: &[EffectiveWidthSpec],
        net: &Network,
        p: &CoreParams,
    ) -> Result<Self, String> {
        let narrowest = VehicleType::ALL
            .iter()
            .map(|t| p.draw_dist(Drawn::Width, t.index()).mean())
            .fold(f64::INFINITY, f64::min);
        let cell = p.lateral_histogram_bin;
        let mut widest: f64 = 0.0;
        let mut observers = Vec::new();
        for (k, spec) in specs.iter().enumerate() {
            let bad = |m: String| format!("effective-width observer `{}`: {m}", spec.id);
            if spec.id.is_empty() || specs[..k].iter().any(|o| o.id == spec.id) {
                return Err(bad("ids must be unique and not empty".into()));
            }
            let ribbon = net
                .index(&spec.ribbon)
                .map_err(|_| bad(format!("unknown ribbon `{}`", spec.ribbon)))?;
            let rib = net.ribbon(ribbon);
            if !(spec.s_from >= 0.0 && spec.s_to > spec.s_from && spec.s_to <= rib.length()) {
                return Err(bad(format!(
                    "needs 0 <= s_from < s_to <= {} (the ribbon's length)",
                    rib.length()
                )));
            }
            if spec.bin_m.is_nan() || spec.bin_m <= 0.0 {
                return Err(bad("bin_m must be positive".into()));
            }
            let n = ((spec.s_to - spec.s_from) / spec.bin_m).ceil() as usize;
            // Mean width over the bin from its ends and the profile points in it.
            let road_width = (0..n)
                .map(|b| {
                    let from = spec.s_from + b as f64 * spec.bin_m;
                    let to = (from + spec.bin_m).min(spec.s_to);
                    let mut at: Vec<f64> = rib
                        .width_points()
                        .iter()
                        .map(|w| w.s)
                        .filter(|s| *s > from && *s < to)
                        .collect();
                    at.insert(0, from);
                    at.push(to);
                    let width = |s: f64| {
                        let (lo, hi) = rib.y_bounds(s);
                        hi - lo
                    };
                    at.windows(2)
                        .map(|w| 0.5 * (width(w[0]) + width(w[1])) * (w[1] - w[0]))
                        .sum::<f64>()
                        / (to - from)
                })
                .collect::<Vec<f64>>();
            widest = road_width.iter().copied().fold(widest, f64::max);
            observers.push(Observer {
                id: spec.id.clone(),
                ribbon,
                s_from: spec.s_from,
                s_to: spec.s_to,
                bin: spec.bin_m,
                road_width,
                blocked_now: vec![0.0; n],
                blocked: vec![0.0; n],
                time: vec![0.0; n],
                distance: vec![0.0; n],
                used_lo: vec![f64::INFINITY; n],
                used_hi: vec![f64::NEG_INFINITY; n],
                beside: vec![0.0; n],
                desired: vec![0.0; n],
                minimum: vec![0.0; n],
                gap_min: vec![f64::INFINITY; n],
                gap_hist: Vec::new(),
            });
        }
        let cells = if observers.is_empty() {
            0
        } else {
            (widest / cell).ceil() as usize + 1
        };
        for o in &mut observers {
            o.gap_hist = vec![0.0; o.time.len() * cells];
        }
        Ok(Self {
            observers,
            cell,
            cells,
            narrowest,
            generation: None,
            measured_time: 0.0,
        })
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.observers.is_empty()
    }

    /// Record one step of length `dt` (the state after the step).
    #[allow(clippy::too_many_arguments)]
    pub fn observe(
        &mut self,
        a: &AgentStore,
        orders: &[Vec<u32>],
        objects: &Objects,
        net: &Network,
        prev: &[Option<usize>],
        p: &CoreParams,
        dt: f64,
    ) {
        if self.observers.is_empty() {
            return;
        }
        if self.generation != Some(objects.generation()) {
            self.generation = Some(objects.generation());
            for o in &mut self.observers {
                o.block(objects, net, prev, self.narrowest);
            }
        }
        self.measured_time += dt;
        let (cell, cells) = (self.cell, self.cells);
        for o in &mut self.observers {
            for (now, sum) in o.blocked_now.iter().zip(&mut o.blocked) {
                *sum += now * dt;
            }
            // The order is that of the start of the step: not relied on.
            for &i in &orders[o.ribbon] {
                let i = i as usize;
                if a.s[i] < o.s_from || a.s[i] >= o.s_to {
                    continue;
                }
                let b = (((a.s[i] - o.s_from) / o.bin) as usize).min(o.time.len() - 1);
                // Bodies as if straight: the kerb bounds a body's unturned
                // width (ADR-0010 §4), so the corners of a long vehicle that
                // turns can cross its line, and a turned body's lateral
                // extent overstates how close it is to what it is beside.
                let (y, hw, class) = (a.y[i], 0.5 * a.width[i], usize::from(a.class[i]));
                o.time[b] += dt;
                o.distance[b] += a.v[i] * dt;
                o.used_lo[b] = o.used_lo[b].min(y - hw);
                o.used_hi[b] = o.used_hi[b].max(y + hw);
                // The nearest object the vehicle is beside.
                let mut nearest: Option<(f64, Option<usize>)> = None;
                let half = 0.5 * a.length[i];
                objects.near(net, prev, o.ribbon, a.s[i], half, half, |obj, _| {
                    if !obj.blocks[class] {
                        return;
                    }
                    let gap = edge_gap(y, hw, obj.y, 0.5 * obj.width);
                    if nearest.is_none_or(|(g, _)| gap < g) {
                        nearest = Some((gap, obj.clearance_class));
                    }
                });
                if let Some((gap, clearance_class)) = nearest {
                    let (desired, minimum) =
                        p.object_clearance(class, clearance_class, 0.5 * a.v[i]);
                    o.beside[b] += dt;
                    o.desired[b] += a.clearance_scale[i] * desired * dt;
                    o.minimum[b] += a.clearance_scale[i] * minimum * dt;
                    o.gap_min[b] = o.gap_min[b].min(gap);
                    let c = ((gap.max(0.0) / cell) as usize).min(cells - 1);
                    o.gap_hist[b * cells + c] += dt;
                }
            }
        }
    }

    /// One row per observer and bin.
    pub fn records(&self, net: &Network) -> Vec<EffectiveWidthRecord> {
        let nan_if = |empty: bool, x: f64| if empty { f64::NAN } else { x };
        let mut rows = Vec::new();
        for o in &self.observers {
            for b in 0..o.time.len() {
                let s_from = o.s_from + b as f64 * o.bin;
                let blocked = if self.measured_time > 0.0 {
                    o.blocked[b] / self.measured_time
                } else {
                    o.blocked_now[b]
                };
                let free = o.road_width[b] - blocked;
                let (none, alone) = (o.time[b] == 0.0, o.beside[b] == 0.0);
                let shy_desired = nan_if(alone, o.desired[b] / o.beside[b]);
                // The gap below which the vehicles beside an object spent
                // `CLOSEST_SHARE` of their time, interpolated within its cell.
                let hist = &o.gap_hist[b * self.cells..(b + 1) * self.cells];
                let target = CLOSEST_SHARE * o.beside[b];
                let mut below = 0.0;
                let mut shy_observed = f64::NAN;
                for (c, t) in hist.iter().enumerate() {
                    if !alone && below + t >= target && *t > 0.0 {
                        shy_observed = self.cell * (c as f64 + (target - below) / t);
                        break;
                    }
                    below += t;
                }
                rows.push(EffectiveWidthRecord {
                    observer_id: o.id.clone(),
                    ribbon: net.ribbon(o.ribbon).id.clone(),
                    s_from,
                    s_to: (s_from + o.bin).min(o.s_to),
                    road_width: o.road_width[b],
                    blocked_width: blocked,
                    free_width: free,
                    shy_desired,
                    shy_minimum: nan_if(alone, o.minimum[b] / o.beside[b]),
                    effective_width_model: free - shy_desired,
                    shy_observed,
                    shy_observed_min: nan_if(alone, o.gap_min[b]),
                    effective_width_observed: free - shy_observed,
                    used_lo: nan_if(none, o.used_lo[b]),
                    used_hi: nan_if(none, o.used_hi[b]),
                    time_beside: o.beside[b],
                    time_spent: o.time[b],
                    mean_speed: nan_if(none, o.distance[b] / o.time[b]),
                });
            }
        }
        rows
    }
}

impl Observer {
    /// The width objects block in each bin now: their footprints, and any
    /// strip between a footprint and the kerb, or between two footprints,
    /// that is narrower than the narrowest vehicle. Averaged along the bin.
    fn block(&mut self, objects: &Objects, net: &Network, prev: &[Option<usize>], narrowest: f64) {
        let rib = net.ribbon(self.ribbon);
        for b in 0..self.blocked_now.len() {
            let from = self.s_from + b as f64 * self.bin;
            let to = (from + self.bin).min(self.s_to);
            let mid = 0.5 * (from + to);
            // Footprints in the bin as (rear, front, y_lo, y_hi), cut to it.
            let mut inside: Vec<(f64, f64, f64, f64)> = Vec::new();
            objects.near(
                net,
                prev,
                self.ribbon,
                mid,
                mid - from,
                to - mid,
                |o, ds| {
                    let (rear, front) = (mid + ds - 0.5 * o.length, mid + ds + 0.5 * o.length);
                    if front > from && rear < to {
                        inside.push((
                            rear.max(from),
                            front.min(to),
                            o.y - 0.5 * o.width,
                            o.y + 0.5 * o.width,
                        ));
                    }
                },
            );
            let mut cuts: Vec<f64> = inside.iter().flat_map(|f| [f.0, f.1]).collect();
            cuts.sort_by(f64::total_cmp);
            cuts.dedup();
            let mut blocked = 0.0;
            for w in cuts.windows(2) {
                let x = 0.5 * (w[0] + w[1]);
                let mut spans: Vec<(f64, f64)> = inside
                    .iter()
                    .filter(|f| f.0 <= x && f.1 >= x)
                    .map(|f| (f.2, f.3))
                    .collect();
                spans.sort_by(|p, q| p.0.total_cmp(&q.0));
                let (lo, hi) = rib.y_bounds(x);
                // Walk across the road: what is not a usable strip is blocked.
                let (mut usable, mut at) = (0.0, lo);
                for (y_lo, y_hi) in spans {
                    if y_lo - at >= narrowest {
                        usable += y_lo - at;
                    }
                    at = at.max(y_hi);
                }
                if hi - at >= narrowest {
                    usable += hi - at;
                }
                blocked += ((hi - lo) - usable).max(0.0) * (w[1] - w[0]);
            }
            self.blocked_now[b] = blocked / (to - from);
        }
    }
}
