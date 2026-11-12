//! The step loop (plan/02-architecture.md §4).
//!
//! Each step: **move** (parallel, read-only: accelerations, lateral speeds,
//! lateral decisions and conflicts from the current snapshot) → **apply**
//! (integrate along and across the ribbon) → **collision guard** →
//! **observe** (detectors) → **transitions** (ribbon handover, exits) →
//! **compact** (remove exits, re-sort ribbons) → **trajectories** →
//! **demand** (insert arrivals) → **hash**.
//! Only the move phase runs in parallel, and it writes per-agent results in
//! a fixed order, so outputs are bitwise identical for any thread count.

use crate::agents::{
    AgentStore, NO_AGENT, NewAgent, REMOVED, STOP_IN_PLACE, STOP_NONE, STOP_PULLING_OVER,
    STOP_STANDING,
};
use crate::demand::{Arrival, Demand, Draws, draw_one};
use crate::model::coverage::Coverage;
use crate::model::geometry::{Body, overlap, penetration};
use crate::model::lateral::{ChoiceParams, body_half_width, body_stretch, edge_gap};
use crate::model::longitudinal::Idm;
use crate::objects::{Narrowings, Objects, RoadObject};
use crate::observe::Detectors;
use crate::output::{
    AgentRecord, ConflictRecord, DetectorRecord, EventRecord, RunOutputs, TrajectoryChunk,
    TrajectorySink,
};
use crate::params::{CoreParams, Drawn};
use crate::parking::Parking;
pub use crate::scan::Interaction;
use crate::scan::{Barrier, MoveCtx, MoveOut, centre_bounds, conflict_kind_name};
use crate::stopping::Stopping;
use crate::width::WidthObservers;
use marg_net::Network;
use marg_params::spec::ObjectSpec;
use marg_params::{ResolvedSpec, Scenario, VehicleClass, VehicleType};
use rayon::prelude::*;

/// State bytes buffered before they are hashed.
const HASH_BATCH_BYTES: usize = 1 << 18;
/// Rows buffered before a trajectory chunk is handed to the sink.
const TRAJECTORY_CHUNK_ROWS: usize = 1 << 16;
/// Minimum work per rayon task in the move phase.
const MOVE_MIN_LEN: usize = 256;
/// Below this many agents the move phase runs on the calling thread: the
/// thread-pool round trip costs more than it saves. Results are identical.
const MOVE_PARALLEL_MIN: usize = 4 * MOVE_MIN_LEN;

/// Metres per kilometre, for reporting rates per kilometre.
const METRES_PER_KM: f64 = 1000.0;

/// How often a lateral step that does not fit beside a neighbour is halved
/// before it is dropped.
const LATERAL_HALVINGS: u32 = 5;

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
    #[error("{0}")]
    Object(String),
    #[error("cannot resample `{0}`: {1}")]
    Resample(String, &'static str),
}

/// Detectors with their own (short) interval, observed alongside the
/// scenario's, for the live viewer's dashboards.
struct LiveDetectors {
    det: Detectors,
    interval: f64,
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
    move_buf: Vec<MoveOut>,
    /// Per agent: limits of this step's lateral position (from the move phase).
    y_limits: Vec<(f64, f64)>,
    /// Per agent: how far the kerb of a narrowing road may move it this step.
    kerb_limits: Vec<(f64, f64)>,
    /// Per agent: how far a turn may lengthen its footprint at each end.
    turn_room: Vec<f64>,
    /// A predecessor of each ribbon, and distances between ribbon starts.
    prev: Vec<Option<usize>>,
    chain_offset: Vec<Vec<f64>>,
    barriers: Vec<Barrier>,
    objects: Objects,
    /// Stop-anywhere events (ADR-0014).
    stopping: Stopping,
    /// Effective-width observers (G§6.2).
    widths: WidthObservers,
    /// Kerbside parking: parked vehicles are objects; this runs the
    /// manoeuvres and knows where vehicles cruise for a space.
    parking: Parking,
    narrow: Narrowings,
    /// Agents the kerb stopped this step: index, and how far each was cut back.
    edge_stops: Vec<(u32, f64)>,
    /// Steps between an agent's scheduled lateral decisions.
    decision_steps: u64,
    conflicts: Vec<ConflictRecord>,
    guard_restored: Vec<bool>,
    remap: Vec<u32>,
    transferred: Vec<u32>,
    records: Vec<AgentRecord>,
    events: Vec<EventRecord>,
    collision_guards: u64,
    lateral_moves: u64,
    lateral_decisions: u64,
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
        let mut prev = vec![None; n_ribbons];
        for r in 0..n_ribbons {
            if let Some(n) = net.ribbon(r).next
                && prev[n].is_none()
            {
                prev[n] = Some(r);
            }
        }
        let mut chain_offset = vec![vec![f64::NAN; n_ribbons]; n_ribbons];
        for (from, row) in chain_offset.iter_mut().enumerate() {
            let (mut at, mut off) = (from, 0.0);
            loop {
                row[at] = off;
                off += net.ribbon(at).length();
                match net.ribbon(at).next {
                    Some(n) => at = n,
                    None => break,
                }
            }
        }
        let mut barriers = Vec::new();
        for b in &scenario.network.barriers {
            let ribbon = ribbon_of(&b.ribbon).ok_or_else(|| {
                SimError::InvalidScenario(format!(
                    "barrier `{}`: unknown ribbon `{}`",
                    b.id, b.ribbon
                ))
            })?;
            if b.closed
                .iter()
                .any(|c| c.to_s.is_nan() || c.to_s <= c.from_s)
            {
                return Err(SimError::InvalidScenario(format!(
                    "barrier `{}`: every closed period needs to_s > from_s",
                    b.id
                )));
            }
            barriers.push(Barrier {
                id: b.id.clone(),
                ribbon,
                s: b.s,
                closed: b.closed.iter().map(|c| (c.from_s, c.to_s)).collect(),
                is_closed: false,
            });
        }
        let mut objects = Objects::new(n_ribbons);
        for o in &scenario.network.objects {
            objects
                .add(o, &net, None)
                .map_err(SimError::InvalidScenario)?;
        }
        let parking = Parking::new(&scenario.network.parking, &net, &p, seed, &mut objects)
            .map_err(SimError::InvalidScenario)?;
        let stopping = Stopping::new(&scenario.network.attractors, &net, &p, seed)
            .map_err(SimError::InvalidScenario)?;
        let widths = WidthObservers::new(&scenario.outputs.effective_width, &net, &p)
            .map_err(SimError::InvalidScenario)?;
        let narrow = Narrowings::new(&net);
        let decision_steps = decision_steps(&p, scenario.dt_s);
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
            y_limits: Vec::new(),
            kerb_limits: Vec::new(),
            turn_room: Vec::new(),
            prev,
            chain_offset,
            barriers,
            objects,
            parking,
            stopping,
            widths,
            narrow,
            edge_stops: Vec::new(),
            decision_steps,
            conflicts: Vec::new(),
            guard_restored: Vec::new(),
            remap: Vec::new(),
            transferred: Vec::new(),
            records: Vec::new(),
            events: Vec::new(),
            collision_guards: 0,
            lateral_moves: 0,
            lateral_decisions: 0,
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
        self.collision_guard(t0 + self.dt);
        self.detectors
            .observe(&self.agents, &self.orders, t0, self.dt, self.max_step);
        if let Some(live) = &mut self.live {
            live.det
                .observe(&self.agents, &self.orders, t0, self.dt, self.max_step);
        }
        if !self.widths.is_empty() && t0 >= self.scenario.warmup_s {
            self.widths.observe(
                &self.agents,
                &self.orders,
                &self.objects,
                &self.net,
                &self.prev,
                &self.p,
                self.dt,
            );
        }
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
            output_hash: {
                let mut h = self.hasher.clone();
                h.update(&self.hash_buf);
                h.finalize().to_hex().to_string()
            },
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
        self.decision_steps = decision_steps(&self.p, self.dt);
        for i in 0..self.agents.len() {
            self.agents.clearance_scale[i] = self.p.clearance_scale(self.agents.kappa[i]);
        }
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
                    Drawn::V0 => {
                        a.v0[i] += x - a.v0_base[i];
                        a.v0_base[i] = x;
                    }
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

    /// Every agent the scan of agent `i` considers this step (leaders and
    /// side neighbours), with the terms of its acceleration.
    pub fn interactions(&self, i: usize) -> Vec<Interaction> {
        let r = self.agents.ribbon[i] as usize;
        let Some(k) = self.orders[r].iter().position(|&x| x as usize == i) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        self.move_ctx().agent_with(r, k, i, |x| out.push(x));
        out
    }

    /// The lateral choice agent `i` would make now (ADR-0010 §3), whether or
    /// not a decision is due: its target and the utility terms.
    pub fn lateral_choice(&self, i: usize) -> Option<crate::model::lateral::Choice> {
        let r = self.agents.ribbon[i] as usize;
        let k = self.orders[r].iter().position(|&x| x as usize == i)?;
        let ctx = self.move_ctx();
        let a = &self.agents;
        let idm = Idm {
            v0: a.v0[i],
            time_gap: a.time_gap[i],
            standstill_gap: a.standstill_gap[i],
            max_accel: a.max_accel[i],
            comfortable_decel: a.comfortable_decel[i],
            exponent: self.p.accel_exponent,
        };
        Some(ctx.decide(r, k, i, &idm, f64::INFINITY))
    }

    /// Every road object, on the road now or not.
    pub fn objects(&self) -> &[RoadObject] {
        &self.objects.all
    }

    /// Kerbside parking stretches and their state.
    pub fn parking(&self) -> &Parking {
        &self.parking
    }

    /// Put a road object on the road from the next step on (the live
    /// viewer's `add_object`). Vehicles under it at that moment drive on.
    pub fn add_object(&mut self, spec: &ObjectSpec) -> Result<(), SimError> {
        self.objects
            .add(spec, &self.net, Some(self.t))
            .map_err(SimError::Object)
    }

    /// Take a road object off the road from the next step on.
    pub fn remove_object(&mut self, id: &str) -> Result<(), SimError> {
        self.objects.remove(id, self.t).map_err(SimError::Object)
    }

    /// Every conflict episode closed so far.
    pub fn conflicts(&self) -> &[ConflictRecord] {
        &self.conflicts
    }

    fn move_ctx(&self) -> MoveCtx<'_> {
        let p = &self.p;
        let cut = p.interaction_cutoff_distance();
        let a = &self.agents;
        let fold = |xs: &[f64]| xs.iter().copied().fold(0.0, f64::max);
        let vy_max = a.vy.iter().fold(0.0, |m: f64, x| m.max(x.abs()));
        let flat = |m: &[[f64; crate::params::N_CLASSES]; crate::params::N_CLASSES]| {
            m.iter().flatten().copied().fold(0.0, f64::max)
        };
        let c_min_cap = fold(&a.clearance_scale)
            * (flat(&p.clearance_intercept) + flat(&p.clearance_slope) * fold(&a.v)
                - p.clearance_min_offset)
                .max(p.clearance_min_floor);
        MoveCtx {
            a: &self.agents,
            orders: &self.orders,
            net: &self.net,
            p,
            prev: &self.prev,
            chain_offset: &self.chain_offset,
            barriers: &self.barriers,
            objects: &self.objects,
            narrow: &self.narrow,
            cut,
            steer_cut: p.steering_cutoff_distance(),
            l_max: self.agents.length.iter().copied().fold(0.0, f64::max),
            step: self.step,
            decision_steps: self.decision_steps,
            dt: self.dt,
            choice: ChoiceParams {
                braking_scale: p.braking_scale,
                cutoff: cut,
                politeness: p.politeness,
                safe_decel: p.safe_decel,
                comfort_weight: p.comfort_weight,
                move_cost: p.move_cost,
                edge_clearance: p.edge_clearance,
                switch_threshold: p.switch_threshold,
                step: p.candidate_step,
                range: p.candidate_range,
            },
            vy_max,
            c_min_cap,
            v_max: fold(&a.v),
        }
    }

    /// Headline KPIs if the run ended now (the same computation as `finish`).
    pub fn kpis_now(&self) -> Vec<(&'static str, f64)> {
        let mut agents = self.records.clone();
        for i in 0..self.agents.len() {
            agents.push(self.record(i, None));
        }
        agents.sort_by_key(|r| r.agent_id);
        // Conflict episodes still open count as they stand.
        let mut conflicts = self.conflicts.clone();
        for i in 0..self.agents.len() {
            if self.agents.conflict_other[i] != NO_AGENT {
                conflicts.push(ConflictRecord {
                    t_start: self.agents.conflict_t0[i],
                    kind: conflict_kind_name(self.agents.conflict_kind[i]),
                    ..ConflictRecord::default()
                });
            }
        }
        self.kpis_with(&agents, &conflicts)
    }

    // ---------------------------------------------------------------- move

    fn phase_move(&mut self) {
        self.work.clear();
        for (r, order) in self.orders.iter().enumerate() {
            self.work
                .extend((0..order.len() as u32).map(|k| (r as u32, k)));
        }
        let t = self.t;
        for b in &mut self.barriers {
            b.is_closed = b.closed.iter().any(|&(from, to)| t >= from && t < to);
        }
        let (events, net) = (&mut self.events, &self.net);
        self.parking.step(
            t,
            &self.agents,
            &self.orders,
            net,
            &self.prev,
            &self.p,
            &mut self.objects,
            |m| {
                events.push(EventRecord {
                    t,
                    kind: "parking_manoeuvre",
                    agent_id: NO_AGENT,
                    ribbon: net.ribbon(m.ribbon).id.clone(),
                    s: m.s,
                    y: m.y,
                    attrs: format!(
                        "{{\"zone\":{},\"object\":{},\"action\":\"{}\",\"duration_s\":{},\"waited_s\":{}}}",
                        json_string(&m.zone),
                        json_string(&m.object),
                        if m.park { "park" } else { "unpark" },
                        m.duration,
                        m.waited
                    ),
                });
            },
        );
        self.objects.update(t, |o| {
            events.push(EventRecord {
                t,
                kind: if o.active {
                    "object_added"
                } else {
                    "object_removed"
                },
                agent_id: NO_AGENT,
                ribbon: net.ribbon(o.ribbon).id.clone(),
                s: o.s,
                y: o.y,
                attrs: format!(
                    "{{\"object\":{},\"kind\":\"{}\",\"length\":{},\"width\":{}}}",
                    json_string(&o.id),
                    o.kind.key(),
                    o.length,
                    o.width
                ),
            });
        });
        let mut buf = std::mem::take(&mut self.move_buf);
        {
            let ctx = self.move_ctx();
            let work = &self.work;
            let one = |&(r, k): &(u32, u32)| {
                let (r, k) = (r as usize, k as usize);
                ctx.agent(r, k, ctx.orders[r][k] as usize)
            };
            if work.len() < self.parallel_min || self.pool.current_num_threads() == 1 {
                buf.clear();
                buf.extend(work.iter().map(one));
            } else {
                self.pool.install(|| {
                    work.par_iter()
                        .with_min_len(MOVE_MIN_LEN)
                        .map(one)
                        .collect_into_vec(&mut buf)
                });
            }
        }
        self.kerb_limits.clear();
        self.kerb_limits
            .resize(self.agents.len(), (f64::NEG_INFINITY, f64::INFINITY));
        self.turn_room.clear();
        self.turn_room.resize(self.agents.len(), f64::INFINITY);
        self.y_limits.clear();
        self.y_limits
            .resize(self.agents.len(), (f64::NEG_INFINITY, f64::INFINITY));
        for (n, out) in buf.iter().enumerate() {
            let (r, k) = self.work[n];
            let i = self.orders[r as usize][k as usize] as usize;
            self.agents.acc[i] = out.acc;
            self.agents.vy[i] = out.vy;
            self.y_limits[i] = (out.y_lo, out.y_hi);
            self.kerb_limits[i] = (out.kerb_lo, out.kerb_hi);
            self.turn_room[i] = out.turn_room;
            if let Some(c) = out.choice {
                self.lateral_decisions += 1;
                self.agents.decision_leader[i] = out.leader;
                if c.switched {
                    self.agents.lateral_moves[i] += 1;
                    self.lateral_moves += 1;
                    if self.p.log_lateral_decisions {
                        let a = &self.agents;
                        self.events.push(EventRecord {
                            t,
                            kind: "lateral_move",
                            agent_id: a.id[i],
                            ribbon: self.net.ribbon(a.ribbon[i] as usize).id.clone(),
                            s: a.s[i],
                            y: a.y[i],
                            attrs: format!(
                                "{{\"target\":{},\"utility\":{},\"own\":{},\"polite\":{},\"comfort\":{},\"cost\":{}}}",
                                c.target, c.utility, c.own, c.polite, c.comfort, c.cost
                            ),
                        });
                    }
                }
                self.agents.y_target[i] = c.target;
            }
            self.track_conflict(i, out, t);
        }
        self.move_buf = buf;
    }

    /// Extend, close or open agent `i`'s conflict episode (ADR-0010 §6). An
    /// agent has one open episode at a time: with the neighbour ahead of it
    /// in the ribbon order whose time to contact is shortest.
    fn track_conflict(&mut self, i: usize, out: &MoveOut, t: f64) {
        let other = out
            .conflict
            .map_or(NO_AGENT, |c| self.agents.id[c.other as usize]);
        if self.agents.conflict_other[i] != other {
            self.close_conflict(i, t);
        }
        let Some(c) = out.conflict else { return };
        let a = &mut self.agents;
        if a.conflict_other[i] == NO_AGENT {
            a.conflict_other[i] = other;
            a.conflict_t0[i] = t;
            a.conflict_min_ttc[i] = f64::INFINITY;
            a.conflict_max_drac[i] = f64::NAN;
        }
        if c.ttc < a.conflict_min_ttc[i] {
            a.conflict_min_ttc[i] = c.ttc;
            a.conflict_kind[i] = c.kind;
            a.conflict_s[i] = a.s[i];
            a.conflict_y[i] = a.y[i];
        }
        // f64::max ignores NaN, so the maximum is over the steps where DRAC is defined.
        a.conflict_max_drac[i] = a.conflict_max_drac[i].max(c.drac);
    }

    /// Close agent `i`'s open conflict episode, if any, at time `t`.
    fn close_conflict(&mut self, i: usize, t: f64) {
        let a = &mut self.agents;
        if a.conflict_other[i] == NO_AGENT {
            return;
        }
        let other_id = a.conflict_other[i];
        a.conflict_other[i] = NO_AGENT;
        let a = &self.agents;
        let class_of = |id: u64| VehicleClass::ALL[((id >> 40) & 0xff) as usize].key();
        self.conflicts.push(ConflictRecord {
            t_start: a.conflict_t0[i],
            t_end: t,
            agent_id: a.id[i],
            other_id,
            class: VehicleClass::ALL[usize::from(a.class[i])].key(),
            other_class: class_of(other_id),
            kind: conflict_kind_name(a.conflict_kind[i]),
            min_ttc: a.conflict_min_ttc[i],
            max_drac: a.conflict_max_drac[i],
            ribbon: self.net.ribbon(a.ribbon[i] as usize).id.clone(),
            s: a.conflict_s[i],
            y: a.conflict_y[i],
        });
    }

    // --------------------------------------------------------------- apply

    fn phase_apply(&mut self) {
        let dt = self.dt;
        let p = &self.p;
        let a = &mut self.agents;
        let measured = self.t >= self.scenario.warmup_s;
        let mut max_step: f64 = 0.0;
        let mut ttd = 0.0;
        self.edge_stops.clear();
        let cruising = self.parking.any_cruising();
        let stopping = self.stopping.enabled();
        for i in 0..a.len() {
            let (v, acc) = (a.v[i], a.acc[i]);
            let v1 = v + acc * dt;
            let mut ds = if v1 >= 0.0 {
                0.5 * (v + v1) * dt
            } else {
                -v * v / (2.0 * acc)
            };
            let (s0, distance0, free_flow_time0) = (a.s[i], a.distance[i], a.free_flow_time[i]);
            a.s_prev[i] = s0;
            a.s[i] += ds;
            a.v[i] = v1.max(0.0);
            a.distance[i] += ds;
            a.free_flow_time[i] += ds / a.v0[i];
            // Across the ribbon: the heading limit (or the creep speed when
            // nearly stopped), the side neighbours' limits, the carriageway.
            let creep = if a.v[i] < p.stop_speed_below {
                p.creep_speed[usize::from(a.class[i])] * dt
            } else {
                0.0
            };
            let reach = (ds * p.max_heading_tan).max(creep);
            let dy = (a.vy[i] * dt).clamp(-reach, reach);
            let r = a.ribbon[i] as usize;
            let (mut edge_lo, mut edge_hi) = chain_bounds(&self.net, r, a.s[i]);
            let (mut road_lo, mut road_hi) = centre_bounds(edge_lo, edge_hi, 0.5 * a.width[i]);
            let (y0, (lim_lo, lim_hi)) = (a.y[i], self.y_limits[i]);
            // A body widens as it turns (ADR-0010 §4): its corners swing out
            // on both sides, so a move away from one neighbour can still
            // reach the other. Halve the move until the body, at the heading
            // the move gives it, stays within the room on each side; not
            // moving always does (straightening never widens a body).
            let (width, length, half_now) = (a.width[i], a.length[i], a.half_width[i]);
            let turn_room = self.turn_room[i];
            let stretch_now = body_stretch(width, length, a.heading_tan[i]);
            let fits = |dy: f64| -> bool {
                if dy == 0.0 {
                    return true;
                }
                let along = ds.max(dy.abs() / p.max_heading_tan);
                let swing = body_half_width(width, length, dy / along) - half_now;
                // A body that creeps sideways faster than its forward
                // movement allows turns on the spot, and turned it is also
                // longer along the road: it must not reach a body just in
                // front of or behind it (ADR-0012 part 2).
                let stretch = body_stretch(width, length, dy / along);
                dy + swing <= lim_hi - y0
                    && swing - dy <= y0 - lim_lo
                    && (dy.abs() <= ds * p.max_heading_tan
                        || stretch <= turn_room
                        || stretch <= stretch_now)
            };
            let mut dy = dy;
            if dy != 0.0
                && (dy.abs() > ds * p.max_heading_tan
                    || lim_hi < f64::INFINITY
                    || lim_lo > f64::NEG_INFINITY)
            {
                let mut tries = 0;
                while !fits(dy) {
                    tries += 1;
                    dy = if tries < LATERAL_HALVINGS {
                        0.5 * dy
                    } else {
                        0.0
                    };
                }
            }
            let steered = y0 + dy;
            let mut y1 = steered.clamp(road_lo, road_hi);
            // A narrowing carriageway moves the vehicle sideways. If that
            // takes it beyond the room its neighbours leave, it stops short
            // instead (as far along as the room allows), and the stop is
            // logged like any other step the collision guard cuts back.
            let (kerb_lo, kerb_hi) = self.kerb_limits[i];
            if (y1 > steered && y1 > kerb_hi) || (y1 < steered && y1 < kerb_lo) {
                let full = ds;
                let mut fraction = 1.0;
                for tries in 1..=LATERAL_HALVINGS {
                    fraction = if tries < LATERAL_HALVINGS {
                        0.5 * fraction
                    } else {
                        0.0
                    };
                    ds = fraction * full;
                    (edge_lo, edge_hi) = chain_bounds(&self.net, r, s0 + ds);
                    (road_lo, road_hi) = centre_bounds(edge_lo, edge_hi, 0.5 * a.width[i]);
                    y1 = y0.clamp(road_lo, road_hi);
                    if y1 >= kerb_lo && y1 <= kerb_hi {
                        break;
                    }
                }
                if fraction == 0.0 {
                    y1 = y0;
                }
                a.s[i] = s0 + ds;
                a.v[i] = a.v[i].min(ds / dt);
                a.distance[i] = distance0 + ds;
                a.free_flow_time[i] = free_flow_time0 + ds / a.v0[i];
                self.edge_stops.push((i as u32, full - ds));
            }
            max_step = max_step.max(ds);
            ttd += ds;
            let dy = y1 - y0;
            a.y_prev[i] = y0;
            a.y[i] = y1;
            a.vy[i] = dy / dt;
            a.lateral_distance[i] += dy.abs();
            a.heading_tan_prev[i] = a.heading_tan[i];
            // Creeping sideways at a standstill is the limit heading. A body
            // the kerb moved over was not steered there: it stays straight
            // (the carriageway bounds its unturned width).
            let pushed = y1 < y0.min(steered) || y1 > y0.max(steered);
            let along = ds.max(dy.abs() / p.max_heading_tan);
            a.heading_tan[i] = if dy == 0.0 || pushed {
                0.0
            } else {
                (dy / along).clamp(-p.max_heading_tan, p.max_heading_tan)
            };
            a.half_width[i] = body_half_width(a.width[i], a.length[i], a.heading_tan[i]);
            // The median band's speed bonus follows the lateral position.
            let in_band = y1 - edge_lo < p.median_band_width;
            a.v0[i] = a.v0_base[i] + if in_band { a.v0_bonus[i] } else { 0.0 };
            // A vehicle looking for a parking space drives slowly along the
            // stretch (a speed cap, G§6.2).
            if cruising
                && let Some(cap) =
                    self.parking
                        .cruising_cap(r, a.s[i], usize::from(a.class[i]), a.cruising[i])
            {
                a.v0[i] = a.v0[i].min(cap);
            }
            if stopping && a.stop_exposure[i] < f64::INFINITY {
                let class = usize::from(a.class[i]);
                match a.stop_state[i] {
                    STOP_NONE => {
                        let multiplier = self.stopping.multiplier(r, a.s[i]);
                        a.stop_exposure[i] -= ds * multiplier;
                        if a.stop_exposure[i] <= 0.0 {
                            let d = self.stopping.decide(a.id[i], class, a.stop_count[i]);
                            let idm = Idm {
                                v0: a.v0[i],
                                time_gap: a.time_gap[i],
                                standstill_gap: a.standstill_gap[i],
                                max_accel: a.max_accel[i],
                                comfortable_decel: a.comfortable_decel[i],
                                exponent: p.accel_exponent,
                            };
                            // Where its following model would begin to
                            // respond to something standing in its way.
                            let ahead = idm.desired_gap(a.v[i], a.v[i], 1.0, 1.0);
                            a.stop_state[i] = if d.pull_over {
                                STOP_PULLING_OVER
                            } else {
                                STOP_IN_PLACE
                            };
                            a.stop_point[i] = a.distance[i] + ahead;
                            a.stop_until[i] = d.dwell;
                            a.y_target[i] = y1;
                            self.events.push(EventRecord {
                                t: self.t + dt,
                                kind: "stop_anywhere",
                                agent_id: a.id[i],
                                ribbon: self.net.ribbon(r).id.clone(),
                                s: a.s[i],
                                y: y1,
                                attrs: format!(
                                    "{{\"rate_per_km\":{},\"multiplier\":{multiplier},\"pull_over\":{},\"pull_over_probability\":{},\"dwell_s\":{},\"stop_in_m\":{ahead}}}",
                                    self.stopping.rate(class) * METRES_PER_KM,
                                    d.pull_over,
                                    d.pull_over_probability,
                                    d.dwell
                                ),
                            });
                        }
                    }
                    STOP_STANDING => {
                        if self.t + dt >= a.stop_until[i] {
                            a.stop_state[i] = STOP_NONE;
                            a.stop_count[i] += 1;
                            a.stop_exposure[i] =
                                self.stopping.exposure(a.id[i], class, a.stop_count[i]);
                            a.decision_leader[i] = crate::agents::UNDECIDED;
                        }
                    }
                    _ => {
                        if a.v[i] < p.stop_speed_below {
                            let pulled_over = a.stop_state[i] == STOP_PULLING_OVER;
                            a.stop_state[i] = STOP_STANDING;
                            a.stop_until[i] += self.t + dt;
                            a.y_target[i] = y1;
                            self.events.push(EventRecord {
                                t: self.t + dt,
                                kind: "stop_anywhere_standing",
                                agent_id: a.id[i],
                                ribbon: self.net.ribbon(r).id.clone(),
                                s: a.s[i],
                                y: y1,
                                attrs: format!(
                                    "{{\"pull_over\":{pulled_over},\"kerb_gap_m\":{},\"until\":{}}}",
                                    edge_hi - y1 - 0.5 * a.width[i],
                                    a.stop_until[i]
                                ),
                            });
                        }
                    }
                }
            }
            if a.v[i] < p.stop_speed_below {
                if a.stop_armed[i] {
                    a.stops[i] += 1;
                    a.stop_armed[i] = false;
                }
            } else if a.v[i] > p.stop_rearm_above {
                a.stop_armed[i] = true;
            }
        }
        self.max_step = max_step;
        self.agent_steps += a.len() as u64;
        if measured {
            self.measured_tts += a.len() as f64 * dt;
            self.measured_ttd += ttd;
        }
        let t = self.t + dt;
        for &(i, cut) in &self.edge_stops {
            let i = i as usize;
            self.events.push(EventRecord {
                t,
                kind: "collision_guard",
                agent_id: a.id[i],
                ribbon: self.net.ribbon(a.ribbon[i] as usize).id.clone(),
                s: a.s[i],
                y: a.y[i],
                attrs: format!("{{\"other\":\"edge\",\"cut_back_s\":{cut}}}"),
            });
        }
        self.collision_guards += self.edge_stops.len() as u64;
    }

    /// The body of agent `i`, `offset` m further along the chain.
    fn body(&self, i: usize, offset: f64) -> Body {
        let a = &self.agents;
        Body::from_tan(
            a.s[i] + offset,
            a.y[i],
            a.length[i],
            a.width[i],
            a.heading_tan[i],
        )
    }

    /// Resolve any overlap of oriented bodies that integration produced
    /// (ADR-0010 §5; should not happen: counted and logged). The rear vehicle
    /// of an overlapping pair is put back where it was before the step; if it
    /// already has been, the other one is. Every vehicle is put back at most
    /// once, and the state before the step had no overlaps, so none remain.
    fn collision_guard(&mut self, t: f64) {
        let l_max = self.agents.length.iter().copied().fold(0.0, f64::max);
        let w_max = self.agents.width.iter().copied().fold(0.0, f64::max);
        // Orders are sorted by position before the step; bodies turned by the
        // heading limit reach at most this much further along the ribbon.
        let slack = self.max_step + w_max * self.p.max_heading_tan;
        let mut any = false;
        // Road objects first: a vehicle that drove into one this step goes
        // back to where it was (one that was already on it, because the
        // object appeared there, drives on).
        if self.objects.any_active() {
            for i in 0..self.agents.len() {
                let a = &self.agents;
                let (class, reach) = (usize::from(a.class[i]), 0.5 * a.length[i] + slack);
                let now = self.body(i, 0.0);
                let before = Body::from_tan(
                    a.s_prev[i],
                    a.y_prev[i],
                    a.length[i],
                    a.width[i],
                    a.heading_tan_prev[i],
                );
                let mut hit: Option<(usize, f64)> = None;
                self.objects.near(
                    &self.net,
                    &self.prev,
                    a.ribbon[i] as usize,
                    a.s[i],
                    reach,
                    reach,
                    |o, ds| {
                        if !o.blocks[class] || hit.is_some() {
                            return;
                        }
                        // Both of the vehicle's bodies are in the frame of its ribbon.
                        let body = Body::from_tan(now.s + ds, o.y, o.length, o.width, 0.0);
                        if overlap(&now, &body) && !overlap(&before, &body) {
                            hit = Some((o.serial as usize, penetration(&now, &body)));
                        }
                    },
                );
                if let Some((serial, depth)) = hit {
                    if !any {
                        any = true;
                        self.guard_restored.clear();
                        self.guard_restored.resize(self.agents.len(), false);
                    }
                    self.guard_restore(i, Against::Object(serial), depth, t);
                }
            }
        }
        loop {
            let mut changed = false;
            for r in 0..self.orders.len() {
                for k in (0..self.orders[r].len()).rev() {
                    let i = self.orders[r][k] as usize;
                    let reach = 0.5 * (self.agents.length[i] + l_max) + slack;
                    let (mut ribbon, mut pos, mut offset) = (r, k + 1, 0.0);
                    'scan: loop {
                        while pos < self.orders[ribbon].len() {
                            let j = self.orders[ribbon][pos] as usize;
                            pos += 1;
                            let a = &self.agents;
                            if a.s_prev[j] + offset - a.s_prev[i] > reach {
                                break 'scan;
                            }
                            let ds = (a.s[j] + offset - a.s[i]).abs();
                            if ds > 0.5 * (a.length[i] + a.length[j]) + slack
                                || edge_gap(a.y[i], a.half_width[i], a.y[j], a.half_width[j]) > 0.0
                            {
                                continue;
                            }
                            let (bi, bj) = (self.body(i, 0.0), self.body(j, offset));
                            if !overlap(&bi, &bj) {
                                continue;
                            }
                            if !any {
                                any = true;
                                self.guard_restored.clear();
                                self.guard_restored.resize(self.agents.len(), false);
                            }
                            let depth = penetration(&bi, &bj);
                            let (back, other) = if !self.guard_restored[i] {
                                (i, j)
                            } else if !self.guard_restored[j] {
                                (j, i)
                            } else {
                                continue;
                            };
                            self.guard_restore(back, Against::Agent(other), depth, t);
                            changed = true;
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
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Put agent `i` back where it was before this step, because it
    /// overlapped `other` by `depth` m, and log it.
    fn guard_restore(&mut self, i: usize, other: Against, depth: f64, t: f64) {
        let a = &mut self.agents;
        let (ds, dy) = (a.s[i] - a.s_prev[i], a.y[i] - a.y_prev[i]);
        a.s[i] = a.s_prev[i];
        a.y[i] = a.y_prev[i];
        a.heading_tan[i] = a.heading_tan_prev[i];
        a.half_width[i] = body_half_width(a.width[i], a.length[i], a.heading_tan[i]);
        a.distance[i] -= ds;
        a.free_flow_time[i] -= ds / a.v0[i];
        a.lateral_distance[i] -= dy.abs();
        a.v[i] = match other {
            Against::Agent(j) => a.v[i].min(a.v[j]),
            Against::Object(_) => 0.0,
        };
        a.vy[i] = 0.0;
        if self.t >= self.scenario.warmup_s {
            self.measured_ttd -= ds;
        }
        self.guard_restored[i] = true;
        let a = &self.agents;
        self.events.push(EventRecord {
            t,
            kind: "collision_guard",
            agent_id: a.id[i],
            ribbon: self.net.ribbon(a.ribbon[i] as usize).id.clone(),
            s: a.s[i],
            y: a.y[i],
            attrs: format!(
                "{{{},\"overlap_m\":{},\"cut_back_s\":{},\"cut_back_y\":{}}}",
                match other {
                    Against::Agent(j) => format!("\"other_id\":{}", a.id[j]),
                    Against::Object(serial) => format!(
                        "\"other\":\"object\",\"object\":{}",
                        json_string(&self.objects.all[serial].id)
                    ),
                },
                depth,
                ds,
                dy
            ),
        });
        self.collision_guards += 1;
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
                        self.agents.conflict_s[i] -= len;
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
                        self.close_conflict(i, t);
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
        // The overshoot beyond the exit was driven at the desired speed in force.
        let free_flow_time = a.free_flow_time[i] - (a.distance[i] - distance) / a.v0[i];
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
            v0: a.v0_base[i],
            time_gap: a.time_gap[i],
            standstill_gap: a.standstill_gap[i],
            max_accel: a.max_accel[i],
            comfortable_decel: a.comfortable_decel[i],
            kappa: a.kappa[i],
            lateral_distance: a.lateral_distance[i],
            lateral_moves: a.lateral_moves[i],
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
            sort_nearly_sorted(order, |x, y| {
                s[x as usize]
                    .total_cmp(&s[y as usize])
                    .then(id[x as usize].cmp(&id[y as usize]))
                    .is_lt()
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
            c.heading.push(h + a.heading(i));
            c.v.push(a.v[i]);
            c.v_lat.push(a.vy[i]);
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
        let v0_bonus = d.median_band_bonus.max(0.0);
        let v0 = d.v0 + if in_band { v0_bonus } else { 0.0 };
        let class = arr.vtype.class();
        let clearance_scale = self.p.clearance_scale(d.kappa);
        let idm = Idm {
            v0,
            time_gap: d.time_gap,
            standstill_gap: d.standstill_gap,
            max_accel: d.max_accel,
            comfortable_decel: d.comfortable_decel,
            exponent: self.p.accel_exponent,
        };
        let Some(v_ins) =
            self.insertion_speed(r, s_e, y, d.length, d.width, class, clearance_scale, &idm)
        else {
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
            v0_base: d.v0,
            v0_bonus,
            time_gap: d.time_gap,
            standstill_gap: d.standstill_gap,
            max_accel: d.max_accel,
            comfortable_decel: d.comfortable_decel,
            kappa: d.kappa,
            clearance_scale,
            cruising: d.cruising,
            t_arrival: arr.t_arrival,
            t_entry: self.t,
        };
        let i = self.agents.push(new);
        if self.stopping.enabled() {
            self.agents.stop_exposure[i as usize] =
                self.stopping.exposure(arr.id, class.index(), 0);
        }
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
        clearance_scale: f64,
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
            let gy = swept_gap(y, 0.5 * width, a.y[j], a.y_target[j], a.half_width[j]);
            let cj = usize::from(a.class[j]);
            let c = clearance_scale * p.pair_clearance(ci, cj, 0.5 * (v + a.v[j])).1;
            if gy - c > 0.0 {
                continue;
            }
            if gap_s <= 0.0 {
                // Level with the entry point and inside the minimum clearance.
                return None;
            }
            let v_ok = idm.insertion_speed(
                gap_s,
                a.v[j],
                p.mult_time_gap[ci][cj],
                p.mult_standstill_gap[ci][cj],
            );
            v = v.min(v_ok);
            if horizon == p.lookahead {
                cov.add(a.y[j] - a.half_width[j] - c, a.y[j] + a.half_width[j] + c);
                if cov.full() {
                    horizon = horizon.min(ds + 0.5 * l_max);
                }
            }
        }
        // Road objects at or ahead of the entry point.
        let mut blocked = false;
        self.objects.near(
            &self.net,
            &self.prev,
            r,
            s_e,
            0.5 * length,
            p.lookahead,
            |o, ds| {
                let gap_s = ds.abs() - 0.5 * (length + o.length);
                if !o.blocks[ci] || (gap_s > 0.0 && ds < 0.0) {
                    return;
                }
                let gy = edge_gap(y, 0.5 * width, o.y, 0.5 * o.width);
                let c = clearance_scale * p.object_clearance(ci, o.clearance_class, 0.5 * v).1;
                if gy - c > 0.0 {
                    return;
                }
                if gap_s <= 0.0 {
                    blocked = true;
                    return;
                }
                let (mt, ms) = p.object_multipliers(ci, o.clearance_class);
                v = v.min(idm.insertion_speed(gap_s, 0.0, mt, ms));
            },
        );
        if blocked {
            return None;
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
            let gy = swept_gap(y, 0.5 * width, a.y[j], a.y_target[j], a.half_width[j]);
            let cj = usize::from(a.class[j]);
            let c = a.clearance_scale[j] * p.pair_clearance(ci, cj, 0.5 * (v + a.v[j])).1;
            if gy - c > 0.0 {
                continue;
            }
            if gap_s <= 0.0 {
                return None;
            }
            let needed = a.standstill_gap[j] * p.mult_standstill_gap[cj][ci]
                + a.v[j] * a.time_gap[j] * p.mult_time_gap[cj][ci];
            if gap_s < needed {
                return None;
            }
            if horizon == p.lookahead {
                cov.add(a.y[j] - a.half_width[j] - c, a.y[j] + a.half_width[j] + c);
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
        // blake3 is fastest on large inputs, and the digest does not depend
        // on how its input is split, so several steps go in at once.
        if self.hash_buf.len() >= HASH_BATCH_BYTES {
            self.hasher.update(&self.hash_buf);
            self.hash_buf.clear();
        }
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
        for i in 0..self.agents.len() {
            self.close_conflict(i, self.t);
        }
        let mut conflicts = std::mem::take(&mut self.conflicts);
        conflicts.sort_by(|x, y| {
            x.t_start
                .total_cmp(&y.t_start)
                .then(x.agent_id.cmp(&y.agent_id))
                .then(x.other_id.cmp(&y.other_id))
        });
        let kpis = self.kpis_with(&agents, &conflicts);
        let ribbons = self.net.ribbons.iter().map(|r| r.id.clone()).collect();
        Ok((
            summary,
            RunOutputs {
                agents,
                detectors,
                events: std::mem::take(&mut self.events),
                conflicts,
                effective_width: self.widths.records(&self.net),
                kpis,
                ribbons,
            },
        ))
    }

    fn kpis_with(
        &self,
        agents: &[AgentRecord],
        conflicts: &[ConflictRecord],
    ) -> Vec<(&'static str, f64)> {
        let warm = self.scenario.warmup_s;
        let count = |kind: &str| {
            conflicts
                .iter()
                .filter(|c| c.t_start >= warm && (kind.is_empty() || c.kind == kind))
                .count() as f64
        };
        let lateral_moves: u64 = agents
            .iter()
            .filter(|r| r.t_entry >= warm)
            .map(|r| u64::from(r.lateral_moves))
            .sum();
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
            ("conflicts", count("")),
            ("conflicts_rear_end", count("rear_end")),
            ("conflicts_sideswipe", count("sideswipe")),
            ("lateral_moves", lateral_moves as f64),
            ("lateral_decisions", self.lateral_decisions as f64),
        ]
    }
}

/// What the collision guard put a vehicle back for.
#[derive(Debug, Clone, Copy)]
enum Against {
    /// Another vehicle (store index).
    Agent(usize),
    /// A road object (serial number).
    Object(usize),
}

/// `text` as a JSON string literal.
fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Bounds of the carriageway at station `s` of ribbon `r`, following the
/// chain when `s` is beyond the ribbon's end (a vehicle changes ribbon at
/// the end of the step that takes it across).
#[inline]
fn chain_bounds(net: &Network, r: usize, s: f64) -> (f64, f64) {
    let (mut r, mut s) = (r, s);
    loop {
        let ribbon = net.ribbon(r);
        match ribbon.next {
            Some(n) if s >= ribbon.length() => {
                s -= ribbon.length();
                r = n;
            }
            _ => return ribbon.y_bounds(s),
        }
    }
}

/// Lateral gap between a body at `y` and another on its way from `yj` to its
/// lateral target `target`: an arrival must not be put in the path of a
/// vehicle that is already moving across.
fn swept_gap(y: f64, half: f64, yj: f64, target: f64, half_j: f64) -> f64 {
    let (lo, hi) = (yj.min(target), yj.max(target));
    edge_gap(y, half, y.clamp(lo, hi), half_j)
}

/// Sort `v` by `less`. Orders move little between steps, so insertion sort
/// (linear when nothing changed) is used unless many elements are out of
/// place. Keys are unique (ids break ties), so the result is the same as any
/// other sort.
fn sort_nearly_sorted(v: &mut [u32], less: impl Fn(u32, u32) -> bool) {
    /// Element moves beyond which insertion sort gives way to a full sort.
    const MAX_SHIFTS_PER_ELEMENT: usize = 4;
    let budget = MAX_SHIFTS_PER_ELEMENT * v.len();
    let mut shifts = 0;
    for i in 1..v.len() {
        let x = v[i];
        let mut j = i;
        while j > 0 && less(x, v[j - 1]) {
            v[j] = v[j - 1];
            j -= 1;
        }
        v[j] = x;
        shifts += i - j;
        if shifts > budget {
            v.sort_unstable_by(|&a, &b| {
                if less(a, b) {
                    std::cmp::Ordering::Less
                } else if less(b, a) {
                    std::cmp::Ordering::Greater
                } else {
                    std::cmp::Ordering::Equal
                }
            });
            return;
        }
    }
}

/// Steps between an agent's scheduled lateral decisions.
fn decision_steps(p: &CoreParams, dt: f64) -> u64 {
    ((p.decision_interval / dt).round() as u64).max(1)
}
