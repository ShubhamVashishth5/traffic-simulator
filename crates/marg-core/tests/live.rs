//! Live-viewer hooks (plan P4): parameter changes between steps, resampling,
//! live detectors, KPIs so far and the inspector's interaction terms.

mod common;
use common::*;
use marg_core::Simulation;
use marg_core::model::longitudinal::combine;
use marg_core::{RunOutputs, RunSummary};
use marg_params::{ParamValue, ResolvedSpec, VehicleType};

fn run_all(mut sim: Simulation) -> (RunSummary, RunOutputs) {
    sim.run_to_end();
    sim.finish().unwrap()
}

fn hatchback() -> u8 {
    VehicleType::Hatchback.index() as u8
}

const CARS: &str = "{hatchback: 0.6, motorcycle: 0.3, auto_rickshaw: 0.1}";
const V0: &str = "vehicles.free_flow_speed.hatchback";

fn base() -> ResolvedSpec {
    spec(&corridor(800.0, 240.0, 2400.0, CARS), &[])
}

fn fast() -> ParamValue {
    serde_norway::from_str("{dist: trunc_normal, mean: 80, sd: 15.52, min: 15, max: 120}").unwrap()
}

fn run_with_change(at_step: u64, resample: bool) -> (Simulation, ResolvedSpec) {
    let mut spec = base();
    let mut sim = Simulation::new(&spec, 5, 1).unwrap();
    while !sim.finished() {
        if sim.step_index() == at_step {
            spec.set_live(&register(), V0, &fast()).unwrap();
            sim.update_params(&spec).unwrap();
            if resample {
                assert!(sim.resample(V0).unwrap() > 0);
            }
        }
        sim.step();
    }
    (sim, spec)
}

#[test]
fn a_change_at_the_same_step_gives_the_same_run() {
    // With resampling the change acts on the next step, so one step later differs.
    let (a, _) = run_with_change(600, true);
    let (b, _) = run_with_change(600, true);
    let (c, _) = run_with_change(601, true);
    let plain = Simulation::new(&base(), 5, 1).unwrap().run_to_end();
    assert_eq!(a.summary(), b.summary());
    assert_ne!(a.summary().output_hash, c.summary().output_hash);
    assert_ne!(a.summary().output_hash, plain.output_hash);
}

#[test]
fn new_agents_get_the_new_distribution_and_old_ones_keep_theirs() {
    let at = 600; // 60 s
    let (sim, _) = run_with_change(at, false);
    let (_, out) = sim.finish().unwrap();
    // Reference: the same seed with the fast distribution from the start.
    let mut fast_spec = base();
    fast_spec.set_live(&register(), V0, &fast()).unwrap();
    let (_, fast_out) = run_all(Simulation::new(&fast_spec, 5, 1).unwrap());
    let (_, slow_out) = run_all(Simulation::new(&base(), 5, 1).unwrap());
    let find = |o: &marg_core::RunOutputs, id| o.agents.iter().find(|r| r.agent_id == id).cloned();
    let (mut early, mut late) = (0, 0);
    for r in out.agents.iter().filter(|r| r.vehicle_type == "hatchback") {
        let (Some(f), Some(s)) = (find(&fast_out, r.agent_id), find(&slow_out, r.agent_id)) else {
            continue;
        };
        if r.t_entry < at as f64 * 0.1 - 1e-9 {
            assert_eq!(r.v0, s.v0, "agent {} entered before the change", r.agent_id);
            early += 1;
        } else {
            assert_eq!(r.v0, f.v0, "agent {} entered after the change", r.agent_id);
            late += 1;
        }
    }
    assert!(early > 5 && late > 5, "early {early}, late {late}");
}

#[test]
fn resample_moves_agents_in_the_network_to_their_new_draw() {
    let mut spec = base();
    let mut sim = Simulation::new(&spec, 5, 1).unwrap();
    for _ in 0..600 {
        sim.step();
    }
    let before: Vec<(u64, f64)> = (0..sim.agent_count())
        .map(|i| (sim.agents().id[i], sim.agents().v0[i]))
        .collect();
    spec.set_live(&register(), V0, &fast()).unwrap();
    sim.update_params(&spec).unwrap();
    let n = sim.resample(V0).unwrap();
    let a = sim.agents();
    let cars = (0..a.len()).filter(|&i| a.vtype[i] == hatchback()).count();
    assert_eq!(n, cars);
    // Same seed, fast distribution from the start: the same agent draws the same v0.
    let mut fresh_spec = base();
    fresh_spec.set_live(&register(), V0, &fast()).unwrap();
    let (_, fresh) = run_all(Simulation::new(&fresh_spec, 5, 1).unwrap());
    for (i, &(id, v_old)) in before.iter().enumerate() {
        let r = fresh.agents.iter().find(|r| r.agent_id == id).unwrap();
        if a.vtype[i] == hatchback() {
            assert_eq!(a.v0[i], r.v0, "agent {id}");
            assert!(
                a.v0[i] > v_old - 1e-9,
                "a higher mean moves every quantile up"
            );
        } else {
            assert_eq!(a.v0[i], v_old, "other types are untouched");
        }
    }
    // Refusals.
    assert!(sim.resample("interaction.lateral.braking_scale").is_err());
    assert!(sim.resample("vehicles.length.hatchback").is_err());
}

#[test]
fn live_detectors_match_a_headless_run_with_that_interval() {
    let yaml = corridor(800.0, 240.0, 2400.0, CARS);
    let mut live = Simulation::new(&spec(&yaml, &[]), 9, 1).unwrap();
    live.enable_live_detectors(20.0).unwrap();
    let mut seen = Vec::new();
    let mut k = 0;
    while !live.finished() {
        live.step();
        let closed = live.live_intervals_closed();
        seen.extend(live.live_detector_records(k, closed));
        k = closed;
    }
    let headless = Simulation::new(
        &spec(&yaml.replace("interval_s: 60", "interval_s: 20"), &[]),
        9,
        1,
    )
    .unwrap();
    let (_, out) = run_all(headless);
    assert_eq!(k, 12);
    assert!(!seen.is_empty());
    // NaN marks "not measured": compare the exact printed values.
    assert_eq!(format!("{seen:?}"), format!("{:?}", out.detectors));
    // And the live window does not change the run itself.
    let plain = Simulation::new(&spec(&yaml, &[]), 9, 1)
        .unwrap()
        .run_to_end();
    assert_eq!(live.summary(), plain);
}

#[test]
fn kpis_so_far_equal_the_final_kpis() {
    let mut sim = Simulation::new(&base(), 3, 1).unwrap();
    sim.run_to_end();
    let now = sim.kpis_now();
    let (_, out) = sim.finish().unwrap();
    assert_eq!(now.len(), out.kpis.len());
    for ((a, x), (b, y)) in now.iter().zip(&out.kpis) {
        assert_eq!(a, b);
        assert!(x == y || (x.is_nan() && y.is_nan()), "{a}: {x} vs {y}");
    }
}

#[test]
fn inspector_terms_reproduce_the_next_acceleration() {
    let mut sim = Simulation::new(&base(), 3, 1).unwrap();
    for _ in 0..900 {
        sim.step();
    }
    let p = sim.params().clone();
    let mut checked = 0;
    let terms: Vec<_> = (0..sim.agent_count())
        .map(|i| {
            (
                sim.agents().id[i],
                sim.interactions(i),
                sim.agents().v[i],
                sim.agents().v0[i],
                sim.agents().max_accel[i],
            )
        })
        .collect();
    sim.step();
    for (id, terms, v, v0, a_max) in terms {
        let Some(i) = sim.agent_index(id) else {
            continue;
        };
        let free = a_max * (1.0 - (v / v0).powf(p.accel_exponent));
        let mut acc = free;
        for t in &terms {
            if t.weight.is_nan() {
                if t.lateral_gap < 0.0 {
                    acc = -p.emergency_decel;
                }
            } else {
                acc = combine(acc, free, t.leader_accel, t.weight);
            }
        }
        let acc = acc.clamp(-p.emergency_decel, a_max);
        assert!(
            (acc - sim.agents().acc[i]).abs() < 1e-9,
            "agent {id}: {acc} vs {}",
            sim.agents().acc[i]
        );
        checked += 1;
    }
    assert!(checked > 20);
}
