//! Bitwise determinism across thread counts and reruns (ADR-0003).

mod common;
use common::*;
use marg_core::Simulation;

fn hash(threads: usize) -> (String, u64) {
    let mut yaml = scenario_file("corridor_2km.yaml");
    yaml = yaml
        .replace("duration_s: 3600", "duration_s: 600")
        .replace("warmup_s: 300", "warmup_s: 60");
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 17, threads).unwrap();
    sim.set_parallel_min(0); // always take the parallel path when threads > 1
    let s = sim.run_to_end();
    (s.output_hash, s.agent_steps)
}

#[test]
fn determinism_across_threads_and_reruns() {
    let one = hash(1);
    assert!(
        one.1 > 100_000,
        "too small to be meaningful: {} agent-steps",
        one.1
    );
    assert_eq!(one, hash(1), "rerun differs");
    assert_eq!(one, hash(4), "4 threads differ from 1");
    assert_eq!(one, hash(3), "3 threads differ from 1");
}
