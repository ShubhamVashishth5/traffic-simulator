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

use crate::model::traits::{linked_score, phone_user};
use crate::params::{CoreParams, Drawn};
use crate::personas::{DISTRACTION, KAPPA, Linked, N_TRAITS, NO_PERSONA};
use crate::rng::{StreamKind, StreamRng};
use marg_params::Dist;
use marg_params::spec::{EntrySpec, FlowInterval, UpstreamSignalSpec};
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
    /// Aggressiveness κ at entry: `mean + sd · (z + persona shift)`.
    pub kappa: f64,
    /// The persona (index into `Persona::ALL`), or `NO_PERSONA`.
    pub persona: u8,
    /// The trait vector `(κ, π, ρ, c, η, skill, δ)` at entry (G§3.1).
    pub traits: [f64; N_TRAITS],
    /// Whether the driver uses a phone (has distraction episodes).
    pub phone_user: bool,
    /// Uniform in [0, 1): the vehicle looks for a parking space along a
    /// stretch whose cruising share is above this (G§6.2).
    pub cruising: f64,
    /// The speed the driver takes a speed hump at (m/s); infinite if the
    /// register has no hump speed.
    pub hump_speed: f64,
    /// Critical gap at an uncontrolled junction (s), before any factor.
    pub critical_gap: f64,
    /// Factor on it against a heavy vehicle.
    pub heavy_factor: f64,
    /// Whether the driver is a late realiser at splits, and how far before
    /// a split it learns its branch (m; infinite if the register has none).
    pub late_realiser: bool,
    pub aware_distance: f64,
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

/// Counter block of the persona draw; the traits after κ use the next six.
const PERSONA_BLOCK: u64 = 16;
const TRAIT_BLOCK: u64 = 17;

/// The stream of an agent's `k`-th draw.
fn draw_rng(seed: u64, id: u64, k: u64) -> StreamRng {
    StreamRng::at(seed, StreamKind::AgentDraws, id, k * DRAW_BLOCK)
}

/// Draw from `d` with the random numbers of block `k`. With a κ link
/// (`r ≠ 0`) the agent draws at the normal score `r·z_κ + √(1 − r²)·ε`,
/// `ε` the block's first normal, so the register's distribution stays the
/// marginal (ADR-0018 part 5); with none, as before.
fn draw_linked(d: &Dist, seed: u64, id: u64, k: u64, r: f64, z_kappa: f64) -> f64 {
    let x = if r == 0.0 {
        d.sample(&mut draw_rng(seed, id, k))
    } else {
        let eps = marg_params::dist::Rng64::standard_normal(&mut draw_rng(seed, id, k));
        d.from_normal_score(linked_score(r, z_kappa, eps))
            .expect("a κ link on a distribution without a quantile is refused at start-up")
    };
    positive(x, d.mean())
}

/// One per-agent parameter, drawn from its own counter block. Re-drawing it
/// after its distribution changed reuses the same random numbers, so the
/// agent keeps its place in the distribution (common random numbers).
/// `z_kappa` is the agent's κ normal score, read if the draw is κ-linked.
pub fn draw_one(
    seed: u64,
    id: u64,
    vtype: VehicleType,
    p: &CoreParams,
    drawn: Drawn,
    z_kappa: f64,
) -> f64 {
    let d = p.draw_dist(drawn, vtype.index());
    let r = drawn.linked().map_or(0.0, |l| p.personas.link[l as usize]);
    draw_linked(d, seed, id, drawn as u64, r, z_kappa)
}

impl Draws {
    /// Draw an agent's parameters from its own stream.
    pub fn sample(seed: u64, id: u64, vtype: VehicleType, p: &CoreParams) -> Self {
        let cp = &p.classes[vtype.class().index()];
        let pp = &p.personas;
        let persona = pp.draw(vtype.class(), draw_rng(seed, id, PERSONA_BLOCK).uniform());
        let mut z = [0.0; N_TRAITS];
        z[KAPPA] = marg_params::dist::Rng64::standard_normal(&mut draw_rng(seed, id, 9));
        for (t, zt) in z.iter_mut().enumerate().skip(1) {
            *zt = marg_params::dist::Rng64::standard_normal(&mut draw_rng(
                seed,
                id,
                TRAIT_BLOCK + t as u64 - 1,
            ));
        }
        let (traits, score) = pp.traits(persona, z);
        let kappa = traits[KAPPA];
        let z_kappa = score[KAPPA];
        let draw = |drawn| draw_one(seed, id, vtype, p, drawn, z_kappa);
        let late_realiser = draw_rng(seed, id, 14).uniform() < p.junction.late_share.unwrap_or(0.0);
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
            kappa,
            persona: persona.map_or(NO_PERSONA, |q| q.index() as u8),
            traits,
            phone_user: pp
                .phone_share
                .is_some_and(|share| phone_user(score[DISTRACTION], share)),
            cruising: draw_rng(seed, id, 10).uniform(),
            hump_speed: p.road.hump_speed.as_ref().map_or(f64::INFINITY, |d| {
                positive(d.sample(&mut draw_rng(seed, id, 11)), d.mean())
            }),
            critical_gap: draw_linked(
                &p.junction.critical_gap[vtype.class().index()],
                seed,
                id,
                12,
                pp.link[Linked::CriticalGap as usize],
                z_kappa,
            ),
            heavy_factor: p.junction.heavy_factor.sample(&mut draw_rng(seed, id, 13)),
            late_realiser,
            aware_distance: {
                let d = if late_realiser {
                    p.junction.late_awareness.as_ref()
                } else {
                    p.junction.awareness.as_ref()
                };
                d.map_or(f64::INFINITY, |d| {
                    positive(d.sample(&mut draw_rng(seed, id, 15)), d.mean())
                })
            },
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
    /// The composition now (shares by type, normalised), as the scenario set
    /// it or the latest live change.
    pub composition: TypeShares,
    /// A signal upstream that is not simulated: arrivals wait unless it
    /// shows green (G§7.3).
    pub gate: Option<UpstreamSignalSpec>,
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
    #[error("entry `{0}`: a live flow must be a number from 0 to {MAX_LIVE_FLOW} veh/h")]
    BadLiveFlow(String),
}

/// Type shares (normalised), in the order of `VehicleType::ALL`.
pub type TypeShares = Vec<(VehicleType, f64)>;

/// One class's part of a composition: the class, its share, and its types
/// with cumulative within-class probability.
type ClassSplit = (VehicleClass, f64, Vec<(VehicleType, f64)>);

/// One entry's demand now (for the live viewer).
#[derive(Debug, Clone, PartialEq)]
pub struct EntryDemand {
    pub id: String,
    /// Flow now (veh/h).
    pub veh_per_h: f64,
    pub composition: TypeShares,
}

/// Upper bound on a live flow change (veh/h at one entry): far above any
/// entry's capacity, so it only catches typing errors.
pub const MAX_LIVE_FLOW: f64 = 50_000.0;

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
            let composition = normalise(&e.id, e.composition.iter().map(|(&t, &w)| (t, w)))
                .map_err(|err| err.to_string())?;
            for (c, share, cumulative) in by_class(&composition) {
                if share <= 0.0 {
                    continue;
                }
                let mut p = ClassProcess::new(e_idx, c, share, cumulative, params, seed);
                p.next_t = p.headway_from(0.0, &e.flow, &mut 0);
                processes.push(p);
            }
            if let Some(sig) = &e.upstream_signal {
                let timing_ok = sig.cycle_s > 0.0
                    && sig.green_s > 0.0
                    && sig.amber_s >= 0.0
                    && sig.green_s + sig.amber_s < sig.cycle_s
                    && sig.offset_s.is_finite();
                if !timing_ok {
                    return Err(format!(
                        "entry `{}`: upstream_signal needs cycle_s > green_s + amber_s, green_s > 0, amber_s >= 0",
                        e.id
                    ));
                }
            }
            states.push(EntryState {
                id: e.id.clone(),
                ribbon,
                s: e.s,
                flow: e.flow.clone(),
                pending: VecDeque::new(),
                composition,
                gate: e.upstream_signal.filter(|s| s.stop_line_s.is_none()),
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

    /// The flow of entry `e` at time `t` (veh/h).
    pub fn flow_at(&self, e: usize, t: f64) -> f64 {
        flow_at(&self.entries[e].flow, t)
    }

    /// A live change to entry `e` at time `t` (the viewer's `set_demand`):
    /// from `t` on the entry has the constant flow `veh_per_h` (the rest of
    /// the scenario's flow profile is dropped) and/or the type shares
    /// `composition`. Either may be left unchanged with `None`.
    ///
    /// Every arrival process of the entry then draws its next arrival afresh
    /// from `t` (the exponential part is memoryless, so this is the process
    /// at the new rate). A class new to the entry gets its own process and
    /// stream, so agent ids stay unique and arrivals of other classes keep
    /// their random numbers. Arrivals already waiting to enter stay.
    pub fn set_entry(
        &mut self,
        e: usize,
        t: f64,
        veh_per_h: Option<f64>,
        composition: Option<&[(VehicleType, f64)]>,
        params: &CoreParams,
        seed: u64,
    ) -> Result<(), DemandError> {
        let id = self.entries[e].id.clone();
        if let Some(q) = veh_per_h
            && !(q.is_finite() && (0.0..=MAX_LIVE_FLOW).contains(&q))
        {
            return Err(DemandError::BadLiveFlow(id));
        }
        let composition = composition
            .map(|c| normalise(&id, c.iter().copied()))
            .transpose()?;
        let entry = &mut self.entries[e];
        if let Some(q) = veh_per_h {
            entry.flow.retain(|f| f.from_s < t);
            entry.flow.push(FlowInterval {
                from_s: t.max(0.0),
                veh_per_h: q,
            });
        }
        if let Some(c) = composition {
            for (class, share, cumulative) in by_class(&c) {
                match self
                    .processes
                    .iter_mut()
                    .find(|p| p.entry == e && p.class == class)
                {
                    Some(p) => {
                        p.share = share;
                        if share > 0.0 {
                            p.types = cumulative;
                        }
                    }
                    None if share > 0.0 => self
                        .processes
                        .push(ClassProcess::new(e, class, share, cumulative, params, seed)),
                    None => {}
                }
            }
            entry.composition = c;
        }
        let flow = &self.entries[e].flow;
        let mut saturated = 0;
        for p in self.processes.iter_mut().filter(|p| p.entry == e) {
            p.next_t = if p.share > 0.0 {
                p.headway_from(t, flow, &mut saturated)
            } else {
                f64::INFINITY
            };
        }
        self.saturated_headways += saturated;
        Ok(())
    }
}

/// Check and normalise type shares: non-negative, finite, summing to more
/// than 0. Types keep the order of `VehicleType::ALL`; zero shares are dropped.
fn normalise(
    id: &str,
    shares: impl Iterator<Item = (VehicleType, f64)>,
) -> Result<TypeShares, DemandError> {
    let shares: Vec<_> = shares.collect();
    let total: f64 = shares.iter().map(|(_, w)| w).sum();
    if total.is_nan() || total <= 0.0 || shares.iter().any(|&(_, x)| x < 0.0 || !x.is_finite()) {
        return Err(DemandError::BadComposition(id.to_string()));
    }
    Ok(VehicleType::ALL
        .iter()
        .filter_map(|&t| {
            let w: f64 = shares.iter().filter(|(u, _)| *u == t).map(|(_, w)| w).sum();
            (w > 0.0).then_some((t, w / total))
        })
        .collect())
}

/// Per class: its share of the stream and its types with cumulative
/// within-class probability (empty, share 0, for a class not in `shares`).
fn by_class(shares: &[(VehicleType, f64)]) -> Vec<ClassSplit> {
    VehicleClass::ALL
        .iter()
        .map(|&c| {
            let types: Vec<_> = shares.iter().filter(|(t, _)| t.class() == c).collect();
            let share: f64 = types.iter().map(|(_, w)| w).sum();
            let mut cum = 0.0;
            let cumulative = types
                .iter()
                .map(|&&(t, w)| {
                    cum += w / share;
                    (t, cum)
                })
                .collect();
            (c, share, cumulative)
        })
        .collect()
}

impl ClassProcess {
    /// The process of class `class` at entry `entry`, with its own stream.
    fn new(
        entry: usize,
        class: VehicleClass,
        share: f64,
        types: Vec<(VehicleType, f64)>,
        params: &CoreParams,
        seed: u64,
    ) -> Self {
        let entity = (entry as u64) << 8 | class.index() as u64;
        Self {
            entry,
            class,
            share,
            types,
            min_headway: params.classes[class.index()].min_headway,
            next_t: 0.0,
            seq: 0,
            rng: StreamRng::new(seed, StreamKind::Demand, entity),
        }
    }

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
