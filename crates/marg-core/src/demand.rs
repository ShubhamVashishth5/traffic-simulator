//! Arrivals at network entries (G§7.1–7.3, plan/03-model-design.md §12).
//!
//! Each entry runs one shifted-exponential arrival process per vehicle class:
//! headway = class minimum + exponential remainder, at the rate given by the
//! entry's piecewise-constant flow times the class share. The vehicle type
//! within a class is drawn by the within-class shares. Arrivals that cannot
//! enter yet wait in a virtual queue; their spawn delay is recorded.
//!
//! Agent ids are `entry << 48 | class << 40 | sequence`, so an agent's id —
//! and therefore all of its random draws — does not depend on anything else
//! in the scenario (common random numbers across twin arms).

use crate::params::{CoreParams, Drawn, N_CLASSES};
use crate::rng::{StreamKind, StreamRng};
use marg_params::spec::{EntrySpec, FlowInterval};
use marg_params::{VehicleClass, VehicleType};
use std::collections::VecDeque;

/// Seconds per hour, for converting flows in veh/h.
pub const SECONDS_PER_HOUR: f64 = 3600.0;

/// A vehicle waiting to enter (or about to).
#[derive(Debug, Clone)]
pub struct Arrival {
    pub id: u64,
    pub vtype: VehicleType,
    pub t_arrival: f64,
    /// Parameter draws, made once at the first insertion attempt.
    pub draws: Option<Draws>,
}

/// Per-agent parameter draws. **Give a new draw the next block index**:
/// reusing an index changes every agent's values for a given seed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Draws {
    pub length: f64,
    pub width: f64,
    pub v0: f64,
    pub time_gap: f64,
    pub standstill_gap: f64,
    pub max_accel: f64,
    pub comfortable_decel: f64,
    /// Entry lateral position as a fraction of the free width, median side = 0.
    pub lateral_fraction: f64,
    pub median_band_bonus: f64,
}

/// Replace a non-positive draw (possible in a normal's far tail) by the mean.
fn positive(x: f64, mean: f64) -> f64 {
    if x > 0.0 { x } else { mean }
}

/// Counter blocks reserved for each draw in an agent's stream. Draw `k` reads
/// from block `k * DRAW_BLOCK`, so a sampler that consumes a variable number
/// of values (rejection for truncated normals) never shifts the other draws:
/// changing one distribution in an arm changes only that parameter
/// (common random numbers, ADR-0003).
const DRAW_BLOCK: u64 = 1 << 32;

/// The stream of an agent's `k`-th draw.
fn draw_rng(seed: u64, id: u64, k: u64) -> StreamRng {
    StreamRng::at(seed, StreamKind::AgentDraws, id, k * DRAW_BLOCK)
}

/// One per-agent parameter, drawn from its own counter block. Re-drawing it
/// after its distribution changed reuses the same random numbers, so the
/// agent keeps its place in the distribution (common random numbers).
pub fn draw_one(seed: u64, id: u64, vtype: VehicleType, p: &CoreParams, drawn: Drawn) -> f64 {
    let d = p.draw_dist(drawn, vtype.index());
    positive(d.sample(&mut draw_rng(seed, id, drawn as u64)), d.mean())
}

impl Draws {
    /// Draw an agent's parameters from its own stream.
    pub fn sample(seed: u64, id: u64, vtype: VehicleType, p: &CoreParams) -> Self {
        let cp = &p.classes[vtype.class().index()];
        let draw = |drawn| draw_one(seed, id, vtype, p, drawn);
        Draws {
            length: draw(Drawn::Length),
            width: draw(Drawn::Width),
            v0: draw(Drawn::V0),
            time_gap: draw(Drawn::TimeGap),
            standstill_gap: draw(Drawn::StandstillGap),
            max_accel: draw(Drawn::MaxAccel),
            comfortable_decel: draw(Drawn::ComfortableDecel),
            lateral_fraction: cp
                .entry_lateral_position
                .sample(&mut draw_rng(seed, id, 7))
                .clamp(0.0, 1.0),
            median_band_bonus: p.median_band_bonus.sample(&mut draw_rng(seed, id, 8)),
        }
    }
}

/// One class's arrival process at one entry.
#[derive(Debug, Clone)]
struct ClassProcess {
    entry: usize,
    class: VehicleClass,
    share: f64,
    /// Types in this class with cumulative within-class probability.
    types: Vec<(VehicleType, f64)>,
    min_headway: f64,
    next_t: f64,
    seq: u64,
    rng: StreamRng,
}

/// Runtime state of one entry.
#[derive(Debug, Clone)]
pub struct EntryState {
    pub id: String,
    pub ribbon: usize,
    pub s: f64,
    flow: Vec<FlowInterval>,
    pub pending: VecDeque<Arrival>,
}

/// All entries and their arrival processes.
#[derive(Debug, Clone)]
pub struct Demand {
    pub entries: Vec<EntryState>,
    processes: Vec<ClassProcess>,
    /// Headways that hit the class minimum because flow × share exceeded 1/h_min.
    pub saturated_headways: u64,
}

/// Errors in the demand spec.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DemandError {
    #[error("entry `{0}`: composition shares must be non-negative and sum to more than 0")]
    BadComposition(String),
    #[error(
        "entry `{0}`: flow needs at least one interval, starting at 0, with non-negative rising times and flows"
    )]
    BadFlow(String),
}

fn flow_at(flow: &[FlowInterval], t: f64) -> f64 {
    let i = flow.partition_point(|f| f.from_s <= t);
    if i == 0 { 0.0 } else { flow[i - 1].veh_per_h }
}

/// Start of the first interval after `t` with positive flow, if any.
fn next_positive_flow(flow: &[FlowInterval], t: f64) -> Option<f64> {
    flow.iter()
        .find(|f| f.from_s > t && f.veh_per_h > 0.0)
        .map(|f| f.from_s)
}

impl Demand {
    /// Build arrival processes for `entries`; `ribbon_of` maps a ribbon id to its index.
    pub fn new(
        entries: &[EntrySpec],
        ribbon_of: impl Fn(&str) -> Option<usize>,
        params: &CoreParams,
        seed: u64,
    ) -> Result<Self, String> {
        let mut states = Vec::new();
        let mut processes = Vec::new();
        for (e_idx, e) in entries.iter().enumerate() {
            let ribbon = ribbon_of(&e.ribbon)
                .ok_or_else(|| format!("entry `{}`: unknown ribbon `{}`", e.id, e.ribbon))?;
            let flow_ok = !e.flow.is_empty()
                && e.flow[0].from_s == 0.0
                && e.flow.windows(2).all(|w| w[1].from_s > w[0].from_s)
                && e.flow
                    .iter()
                    .all(|f| f.veh_per_h >= 0.0 && f.veh_per_h.is_finite());
            if !flow_ok {
                return Err(DemandError::BadFlow(e.id.clone()).to_string());
            }
            let total: f64 = e.composition.values().sum();
            if total.is_nan()
                || total <= 0.0
                || e.composition.values().any(|&x| x < 0.0 || !x.is_finite())
            {
                return Err(DemandError::BadComposition(e.id.clone()).to_string());
            }
            let mut by_class: [Vec<(VehicleType, f64)>; N_CLASSES] = Default::default();
            for t in VehicleType::ALL {
                if let Some(&w) = e.composition.get(&t)
                    && w > 0.0
                {
                    by_class[t.class().index()].push((t, w / total));
                }
            }
            for c in VehicleClass::ALL {
                let types = &by_class[c.index()];
                if types.is_empty() {
                    continue;
                }
                let share: f64 = types.iter().map(|(_, w)| w).sum();
                let mut cum = 0.0;
                let cumulative = types
                    .iter()
                    .map(|&(t, w)| {
                        cum += w / share;
                        (t, cum)
                    })
                    .collect();
                let entity = (e_idx as u64) << 8 | c.index() as u64;
                let mut p = ClassProcess {
                    entry: e_idx,
                    class: c,
                    share,
                    types: cumulative,
                    min_headway: params.classes[c.index()].min_headway,
                    next_t: 0.0,
                    seq: 0,
                    rng: StreamRng::new(seed, StreamKind::Demand, entity),
                };
                p.next_t = p.headway_from(0.0, &e.flow, &mut 0);
                processes.push(p);
            }
            states.push(EntryState {
                id: e.id.clone(),
                ribbon,
                s: e.s,
                flow: e.flow.clone(),
                pending: VecDeque::new(),
            });
        }
        Ok(Self {
            entries: states,
            processes,
            saturated_headways: 0,
        })
    }

    /// Generate every arrival with `t_arrival < t_end` into the entries' queues.
    pub fn generate(&mut self, t_end: f64) {
        let mut saturated = 0;
        for p in &mut self.processes {
            let flow = &self.entries[p.entry].flow;
            let mut new = Vec::new();
            while p.next_t < t_end {
                let u = p.rng.uniform();
                let vtype = p
                    .types
                    .iter()
                    .find(|(_, c)| u < *c)
                    .map_or(p.types[p.types.len() - 1].0, |(t, _)| *t);
                let id = (p.entry as u64) << 48 | (p.class.index() as u64) << 40 | p.seq;
                p.seq += 1;
                new.push(Arrival {
                    id,
                    vtype,
                    t_arrival: p.next_t,
                    draws: None,
                });
                p.next_t = p.headway_from(p.next_t, flow, &mut saturated);
            }
            let q = &mut self.entries[p.entry].pending;
            q.extend(new);
        }
        self.saturated_headways += saturated;
        for e in &mut self.entries {
            e.pending
                .make_contiguous()
                .sort_by(|a, b| a.t_arrival.total_cmp(&b.t_arrival).then(a.id.cmp(&b.id)));
        }
    }

    /// Number of arrivals still waiting at all entries.
    pub fn pending(&self) -> usize {
        self.entries.iter().map(|e| e.pending.len()).sum()
    }
}

impl ClassProcess {
    /// Time of the next arrival after one at `t`.
    fn headway_from(&mut self, t: f64, flow: &[FlowInterval], saturated: &mut u64) -> f64 {
        let rate = flow_at(flow, t) * self.share / SECONDS_PER_HOUR;
        if rate <= 0.0 {
            return next_positive_flow(flow, t).unwrap_or(f64::INFINITY);
        }
        let mean = 1.0 / rate;
        if mean <= self.min_headway {
            *saturated += 1;
            return t + self.min_headway;
        }
        t + self.min_headway - (mean - self.min_headway) * libm::log(self.rng.uniform_open())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flow_lookup() {
        let f = vec![
            FlowInterval {
                from_s: 0.0,
                veh_per_h: 100.0,
            },
            FlowInterval {
                from_s: 900.0,
                veh_per_h: 0.0,
            },
            FlowInterval {
                from_s: 1800.0,
                veh_per_h: 50.0,
            },
        ];
        assert_eq!(flow_at(&f, 10.0), 100.0);
        assert_eq!(flow_at(&f, 900.0), 0.0);
        assert_eq!(flow_at(&f, 2000.0), 50.0);
        assert_eq!(next_positive_flow(&f, 1000.0), Some(1800.0));
        assert_eq!(next_positive_flow(&f, 2000.0), None);
    }
}
