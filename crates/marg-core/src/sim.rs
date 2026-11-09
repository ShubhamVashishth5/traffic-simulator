//! The step loop.
//!
//! Phase P0 skeleton: a single toy agent kind that enters at s = 0 with a
//! Poisson arrival process, moves at a constant drawn speed and leaves at
//! `TOY_LENGTH_M`. It exists to exercise the phase structure, the RNG streams,
//! the thread pool and the determinism harness; phase P2 replaces it.

use crate::rng::{StreamKind, StreamRng};
use marg_params::Scenario;
use rayon::prelude::*;

/// Length of the toy road in the P0 skeleton.
const TOY_LENGTH_M: f64 = 1000.0;

/// Errors raised while building or running a simulation.
#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("invalid scenario: {0}")]
    InvalidScenario(String),
    #[error("thread pool: {0}")]
    ThreadPool(String),
}

/// Struct-of-arrays agent store (skeleton).
#[derive(Debug, Default, Clone)]
struct Agents {
    id: Vec<u64>,
    s: Vec<f64>,
    v: Vec<f64>,
    /// Per-step buffer written by the parallel move phase.
    v_next: Vec<f64>,
}

impl Agents {
    fn len(&self) -> usize {
        self.id.len()
    }
    fn push(&mut self, id: u64, s: f64, v: f64) {
        self.id.push(id);
        self.s.push(s);
        self.v.push(v);
        self.v_next.push(v);
    }
    fn swap_remove(&mut self, i: usize) {
        self.id.swap_remove(i);
        self.s.swap_remove(i);
        self.v.swap_remove(i);
        self.v_next.swap_remove(i);
    }
}

/// Headline numbers for a finished run.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct RunSummary {
    pub steps: u64,
    pub sim_time_s: f64,
    pub spawned: u64,
    pub exited: u64,
    pub agent_steps: u64,
    /// blake3 of every observed state, in step order (hex).
    pub output_hash: String,
}

/// A running simulation.
pub struct Simulation {
    spec: Scenario,
    seed: u64,
    t: f64,
    step: u64,
    agents: Agents,
    demand: StreamRng,
    next_arrival_t: f64,
    next_seq: u64,
    spawned: u64,
    exited: u64,
    agent_steps: u64,
    pool: rayon::ThreadPool,
    hasher: blake3::Hasher,
}

impl Simulation {
    /// Build a simulation for `spec` with master seed `seed`, using `threads`
    /// worker threads (0 = rayon default).
    pub fn new(spec: Scenario, seed: u64, threads: usize) -> Result<Self, SimError> {
        if !(spec.dt_s > 0.0 && spec.duration_s > 0.0) {
            return Err(SimError::InvalidScenario(
                "dt_s and duration_s must be positive".into(),
            ));
        }
        if spec.toy_arrivals_per_h < 0.0 {
            return Err(SimError::InvalidScenario(
                "toy_arrivals_per_h must be non-negative".into(),
            ));
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|e| SimError::ThreadPool(e.to_string()))?;
        let mut demand = StreamRng::new(seed, StreamKind::Demand, 0);
        let first = next_headway(&mut demand, spec.toy_arrivals_per_h);
        Ok(Self {
            spec,
            seed,
            t: 0.0,
            step: 0,
            agents: Agents::default(),
            demand,
            next_arrival_t: first,
            next_seq: 0,
            spawned: 0,
            exited: 0,
            agent_steps: 0,
            pool,
            hasher: blake3::Hasher::new(),
        })
    }

    /// Simulated time in seconds.
    pub fn time(&self) -> f64 {
        self.t
    }

    /// Number of agents currently in the network.
    pub fn agent_count(&self) -> usize {
        self.agents.len()
    }

    /// Whether the scenario duration has been reached.
    pub fn finished(&self) -> bool {
        self.t >= self.spec.duration_s - 1e-9
    }

    /// Advance one time step.
    pub fn step(&mut self) {
        let dt = self.spec.dt_s;
        self.phase_demand();
        // Perceive and decide: no behaviour in the skeleton.
        self.phase_move();
        self.phase_apply(dt);
        self.phase_observe();
        self.step += 1;
        self.t = self.step as f64 * dt;
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

    fn phase_demand(&mut self) {
        let t_end = self.t + self.spec.dt_s;
        while self.next_arrival_t < t_end {
            let id = self.next_seq;
            self.next_seq += 1;
            let mut draws = StreamRng::new(self.seed, StreamKind::AgentDraws, id);
            let v0 = (10.0 + 5.0 * draws.standard_normal()).max(1.0);
            self.agents.push(id, 0.0, v0);
            self.spawned += 1;
            self.next_arrival_t += next_headway(&mut self.demand, self.spec.toy_arrivals_per_h);
        }
    }

    fn phase_move(&mut self) {
        let agents = &mut self.agents;
        let v = &agents.v;
        self.pool.install(|| {
            agents
                .v_next
                .par_iter_mut()
                .zip(v.par_iter())
                .for_each(|(vn, &vi)| *vn = vi);
        });
    }

    fn phase_apply(&mut self, dt: f64) {
        let a = &mut self.agents;
        for i in 0..a.len() {
            a.v[i] = a.v_next[i];
            a.s[i] += a.v[i] * dt;
        }
        self.agent_steps += a.len() as u64;
        let mut i = a.len();
        while i > 0 {
            i -= 1;
            if a.s[i] > TOY_LENGTH_M {
                a.swap_remove(i);
                self.exited += 1;
            }
        }
    }

    fn phase_observe(&mut self) {
        let a = &self.agents;
        self.hasher.update(&self.step.to_le_bytes());
        for i in 0..a.len() {
            self.hasher.update(&a.id[i].to_le_bytes());
            self.hasher.update(&a.s[i].to_bits().to_le_bytes());
            self.hasher.update(&a.v[i].to_bits().to_le_bytes());
        }
    }
}

fn next_headway(rng: &mut StreamRng, per_hour: f64) -> f64 {
    if per_hour <= 0.0 {
        return f64::INFINITY;
    }
    -libm::log(rng.uniform_open()) * 3600.0 / per_hour
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toy() -> Scenario {
        Scenario {
            name: "toy".into(),
            description: String::new(),
            duration_s: 600.0,
            dt_s: 0.1,
            toy_arrivals_per_h: 1800.0,
        }
    }

    #[test]
    fn deterministic_across_threads_and_reruns() {
        let h1 = Simulation::new(toy(), 7, 1).unwrap().run_to_end();
        let h1b = Simulation::new(toy(), 7, 1).unwrap().run_to_end();
        let h4 = Simulation::new(toy(), 7, 4).unwrap().run_to_end();
        assert_eq!(h1, h1b);
        assert_eq!(h1, h4);
        assert!(h1.spawned > 200, "spawned {}", h1.spawned);
    }

    #[test]
    fn different_seeds_differ() {
        let a = Simulation::new(toy(), 1, 1).unwrap().run_to_end();
        let b = Simulation::new(toy(), 2, 1).unwrap().run_to_end();
        assert_ne!(a.output_hash, b.output_hash);
    }

    #[test]
    fn rejects_bad_scenarios() {
        let mut s = toy();
        s.dt_s = 0.0;
        assert!(Simulation::new(s, 1, 1).is_err());
    }
}
