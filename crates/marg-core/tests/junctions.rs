//! Micro-scenario tests of junctions (plan P7). The geometry tests are in
//! `marg-net`; these run the model.

mod common;
use common::*;
use marg_core::{EventRecord, Simulation};

fn attr(e: &EventRecord, key: &str) -> f64 {
    let rest = e.attrs.split(&format!("\"{key}\":")).nth(1).unwrap();
    rest[..rest.find([',', '}']).unwrap()].parse().unwrap()
}

/// The signalised crossing with every arm's flow set to `flow` veh/h.
fn crossing(flow: u32, duration: u32) -> String {
    scenario_file("signal_crossing.yaml")
        .replace("veh_per_h: 700", &format!("veh_per_h: {flow}"))
        .replace("duration_s: 1800", &format!("duration_s: {duration}"))
        .replace("warmup_s: 300", "warmup_s: 0")
}

/// P7.3: nobody who complies crosses a stop line on red, except a vehicle
/// that could not stop when the signal changed; the signal cycles are
/// recorded; the run is the same on one thread and on four.
#[test]
fn light_traffic_obeys_the_signal_and_is_deterministic() {
    let yaml = crossing(150, 420);
    let run = |threads| {
        let mut sim = Simulation::new(&spec(&yaml, &[]), 3, threads).unwrap();
        sim.set_parallel_min(0);
        sim.run_to_end();
        sim.finish().unwrap()
    };
    let (summary, out) = run(1);
    let (summary4, _) = run(4);
    assert_eq!(summary.output_hash, summary4.output_hash);
    let crossed: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "stop_line_crossed")
        .collect();
    assert!(crossed.len() > 20, "{} crossings", crossed.len());
    for e in &crossed {
        // 3 = red: only at speed (it was past stopping when amber began).
        if attr(e, "light") == 3.0 {
            assert!(
                attr(e, "speed") > 2.0,
                "crossed on red from rest: {}",
                e.attrs
            );
        }
    }
    assert!(out.events.iter().any(|e| e.kind == "signal_state"));
    assert!(out.signal_cycles.len() >= 16);
    assert!(out.signal_cycles.iter().all(|c| c.approach_width == 7.0));
}

/// Agents' records and events of one run of the crossing.
fn run_crossing(yaml: &str, seed: u64) -> marg_core::RunOutputs {
    let mut sim = Simulation::new(&spec(yaml, &[]), seed, 1).unwrap();
    sim.run_to_end();
    sim.finish().unwrap().1
}

fn guards(out: &marg_core::RunOutputs) -> usize {
    out.events
        .iter()
        .filter(|e| e.kind == "collision_guard")
        .count()
}

/// P7.2 (ADR-0016 part 4, decision D): dense traffic at the crossing, every
/// turn, 700 veh/h per arm. Vehicles from one arm that leave it side by side
/// for crossing movements, permitted right turns waiting inside the box,
/// buses: no vehicle is put back by the collision guard, and the junction
/// keeps discharging (it is near saturation: the right turns comply).
#[test]
fn dense_traffic_crosses_without_guard_events() {
    let out = run_crossing(&crossing(700, 900), 1);
    assert_eq!(guards(&out), 0);
    let crossed = out
        .events
        .iter()
        .filter(|e| e.kind == "stop_line_crossed")
        .count();
    let spawned = out.agents.len();
    assert!(
        crossed as f64 > 0.8 * spawned as f64,
        "{crossed} crossings of {spawned} vehicles"
    );
}

/// The same at 300 veh/h per arm, over several seeds: no guard events, and
/// every vehicle but those still on their way gets through. Right turns
/// under the signal wait for their gap inside the box.
#[test]
fn moderate_traffic_every_turn_flows_on_every_seed() {
    let yaml = crossing(300, 900);
    for seed in 1..=3 {
        let out = run_crossing(&yaml, seed);
        assert_eq!(guards(&out), 0, "seed {seed}");
        let exited = out.agents.iter().filter(|a| a.exited).count();
        assert!(
            exited as f64 > 0.9 * out.agents.len() as f64,
            "seed {seed}: {exited} of {} out",
            out.agents.len()
        );
        let gaps: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == "gap_accepted")
            .collect();
        assert!(!gaps.is_empty(), "seed {seed}: no gap taken");
        assert!(
            gaps.iter().any(|e| e.ribbon.starts_with("crossing:")),
            "seed {seed}: no gap taken while waiting inside the box"
        );
    }
}
