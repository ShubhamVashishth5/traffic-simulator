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

/// Dense traffic at the crossing. **Known to fail**: vehicles from one arm
/// that leave it side by side for different movements can lock against each
/// other inside the junction (ADR-0016, open problem 1), after which the
/// collision guard holds them every step and the junction jams.
#[test]
#[ignore = "P7.2 not done: vehicles lock inside the junction in dense traffic (ADR-0016)"]
fn dense_traffic_crosses_without_guard_events() {
    let yaml = crossing(700, 900);
    let mut sim = Simulation::new(&spec(&yaml, &[]), 1, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    let guards = out
        .events
        .iter()
        .filter(|e| e.kind == "collision_guard")
        .count();
    assert_eq!(guards, 0);
}
