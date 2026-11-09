//! The step loop (plan/02-architecture.md §4).
//!
//! Each step: **move** (parallel, read-only: accelerations from the current
//! snapshot) → **apply** (integrate) → **observe** (detectors, trajectories,
//! collision guard) → **transitions** (ribbon handover, exits) → **compact**
//! (remove exits, re-sort ribbons) → **demand** (insert arrivals) → **hash**.
//! Only the move phase runs in parallel, and it writes per-agent results in
//! a fixed order, so outputs are bitwise identical for any thread count.

use crate::agents::{AgentStore, NewAgent, REMOVED};
use crate::demand::{Arrival, Demand, Draws, draw_one};
use crate::model::coverage::Coverage;
use crate::model::lateral::{clearance, lateral_gap, leader_weight};
use crate::model::longitudinal::{Idm, combine};
use crate::observe::Detectors;
use crate::output::{
    AgentRecord, DetectorRecord, EventRecord, RunOutputs, TrajectoryChunk, TrajectorySink,
};
use crate::params::{CoreParams, Drawn};
use marg_net::Network;
use marg_params::{ResolvedSpec, Scenario, VehicleClass, VehicleType};
use rayon::prelude::*;

/// Rows buffered before a trajectory chunk is handed to the sink.
const TRAJECTORY_CHUNK_ROWS: usize = 1 << 16;
/// Minimum work per rayon task in the move phase.
const MOVE_MIN_LEN: usize = 256;
/// Below this many agents the move phase runs on the calling thread: the
/// thread-pool round trip costs more than it saves. Results are identical.
const MOVE_PARALLEL_MIN: usize = 4 * MOVE_MIN_LEN;

/// Errors raised while building or running a simulation.
#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("invalid scenario: {0}")]
    InvalidScenario(String),
    #[error("network: {0}")]
    Network(#[from] marg_net::NetError),
    #[error("parameters: {0}")]
    Params(#[from] marg_params::ResolveError),
    #[error("thread pool: {0}")]
    ThreadPool(String),
    #[error("cannot resample `{0}`: {1}")]
    Resample(String, &'static str),
}

/// Detectors with their own (short) interval, observed alongside the
/// scenario's, for the live viewer's dashboards.
struct LiveDetectors {
    det: Detectors,
    interval: f64,
}

/// One other agent considered by an agent's leader scan: the terms of
/// plan/03-model-design.md §2, for the live inspector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Interaction {
    /// Store index of the other agent.
    pub j: u32,
    /// Distance between centres along the ribbon chain (m).
    pub ds: f64,
    /// Bumper-to-bumper gap (m); ≤ 0 means level with the agent (a side neighbour).
    pub gap_s: f64,
    /// Edge-to-edge lateral gap (m); < 0 means the footprints overlap laterally.
    pub lateral_gap: f64,
    /// Speed-dependent lateral clearance `c(v̄)` (m).
    pub clearance: f64,
    /// Leader weight ω; NaN for side neighbours.
    pub weight: f64,
    /// IDM acceleration toward this leader alone (m/s²); NaN for side neighbours.
    pub leader_accel: f64,
}

/// Headline numbers for a finished run.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RunSummary {
    pub steps: u64,
    pub sim_time_s: f64,
    pub spawned: u64,
    pub exited: u64,
    pub agent_steps: u64,
    /// blake3 of every step's state (agent id, s, y, v), in step order (hex).
    pub output_hash: String,
}

struct TrajectoryConfig {
    every_steps: u64,
    from_t: f64,
    s_from: f64,
    s_to: f64,
}

/// A running simulation.
pub struct Simulation {
    scenario: Scenario,
    p: CoreParams,
    net: Network,
    seed: u64,
    dt: f64,
    step: u64,
    t: f64,
    agents: AgentStore,
    /// Agent indices per ribbon, sorted by `(s, id)` at the end of each step.
    orders: Vec<Vec<u32>>,
    demand: Demand,
    detectors: Detectors,
    pool: rayon::ThreadPool,
    work: Vec<(u32, u32)>,
    parallel_min: usize,
    move_buf: Vec<(f64, u32)>,
    remap: Vec<u32>,
    transferred: Vec<u32>,
    records: Vec<AgentRecord>,
    events: Vec<EventRecord>,
    collision_guards: u64,
    spawned: u64,
    exited: u64,
    agent_steps: u64,
    measured_tts: f64,
    measured_ttd: f64,
    max_step: f64,
    hasher: blake3::Hasher,
    hash_buf: Vec<u8>,
    trajectories: Option<TrajectoryConfig>,
    traj: TrajectoryChunk,
    sink: Option<Box<dyn TrajectorySink>>,
    live: Option<LiveDetectors>,
}

impl Simulation {
    /// Build a simulation for a resolved spec with master seed `seed`, using
    /// `threads` worker threads (0 = all cores).
    pub fn new(spec: &ResolvedSpec, seed: u64, threads: usize) -> Result<Self, SimError> {
        let scenario = spec.scenario.clone();
        let bad = |m: &str| SimError::InvalidScenario(m.to_string());
        if !(scenario.dt_s > 0.0 && scenario.duration_s > 0.0) {
            return Err(bad("dt_s and duration_s must be positive"));
        }
        if !(0.0..scenario.duration_s).contains(&scenario.warmup_s) {
            return Err(bad("warmup_s must be in [0, duration_s)"));
        }
        let p = CoreParams::from_spec(spec)?;
        let net = Network::build(&scenario.network)?;
        let ribbon_of = |id: &str| net.index(id).ok();
        let demand = Demand::new(&scenario.demand.entries, ribbon_of, &p, seed)
            .map_err(SimError::InvalidScenario)?;
        let detectors = Detectors::new(
            &scenario.detectors,
            ribbon_of,
            |r, s| net.ribbon(r).y_bounds(s),
            scenario.duration_s,
            scenario.warmup_s,
            p.lateral_histogram_bin,
        )
        .map_err(SimError::InvalidScenario)?;
        if let Some(id) = &scenario.kpi.throughput_detector
            && !detectors.is_screenline(id)
        {
            return Err(SimError::InvalidScenario(format!(
                "kpi.throughput_detector `{id}` is not a screenline"
            )));
        }
        let trajectories = match &scenario.outputs.trajectories {
            None => None,
            Some(tr) => {
                let every = (tr.every_s / scenario.dt_s).round();
                if every.is_nan() || every < 1.0 {
                    return Err(bad("outputs.trajectories.every_s must be at least dt_s"));
                }
                Some(TrajectoryConfig {
                    every_steps: every as u64,
                    from_t: tr.from_t.unwrap_or(scenario.warmup_s),
                    s_from: tr.s_from.unwrap_or(f64::NEG_INFINITY),
                    s_to: tr.s_to.unwrap_or(f64::INFINITY),
                })
            }
        };
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| SimError::ThreadPool(e.to_string()))?;
        let n_ribbons = net.len();
        Ok(Self {
            dt: scenario.dt_s,
            scenario,
            p,
            net,
            seed,
            step: 0,
            t: 0.0,
            agents: AgentStore::default(),
            orders: vec![Vec::new(); n_ribbons],
            demand,
            detectors,
            pool,
            work: Vec::new(),
            parallel_min: MOVE_PARALLEL_MIN,
            move_buf: Vec::new(),
            remap: Vec::new(),
            transferred: Vec::new(),
            records: Vec::new(),
            events: Vec::new(),
            collision_guards: 0,
            spawned: 0,
            exited: 0,
            agent_steps: 0,
            measured_tts: 0.0,
            measured_ttd: 0.0,
            max_step: 0.0,
            hasher: blake3::Hasher::new(),
            hash_buf: Vec::new(),
            trajectories,
            traj: TrajectoryChunk::default(),
            sink: None,
            live: None,
        })
    }

    /// Stream trajectory rows to `sink` (only if the scenario asks for trajectories).
    pub fn set_trajectory_sink(&mut self, sink: Box<dyn TrajectorySink>) {
        self.sink = Some(sink);
    }

    /// Smallest number of agents for which the move phase runs in parallel
    /// (tests set 0 to exercise the parallel path on small scenarios).
    pub fn set_parallel_min(&mut self, agents: usize) {
        self.parallel_min = agents;
    }

    /// Simulated time in seconds.
    pub fn time(&self) -> f64 {
        self.t
    }

    /// Number of agents currently in the network.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Read-only view of the agents (for tests and the live viewer).
    pub fn agents(&self) -> &AgentStore {
        &self.agents
    }

    /// Whether the scenario duration has been reached.
    pub fn finished(&self) -> bool {
        self.t >= self.scenario.duration_s - 0.5 * self.dt
    }

    /// Advance one time step.
    pub fn step(&mut self) {
        let t0 = self.t;
        self.phase_move();
        self.phase_apply();
        self.detectors
            .observe(&self.agents, &self.orders, t0, self.dt, self.max_step);
        if let Some(live) = &mut self.live {
            live.det
                .observe(&self.agents, &self.orders, t0, self.dt, self.max_step);
        }
        self.collision_guard(t0 + self.dt);
        self.step += 1;
        self.t = self.step as f64 * self.dt;
        self.phase_transitions();
        self.phase_compact();
        self.phase_trajectories();
        self.phase_demand();
        self.phase_hash();
    }

    /// Run until the scenario duration is reached.
    pub fn run_to_end(&mut self) -> RunSummary {
        while !self.finished() {
            self.step();
        }
        self.summary()
    }

    /// Summary of the run so far.
    pub fn summary(&self) -> RunSummary {
        RunSummary {
            steps: self.step,
            sim_time_s: self.t,
            spawned: self.spawned,
            exited: self.exited,
            agent_steps: self.agent_steps,
            output_hash: self.hasher.clone().finalize().to_hex().to_string(),
        }
    }

    // ----------------------------------------------------------------- live

    /// Steps taken so far.
    pub fn step_index(&self) -> u64 {
        self.step
    }

    /// Time step (s).
    pub fn dt(&self) -> f64 {
        self.dt
    }

    /// Master seed.
    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The scenario being simulated (overrides already folded into the parameters).
    pub fn scenario(&self) -> &Scenario {
        &self.scenario
    }

    /// The built road network.
    pub fn network(&self) -> &Network {
        &self.net
    }

    /// The parameters in use.
    pub fn params(&self) -> &CoreParams {
        &self.p
    }

    /// Every event so far, in order.
    pub fn events(&self) -> &[EventRecord] {
        &self.events
    }

    /// Replace the parameters during a run (the live viewer's `set_param`;
    /// `spec` is the run's spec with the `live` layer applied). From the next
    /// step, population-level values (pair multipliers, clearance, λ) act on
    /// every agent; per-agent draws apply to agents that have not entered
    /// yet, including those waiting in an entry queue (their cached draws are
    /// dropped and re-drawn from the same random numbers). Agents already in
    /// the network keep their draws unless [`Self::resample`] is called.
    pub fn update_params(&mut self, spec: &ResolvedSpec) -> Result<(), SimError> {
        self.p = CoreParams::from_spec(spec)?;
        for e in &mut self.demand.entries {
            for arr in &mut e.pending {
                arr.draws = None;
            }
        }
        Ok(())
    }

    /// Re-draw the per-agent parameter read from register `path` for every
    /// agent in the network that draws it, from the current distribution and
    /// the agent's own random numbers (so each agent keeps its quantile).
    /// Returns how many agents changed. Dimensions are refused (a vehicle
    /// cannot change size on the road), as are parameters that are not
    /// per-agent draws (they already act on everyone).
    pub fn resample(&mut self, path: &str) -> Result<usize, SimError> {
        let sources: Vec<(Drawn, usize)> = self
            .p
            .draw_sources
            .iter()
            .filter(|d| d.path == path)
            .map(|d| (d.drawn, d.vtype))
            .collect();
        if sources.is_empty() {
            return Err(SimError::Resample(
                path.to_string(),
                "not a per-agent draw; changes to it already apply to every agent",
            ));
        }
        if sources
            .iter()
            .any(|(d, _)| matches!(d, Drawn::Length | Drawn::Width))
        {
            return Err(SimError::Resample(
                path.to_string(),
                "vehicle dimensions cannot change on the road",
            ));
        }
        let mut n = 0;
        for i in 0..self.agents.len() {
            let vt = usize::from(self.agents.vtype[i]);
            for &(drawn, _) in sources.iter().filter(|(_, t)| *t == vt) {
                let a = &mut self.agents;
                let x = draw_one(self.seed, a.id[i], VehicleType::ALL[vt], &self.p, drawn);
                match drawn {
                    Drawn::V0 => a.v0[i] = x + a.v0_bonus[i],
                    Drawn::TimeGap => a.time_gap[i] = x,
                    Drawn::StandstillGap => a.standstill_gap[i] = x,
                    Drawn::MaxAccel => a.max_accel[i] = x,
                    Drawn::ComfortableDecel => a.comfortable_decel[i] = x,
                    Drawn::Length | Drawn::Width => unreachable!("refused above"),
                }
                n += 1;
            }
        }
        Ok(n)
    }

    /// Observe the scenario's detectors a second time with interval
    /// `interval_s` (the live viewer's dashboard window). Results equal a
    /// headless run whose detectors use that interval.
    pub fn enable_live_detectors(&mut self, interval_s: f64) -> Result<(), SimError> {
        let specs: Vec<_> = self
            .scenario
            .detectors
            .iter()
            .map(|d| d.with_interval(interval_s))
            .collect();
        let net = &self.net;
        let det = Detectors::new(
            &specs,
            |id| net.index(id).ok(),
            |r, s| net.ribbon(r).y_bounds(s),
            self.scenario.duration_s,
            self.scenario.warmup_s,
            self.p.lateral_histogram_bin,
        )
        .map_err(SimError::InvalidScenario)?;
        self.live = Some(LiveDetectors {
            det,
            interval: interval_s,
        });
        Ok(())
    }

    /// Number of live-detector intervals that have closed.
    pub fn live_intervals_closed(&self) -> usize {
        match &self.live {
            Some(l) => ((self.t + 0.5 * self.dt) / l.interval).floor() as usize,
            None => 0,
        }
    }

    /// Live-detector records of intervals `k0..k1`.
    pub fn live_detector_records(&self, k0: usize, k1: usize) -> Vec<DetectorRecord> {
        match &self.live {
            Some(l) => l.det.records_in(self.scenario.duration_s, k0, k1),
            None => Vec::new(),
        }
    }

    /// Store index of agent `id`, if it is in the network.
    pub fn agent_index(&self, id: u64) -> Option<usize> {
        self.agents.id.iter().position(|&x| x == id)
    }

    /// The `agents.parquet` row agent `i` would get if the run ended now.
    pub fn agent_record(&self, i: usize) -> AgentRecord {
        self.record(i, None)
    }

    /// Every agent the leader scan of agent `i` considers this step (leaders
    /// and side neighbours), with the terms of its acceleration.
    pub fn interactions(&self, i: usize) -> Vec<Interaction> {
        let r = self.agents.ribbon[i] as usize;
        let Some(k) = self.orders[r].iter().position(|&x| x as usize == i) else {
            return Vec::new();
        };
        let ctx = MoveCtx {
            a: &self.agents,
            orders: &self.orders,
            net: &self.net,
            p: &self.p,
            cut: self.p.interaction_cutoff_distance(),
            l_max: self.agents.length.iter().copied().fold(0.0, f64::max),
        };
        let mut out = Vec::new();
        ctx.accel_with(r, k, i, |x| out.push(x));
        out
    }

    /// Headline KPIs if the run ended now (the same computation as `finish`).
    pub fn kpis_now(&self) -> Vec<(&'static str, f64)> {
        let mut agents = self.records.clone();
        for i in 0..self.agents.len() {
            agents.push(self.record(i, None));
        }
        agents.sort_by_key(|r| r.agent_id);
        self.kpis(&agents)
    }

    // ---------------------------------------------------------------- move

    fn phase_move(&mut self) {
        self.work.clear();
        for (r, order) in self.orders.iter().enumerate() {
            self.work
                .extend((0..order.len() as u32).map(|k| (r as u32, k)));
        }
        {
            let ctx = MoveCtx {
                a: &self.agents,
                orders: &self.orders,
                net: &self.net,
                p: &self.p,
                cut: self.p.interaction_cutoff_distance(),
                l_max: self.agents.length.iter().copied().fold(0.0, f64::max),
            };
            let work = &self.work;
            let buf = &mut self.move_buf;
            let one = |&(r, k): &(u32, u32)| {
                let (r, k) = (r as usize, k as usize);
                ctx.accel(r, k, ctx.orders[r][k] as usize)
            };
            if work.len() < self.parallel_min || self.pool.current_num_threads() == 1 {
                buf.clear();
                buf.extend(work.iter().map(one));
            } else {
                self.pool.install(|| {
                    work.par_iter()
                        .with_min_len(MOVE_MIN_LEN)
                        .map(one)
                        .collect_into_vec(buf)
                });
            }
        }
        for (&(r, k), &(acc, leader)) in self.work.iter().zip(&self.move_buf) {
            let i = self.orders[r as usize][k as usize] as usize;
            self.agents.acc[i] = acc;
            self.agents.phys_leader[i] = leader;
        }
    }

    // --------------------------------------------------------------- apply

    fn phase_apply(&mut self) {
        let dt = self.dt;
        let a = &mut self.agents;
        let measured = self.t >= self.scenario.warmup_s;
        let mut max_step: f64 = 0.0;
        let mut ttd = 0.0;
        for i in 0..a.len() {
            let (v, acc) = (a.v[i], a.acc[i]);
            let v1 = v + acc * dt;
            let ds = if v1 >= 0.0 {
                0.5 * (v + v1) * dt
            } else {
                -v * v / (2.0 * acc)
            };
            a.s_prev[i] = a.s[i];
            a.s[i] += ds;
            a.v[i] = v1.max(0.0);
            a.distance[i] += ds;
            max_step = max_step.max(ds);
            ttd += ds;
            if a.v[i] < self.p.stop_speed_below {
                if a.stop_armed[i] {
                    a.stops[i] += 1;
                    a.stop_armed[i] = false;
                }
            } else if a.v[i] > self.p.stop_rearm_above {
                a.stop_armed[i] = true;
            }
        }
        self.max_step = max_step;
        self.agent_steps += a.len() as u64;
        if measured {
            self.measured_tts += a.len() as f64 * dt;
            self.measured_ttd += ttd;
        }
    }

    /// Resolve any physical overlap with the nearest overlapping leader that
    /// integration produced (should not happen; counted and logged).
    fn collision_guard(&mut self, t: f64) {
        for r in 0..self.orders.len() {
            for k in (0..self.orders[r].len()).rev() {
                let i = self.orders[r][k] as usize;
                let j = self.agents.phys_leader[i];
                if j == REMOVED {
                    continue;
                }
                let j = j as usize;
                let a = &self.agents;
                let offset = self.ribbon_offset(a.ribbon[i] as usize, a.ribbon[j] as usize);
                let gap = a.s[j] + offset - a.s[i] - 0.5 * (a.length[i] + a.length[j]);
                if gap < 0.0 && lateral_gap(a.y[i], a.width[i], a.y[j], a.width[j]) < 0.0 {
                    let (s_new, v_new) = (a.s[i] + gap, a.v[i].min(a.v[j]));
                    let event = EventRecord {
                        t,
                        kind: "collision_guard",
                        agent_id: a.id[i],
                        ribbon: self.net.ribbon(a.ribbon[i] as usize).id.clone(),
                        s: s_new,
                        y: a.y[i],
                        attrs: format!("{{\"leader_id\":{},\"overlap_m\":{}}}", a.id[j], -gap),
                    };
                    self.agents.distance[i] += gap;
                    self.agents.s[i] = s_new;
                    self.agents.v[i] = v_new;
                    self.events.push(event);
                    self.collision_guards += 1;
                }
            }
        }
    }

    /// Distance from the start of ribbon `from` to the start of ribbon `to`
    /// along the chain (0 if they are the same or not connected).
    fn ribbon_offset(&self, from: usize, to: usize) -> f64 {
        let mut off = 0.0;
        let mut r = from;
        for _ in 0..self.net.len() {
            if r == to {
                return off;
            }
            off += self.net.ribbon(r).length();
            match self.net.ribbon(r).next {
                Some(n) => r = n,
                None => return 0.0,
            }
        }
        0.0
    }

    // --------------------------------------------------------- transitions

    fn phase_transitions(&mut self) {
        self.transferred.clear();
        let t = self.t;
        for i in 0..self.agents.len() {
            let mut moved = false;
            loop {
                let r = self.agents.ribbon[i] as usize;
                let len = self.net.ribbon(r).length();
                if self.agents.s[i] < len {
                    break;
                }
                match self.net.ribbon(r).next {
                    Some(n) => {
                        self.agents.s[i] -= len;
                        self.agents.s_prev[i] -= len;
                        self.agents.ribbon[i] = n as u32;
                        moved = true;
                    }
                    None => {
                        let a = &self.agents;
                        let (s0, s1) = (a.s_prev[i], a.s[i]);
                        let frac = if s1 > s0 { (len - s0) / (s1 - s0) } else { 1.0 };
                        let t_exit = t - self.dt + frac * self.dt;
                        let overshoot = s1 - len;
                        let rec = self.record(i, Some((t_exit, overshoot)));
                        self.records.push(rec);
                        self.agents.exited[i] = true;
                        self.exited += 1;
                        break;
                    }
                }
            }
            if moved && !self.agents.exited[i] {
                self.transferred.push(i as u32);
            }
        }
    }

    fn record(&self, i: usize, exit: Option<(f64, f64)>) -> AgentRecord {
        let a = &self.agents;
        let vtype = VehicleType::ALL[usize::from(a.vtype[i])];
        let (t_exit, distance) = match exit {
            Some((t, overshoot)) => (Some(t), a.distance[i] - overshoot),
            None => (None, a.distance[i]),
        };
        let end = t_exit.unwrap_or(self.t);
        let travel_time = end - a.t_entry[i];
        let free_flow_time = distance / a.v0[i];
        AgentRecord {
            agent_id: a.id[i],
            vehicle_type: vtype.key(),
            class: vtype.class().key(),
            entry: self.demand.entries[usize::from(a.entry[i])].id.clone(),
            t_arrival: a.t_arrival[i],
            t_entry: a.t_entry[i],
            t_exit,
            spawn_delay: a.t_entry[i] - a.t_arrival[i],
            distance,
            travel_time,
            free_flow_time,
            delay: travel_time - free_flow_time,
            stops: a.stops[i],
            exited: exit.is_some(),
            entry_y: a.entry_y[i],
            length: a.length[i],
            width: a.width[i],
            v0: a.v0[i],
            time_gap: a.time_gap[i],
            standstill_gap: a.standstill_gap[i],
            max_accel: a.max_accel[i],
            comfortable_decel: a.comfortable_decel[i],
        }
    }

    // ------------------------------------------------------------- compact

    fn phase_compact(&mut self) {
        let moved = self.agents.compact(&mut self.remap);
        let remap = &self.remap;
        let ribbons = &self.agents.ribbon;
        for (r, order) in self.orders.iter_mut().enumerate() {
            order.retain_mut(|i| {
                let ni = if moved { remap[*i as usize] } else { *i };
                if ni == REMOVED || ribbons[ni as usize] as usize != r {
                    return false;
                }
                *i = ni;
                true
            });
        }
        for &old in &self.transferred {
            let ni = if moved { self.remap[old as usize] } else { old };
            if ni != REMOVED {
                self.orders[self.agents.ribbon[ni as usize] as usize].push(ni);
            }
        }
        let (s, id) = (&self.agents.s, &self.agents.id);
        for order in &mut self.orders {
            order.sort_by(|&x, &y| {
                s[x as usize]
                    .total_cmp(&s[y as usize])
                    .then(id[x as usize].cmp(&id[y as usize]))
            });
        }
    }

    // -------------------------------------------------------- trajectories

    fn phase_trajectories(&mut self) {
        let Some(cfg) = &self.trajectories else {
            return;
        };
        if !self.step.is_multiple_of(cfg.every_steps) || self.t < cfg.from_t - 0.5 * self.dt {
            return;
        }
        let a = &self.agents;
        for i in 0..a.len() {
            if !(cfg.s_from..=cfg.s_to).contains(&a.s[i]) {
                continue;
            }
            let (x, yw, h) = self
                .net
                .ribbon(a.ribbon[i] as usize)
                .to_world(a.s[i], a.y[i]);
            let c = &mut self.traj;
            c.t.push(self.t);
            c.agent_id.push(a.id[i]);
            c.ribbon.push(a.ribbon[i]);
            c.s.push(a.s[i]);
            c.y.push(a.y[i]);
            c.x_world.push(x);
            c.y_world.push(yw);
            c.heading.push(h);
            c.v.push(a.v[i]);
            c.a.push(a.acc[i]);
        }
        if self.traj.len() >= TRAJECTORY_CHUNK_ROWS {
            self.flush_trajectories();
        }
    }

    fn flush_trajectories(&mut self) {
        if self.traj.is_empty() {
            return;
        }
        match &mut self.sink {
            Some(sink) => sink.write(std::mem::take(&mut self.traj)),
            None => self.traj.clear(),
        }
    }

    // -------------------------------------------------------------- demand

    fn phase_demand(&mut self) {
        self.demand.generate(self.t);
        for e in 0..self.demand.entries.len() {
            let mut idx = 0;
            let mut attempts = 0;
            while idx < self.demand.entries[e].pending.len()
                && attempts < self.p.insertion_attempts_per_step
            {
                attempts += 1;
                if self.demand.entries[e].pending[idx].draws.is_none() {
                    let arr = &self.demand.entries[e].pending[idx];
                    let d = Draws::sample(self.seed, arr.id, arr.vtype, &self.p);
                    self.demand.entries[e].pending[idx].draws = Some(d);
                }
                let arr = self.demand.entries[e].pending[idx].clone();
                if self.try_insert(e, &arr) {
                    self.demand.entries[e].pending.remove(idx);
                } else {
                    idx += 1;
                }
            }
        }
    }

    /// Insert `arr` at entry `e` if there is room; returns whether it entered.
    fn try_insert(&mut self, e: usize, arr: &Arrival) -> bool {
        let d = arr.draws.expect("drawn before insertion");
        let entry = &self.demand.entries[e];
        let (r, s_e) = (entry.ribbon, entry.s);
        let (y_min, y_max) = self.net.ribbon(r).y_bounds(s_e);
        let free_width = (y_max - y_min - d.width).max(0.0);
        let y = y_min + 0.5 * d.width.min(y_max - y_min) + d.lateral_fraction * free_width;
        let in_band = y - y_min < self.p.median_band_width;
        let v0_bonus = if in_band {
            d.median_band_bonus.max(0.0)
        } else {
            0.0
        };
        let v0 = d.v0 + v0_bonus;
        let class = arr.vtype.class();
        let idm = Idm {
            v0,
            time_gap: d.time_gap,
            standstill_gap: d.standstill_gap,
            max_accel: d.max_accel,
            comfortable_decel: d.comfortable_decel,
            exponent: self.p.accel_exponent,
        };
        let Some(v_ins) = self.insertion_speed(r, s_e, y, d.length, d.width, class, &idm) else {
            return false;
        };
        if v_ins < self.p.stop_speed_below {
            return false;
        }
        let new = NewAgent {
            id: arr.id,
            vtype: arr.vtype.index() as u8,
            class: class.index() as u8,
            entry: e as u16,
            ribbon: r as u32,
            s: s_e,
            y,
            v: v_ins,
            length: d.length,
            width: d.width,
            v0,
            v0_bonus,
            time_gap: d.time_gap,
            standstill_gap: d.standstill_gap,
            max_accel: d.max_accel,
            comfortable_decel: d.comfortable_decel,
            t_arrival: arr.t_arrival,
            t_entry: self.t,
        };
        let i = self.agents.push(new);
        self.agents.stop_armed[i as usize] = v_ins > self.p.stop_rearm_above;
        let (s, id) = (&self.agents.s, &self.agents.id);
        let order = &mut self.orders[r];
        let pos = order.partition_point(|&j| (s[j as usize], id[j as usize]) < (s_e, arr.id));
        order.insert(pos, i);
        self.spawned += 1;
        true
    }

    /// Speed at which a vehicle can enter at `(r, s_e, y)`, or `None` if the
    /// space is occupied. Leaders in the way cap the speed so the desired gap
    /// is respected; followers in the way must already have their desired gap.
    #[allow(clippy::too_many_arguments)]
    fn insertion_speed(
        &self,
        r: usize,
        s_e: f64,
        y: f64,
        length: f64,
        width: f64,
        class: VehicleClass,
        idm: &Idm,
    ) -> Option<f64> {
        let a = &self.agents;
        let p = &self.p;
        let order = &self.orders[r];
        let start = order.partition_point(|&j| a.s[j as usize] < s_e);
        let ci = class.index();
        let mut v = idm.v0;
        let l_max = a.length.iter().copied().fold(length, f64::max);
        // Leaders ahead (and vehicles level with the entry point).
        let mut cov = Coverage::new(y - 0.5 * width, y + 0.5 * width);
        let mut horizon = p.lookahead;
        for &j in &order[start..] {
            let j = j as usize;
            let ds = a.s[j] - s_e;
            if ds > horizon {
                break;
            }
            let gap_s = ds - 0.5 * (length + a.length[j]);
            let gy = lateral_gap(y, width, a.y[j], a.width[j]);
            let c = clearance(p.clearance_intercept, p.clearance_slope, 0.5 * (v + a.v[j]));
            if gy - c > 0.0 {
                continue;
            }
            if gap_s <= 0.0 {
                if gy < 0.0 {
                    return None;
                }
                continue;
            }
            let cj = usize::from(a.class[j]);
            let v_ok = idm.insertion_speed(
                gap_s,
                a.v[j],
                p.mult_time_gap[ci][cj],
                p.mult_standstill_gap[ci][cj],
            );
            v = v.min(v_ok);
            if horizon == p.lookahead {
                cov.add(a.y[j] - 0.5 * a.width[j] - c, a.y[j] + 0.5 * a.width[j] + c);
                if cov.full() {
                    horizon = horizon.min(ds + 0.5 * l_max);
                }
            }
        }
        // Followers behind the entry point.
        let mut cov = Coverage::new(y - 0.5 * width, y + 0.5 * width);
        let mut horizon = p.lookahead;
        for &j in order[..start].iter().rev() {
            let j = j as usize;
            let ds = s_e - a.s[j];
            if ds > horizon {
                break;
            }
            let gap_s = ds - 0.5 * (length + a.length[j]);
            let gy = lateral_gap(y, width, a.y[j], a.width[j]);
            let c = clearance(p.clearance_intercept, p.clearance_slope, 0.5 * (v + a.v[j]));
            if gy - c > 0.0 {
                continue;
            }
            if gap_s <= 0.0 {
                if gy < 0.0 {
                    return None;
                }
                continue;
            }
            let cj = usize::from(a.class[j]);
            let needed = a.standstill_gap[j] * p.mult_standstill_gap[cj][ci]
                + a.v[j] * a.time_gap[j] * p.mult_time_gap[cj][ci];
            if gap_s < needed {
                return None;
            }
            if horizon == p.lookahead {
                cov.add(a.y[j] - 0.5 * a.width[j] - c, a.y[j] + 0.5 * a.width[j] + c);
                if cov.full() {
                    horizon = horizon.min(ds + 0.5 * l_max);
                }
            }
        }
        Some(v)
    }

    // ---------------------------------------------------------------- hash

    fn phase_hash(&mut self) {
        let a = &self.agents;
        self.hash_buf.clear();
        self.hash_buf.extend_from_slice(&self.step.to_le_bytes());
        for i in 0..a.len() {
            self.hash_buf.extend_from_slice(&a.id[i].to_le_bytes());
            self.hash_buf
                .extend_from_slice(&a.s[i].to_bits().to_le_bytes());
            self.hash_buf
                .extend_from_slice(&a.y[i].to_bits().to_le_bytes());
            self.hash_buf
                .extend_from_slice(&a.v[i].to_bits().to_le_bytes());
        }
        self.hasher.update(&self.hash_buf);
    }

    // -------------------------------------------------------------- finish

    /// Finish the run: flush trajectories and build every output table.
    pub fn finish(mut self) -> Result<(RunSummary, RunOutputs), String> {
        self.flush_trajectories();
        if let Some(sink) = &mut self.sink {
            sink.finish()?;
        }
        let summary = self.summary();
        let mut agents = std::mem::take(&mut self.records);
        for i in 0..self.agents.len() {
            agents.push(self.record(i, None));
        }
        agents.sort_by_key(|r| r.agent_id);
        let detectors = self.detectors.records(self.scenario.duration_s);
        let kpis = self.kpis(&agents);
        let ribbons = self.net.ribbons.iter().map(|r| r.id.clone()).collect();
        Ok((
            summary,
            RunOutputs {
                agents,
                detectors,
                events: std::mem::take(&mut self.events),
                kpis,
                ribbons,
            },
        ))
    }

    fn kpis(&self, agents: &[AgentRecord]) -> Vec<(&'static str, f64)> {
        let warm = self.scenario.warmup_s;
        let measured_s = (self.t - warm).max(f64::MIN_POSITIVE);
        let throughput = self
            .scenario
            .kpi
            .throughput_detector
            .as_deref()
            .and_then(|id| self.detectors.measured_count(id))
            .map_or(f64::NAN, |c| {
                c as f64 * crate::demand::SECONDS_PER_HOUR / measured_s
            });
        let mut delays: Vec<f64> = agents
            .iter()
            .filter(|r| r.exited && r.t_entry >= warm)
            .map(|r| r.delay)
            .collect();
        delays.sort_by(f64::total_cmp);
        let mean = |xs: &[f64]| {
            if xs.is_empty() {
                f64::NAN
            } else {
                xs.iter().sum::<f64>() / xs.len() as f64
            }
        };
        let p95 = if delays.is_empty() {
            f64::NAN
        } else {
            delays[((delays.len() - 1) as f64 * 0.95).round() as usize]
        };
        let spawn_delays: Vec<f64> = agents
            .iter()
            .filter(|r| r.t_arrival >= warm)
            .map(|r| r.spawn_delay)
            .collect();
        let max_spawn = spawn_delays.iter().copied().fold(f64::NAN, f64::max);
        vec![
            ("throughput_veh_h", throughput),
            ("delay_mean_s", mean(&delays)),
            ("delay_p95_s", p95),
            ("trips_completed", delays.len() as f64),
            (
                "network_mean_speed_m_s",
                if self.measured_tts > 0.0 {
                    self.measured_ttd / self.measured_tts
                } else {
                    f64::NAN
                },
            ),
            ("spawned", self.spawned as f64),
            ("exited", self.exited as f64),
            ("in_network_at_end", self.agents.len() as f64),
            ("waiting_to_enter_at_end", self.demand.pending() as f64),
            ("spawn_delay_mean_s", mean(&spawn_delays)),
            ("spawn_delay_max_s", max_spawn),
            ("saturated_headways", self.demand.saturated_headways as f64),
            ("collision_guard_events", self.collision_guards as f64),
            ("agent_steps", self.agent_steps as f64),
        ]
    }
}

/// Read-only context for the parallel move phase.
struct MoveCtx<'a> {
    a: &'a AgentStore,
    orders: &'a [Vec<u32>],
    net: &'a Network,
    p: &'a CoreParams,
    cut: f64,
    /// Longest vehicle in the network: leaders are sorted by centre, so a
    /// long vehicle whose centre is further ahead can still have its rear
    /// nearer than the leader that completed the coverage.
    l_max: f64,
}

impl MoveCtx<'_> {
    /// Acceleration of agent `i` at position `k` of ribbon `r`, and its nearest
    /// physically overlapping leader (plan/03-model-design.md §2).
    #[inline]
    fn accel(&self, r: usize, k: usize, i: usize) -> (f64, u32) {
        self.accel_with(r, k, i, |_| {})
    }

    /// [`Self::accel`], reporting every agent the scan considers to `seen`
    /// (the live inspector). With a no-op `seen` it compiles to the same code.
    #[inline(always)]
    fn accel_with(
        &self,
        r: usize,
        k: usize,
        i: usize,
        mut seen: impl FnMut(Interaction),
    ) -> (f64, u32) {
        let (a, p) = (self.a, self.p);
        let ci = usize::from(a.class[i]);
        let idm = Idm {
            v0: a.v0[i],
            time_gap: a.time_gap[i],
            standstill_gap: a.standstill_gap[i],
            max_accel: a.max_accel[i],
            comfortable_decel: a.comfortable_decel[i],
            exponent: p.accel_exponent,
        };
        let (v, y, w, l, s) = (a.v[i], a.y[i], a.width[i], a.length[i], a.s[i]);
        let free = idm.free_accel(v);
        let mut acc = free;
        let mut phys = (f64::INFINITY, REMOVED);
        let mut cov = Coverage::new(y - 0.5 * w, y + 0.5 * w);
        let mut horizon = p.lookahead;
        let (mut ribbon, mut pos, mut offset) = (r, k + 1, 0.0);
        'scan: loop {
            let order = &self.orders[ribbon];
            while pos < order.len() {
                let j = order[pos] as usize;
                pos += 1;
                let ds = a.s[j] + offset - s;
                if ds > horizon {
                    break 'scan;
                }
                let gy = lateral_gap(y, w, a.y[j], a.width[j]);
                let c = clearance(p.clearance_intercept, p.clearance_slope, 0.5 * (v + a.v[j]));
                let excess = gy - c;
                if excess > self.cut {
                    continue;
                }
                let gap_s = ds - 0.5 * (l + a.length[j]);
                if gap_s <= 0.0 {
                    seen(Interaction {
                        j: j as u32,
                        ds,
                        gap_s,
                        lateral_gap: gy,
                        clearance: c,
                        weight: f64::NAN,
                        leader_accel: f64::NAN,
                    });
                    // Level with us: a side neighbour, unless the bodies overlap.
                    if gy < 0.0 {
                        acc = -p.emergency_decel;
                        if gap_s < phys.0 {
                            phys = (gap_s, j as u32);
                        }
                    }
                    continue;
                }
                if gy < 0.0 && gap_s < phys.0 {
                    phys = (gap_s, j as u32);
                }
                let cj = usize::from(a.class[j]);
                let leader = idm.accel(
                    v,
                    gap_s,
                    v - a.v[j],
                    p.mult_time_gap[ci][cj],
                    p.mult_standstill_gap[ci][cj],
                );
                let weight = leader_weight(excess, p.braking_scale);
                seen(Interaction {
                    j: j as u32,
                    ds,
                    gap_s,
                    lateral_gap: gy,
                    clearance: c,
                    weight,
                    leader_accel: leader,
                });
                acc = combine(acc, free, leader, weight);
                if excess <= 0.0 && horizon == p.lookahead {
                    cov.add(a.y[j] - 0.5 * a.width[j] - c, a.y[j] + 0.5 * a.width[j] + c);
                    if cov.full() {
                        // Anything centred beyond ds + l_max/2 has its rear ahead of every covering leader.
                        horizon = horizon.min(ds + 0.5 * self.l_max);
                    }
                }
            }
            match self.net.ribbon(ribbon).next {
                Some(n) => {
                    offset += self.net.ribbon(ribbon).length();
                    ribbon = n;
                    pos = 0;
                }
                None => break,
            }
        }
        (acc.clamp(-p.emergency_decel, idm.max_accel), phys.1)
    }
}
