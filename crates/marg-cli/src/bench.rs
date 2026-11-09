//! `marg bench`: wall time and ns per agent-step for a scenario, headless
//! with trajectories off (plan/02-architecture.md performance targets).

use anyhow::Result;
use marg_core::Simulation;
use std::time::Instant;

pub fn run(
    spec: &marg_params::ResolvedSpec,
    threads: usize,
    repeat: usize,
) -> Result<serde_json::Value> {
    let mut best = f64::INFINITY;
    let mut agent_steps = 0;
    for _ in 0..repeat.max(1) {
        let start = Instant::now();
        let mut sim = Simulation::new(spec, 1, threads)?;
        let s = sim.run_to_end();
        best = best.min(start.elapsed().as_secs_f64());
        agent_steps = s.agent_steps;
    }
    let ns = best * 1e9 / agent_steps.max(1) as f64;
    Ok(serde_json::json!({
        "scenario": spec.scenario.name,
        "threads": threads,
        "sim_time_s": spec.scenario.duration_s,
        "wall_time_s": best,
        "agent_steps": agent_steps,
        "ns_per_agent_step": ns,
        "agent_steps_per_s": 1e9 / ns,
        "real_time_factor": spec.scenario.duration_s / best,
    }))
}
