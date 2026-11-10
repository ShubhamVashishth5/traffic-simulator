//! Micro-scenario tests of the corridor core (plan P2.4–P2.6).

mod common;
use common::*;
use marg_core::Simulation;

/// Single file (everyone at the centre line): behind a slower leader, every
/// follower settles at the IDM equilibrium gap for the leader's speed.
#[test]
fn platoon_reaches_idm_equilibrium_gaps() {
    // A road one vehicle wide, so nobody can pass (lateral movement, P5).
    let yaml = corridor(4000.0, 900.0, 240.0, "{hatchback: 0.6, e_rickshaw: 0.4}")
        .replace("y_min: -5.25, y_max: 5.25", "y_min: -1.1, y_max: 1.1");
    let spec = spec(
        &yaml,
        &[
            (
                "demand.entry_lateral_position.car",
                "{dist: constant, value: 0.5}",
            ),
            (
                "demand.entry_lateral_position.e_rickshaw",
                "{dist: constant, value: 0.5}",
            ),
            (
                "vehicles.median_band.speed_bonus",
                "{dist: constant, value: 0}",
            ),
        ],
    );
    let mut sim = Simulation::new(&spec, 11, 1).unwrap();
    while !sim.finished() {
        sim.step();
    }
    let a = sim.agents();
    let mut idx: Vec<usize> = (0..a.len()).collect();
    idx.sort_by(|&x, &y| a.s[x].total_cmp(&a.s[y]));
    let mut checked = 0;
    for w in idx.windows(2) {
        let (f, l) = (w[0], w[1]);
        let gap = a.s[l] - a.s[f] - 0.5 * (a.length[f] + a.length[l]);
        let (v, vl) = (a.v[f], a.v[l]);
        // Settled followers only: same speed as the leader, and constrained by it.
        if (v - vl).abs() > 0.01 || v > 0.95 * a.v0[f] || gap > 150.0 {
            continue;
        }
        let s_star = a.standstill_gap[f] + v * a.time_gap[f];
        let eq = s_star / (1.0 - (v / a.v0[f]).powi(4)).sqrt();
        assert!(
            (gap - eq).abs() / eq < 0.02,
            "gap {gap:.2} vs equilibrium {eq:.2} (v {v:.2})"
        );
        checked += 1;
    }
    assert!(checked >= 10, "only {checked} settled followers");
}

/// Lateral separation matters: cars entering at the median side with
/// e-rickshaws entering at the kerb of a 10.5 m road are delayed much less
/// than the same cars entering on the e-rickshaws' line (the same car arrivals
/// and draws in every run, because each class has its own arrival stream).
/// Since P5 the two streams spread towards each other, so "separated" is no
/// longer identical to "cars alone" as it was with fixed lateral positions;
/// measured here: 1.7 s alone, 4.8 s separated, 11.0 s on the same line.
#[test]
fn laterally_separated_slow_vehicles_do_not_interfere() {
    let run = |flow: f64, comp: &str, car_y: &str, erick_y: &str| {
        let spec = spec(
            &corridor(2000.0, 900.0, flow, comp),
            &[
                ("demand.entry_lateral_position.car", car_y),
                ("demand.entry_lateral_position.e_rickshaw", erick_y),
                // Without the median band's speed bonus, which is an
                // incentive of its own to move towards the median.
                (
                    "vehicles.median_band.speed_bonus",
                    "{dist: constant, value: 0}",
                ),
            ],
        );
        let mut sim = Simulation::new(&spec, 5, 1).unwrap();
        sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        out.agents
            .into_iter()
            .filter(|r| r.class == "car")
            .collect::<Vec<_>>()
    };
    let (kerb, median) = (
        "{dist: constant, value: 1.0}",
        "{dist: constant, value: 0.0}",
    );
    let alone = run(300.0, "{hatchback: 1.0}", median, kerb);
    let separated = run(600.0, "{hatchback: 0.5, e_rickshaw: 0.5}", median, kerb);
    let same_line = run(600.0, "{hatchback: 0.5, e_rickshaw: 0.5}", median, median);
    assert!(alone.len() > 50);
    let ids = |v: &[marg_core::AgentRecord]| v.iter().map(|r| r.agent_id).collect::<Vec<_>>();
    assert_eq!(ids(&alone), ids(&separated), "the same cars arrive");
    let mean = |v: &[marg_core::AgentRecord]| {
        let d: Vec<f64> = v.iter().filter(|r| r.exited).map(|r| r.delay).collect();
        d.iter().sum::<f64>() / d.len() as f64
    };
    let (alone, separated, same_line) = (mean(&alone), mean(&separated), mean(&same_line));
    assert!(
        alone < separated && separated - alone < 0.5 * (same_line - alone),
        "alone {alone:.2} s, separated {separated:.2} s, same line {same_line:.2} s"
    );
    assert!(
        same_line > separated + 3.0,
        "same line {same_line:.2} s vs separated {separated:.2} s"
    );
}

/// Vehicles are conserved: spawned = exited + in network, and nothing
/// overlaps (the collision guard never fires) on the reference corridor.
#[test]
fn conservation_and_no_overlaps_on_reference_corridor() {
    let mut yaml = scenario_file("corridor_2km.yaml");
    yaml = yaml
        .replace("duration_s: 3600", "duration_s: 900")
        .replace("warmup_s: 300", "warmup_s: 120");
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 2, 1).unwrap();
    let summary = sim.run_to_end();
    let in_net = sim.agent_count() as u64;
    let (_, out) = sim.finish().unwrap();
    assert_eq!(summary.spawned, summary.exited + in_net);
    assert_eq!(out.agents.len() as u64, summary.spawned);
    let guards = out
        .kpis
        .iter()
        .find(|(k, _)| *k == "collision_guard_events")
        .unwrap()
        .1;
    assert_eq!(guards, 0.0, "collision guard fired");
}

/// Realised arrivals per class match the demand, and headways follow the
/// shifted exponential (Kolmogorov–Smirnov at 1%).
#[test]
fn arrivals_follow_shifted_exponential() {
    // Low flow on a wide road so no arrival is delayed; one class only.
    let yaml = corridor(500.0, 7200.0, 600.0, "{motorcycle: 1.0}");
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 9, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    let mut t: Vec<f64> = out.agents.iter().map(|r| r.t_arrival).collect();
    t.sort_by(f64::total_cmp);
    let n = t.len() as f64;
    let expected = 600.0 * 2.0;
    assert!(
        (n - expected).abs() < 3.0 * expected.sqrt(),
        "{n} arrivals vs {expected}"
    );
    let h: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
    let h_min = spec.scalar_si("demand.min_headway.two_wheeler").unwrap();
    let mean = 3600.0 / 600.0;
    assert!(h.iter().all(|&x| x >= h_min - 1e-9));
    let mut hs = h.clone();
    hs.sort_by(f64::total_cmp);
    let m = hs.len() as f64;
    let d = hs
        .iter()
        .enumerate()
        .map(|(i, &x)| {
            let cdf = 1.0 - (-(x - h_min) / (mean - h_min)).exp();
            ((i as f64 + 1.0) / m - cdf)
                .abs()
                .max((cdf - i as f64 / m).abs())
        })
        .fold(0.0, f64::max);
    assert!(
        d < 1.63 / m.sqrt(),
        "KS statistic {d:.4} vs critical {:.4}",
        1.63 / m.sqrt()
    );
}

/// Composition shares are realised per type within binomial error.
#[test]
fn composition_is_realised() {
    let yaml = corridor(
        300.0,
        3600.0,
        1500.0,
        "{motorcycle: 0.3, scooter: 0.2, hatchback: 0.4, bus: 0.1}",
    );
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 4, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    let n = out.agents.len() as f64;
    for (t, share) in [
        ("motorcycle", 0.3),
        ("scooter", 0.2),
        ("hatchback", 0.4),
        ("bus", 0.1),
    ] {
        let k = out.agents.iter().filter(|r| r.vehicle_type == t).count() as f64;
        let sd = (n * share * (1.0 - share)).sqrt();
        assert!(
            (k - n * share).abs() < 4.0 * sd,
            "{t}: {k} of {n}, expected {:.0}",
            n * share
        );
    }
}

/// Same agent id ⇒ same draws, whatever else is in the scenario
/// (common random numbers across arms).
#[test]
fn agent_draws_do_not_depend_on_other_traffic() {
    let run = |comp: &str| {
        let spec = spec(&corridor(300.0, 600.0, 900.0, comp), &[]);
        let mut sim = Simulation::new(&spec, 21, 1).unwrap();
        sim.run_to_end();
        sim.finish().unwrap().1.agents
    };
    let a = run("{hatchback: 0.5, bus: 0.5}");
    let b = run("{hatchback: 0.5, bus: 0.5, motorcycle: 0.3}");
    let cars_a: Vec<_> = a.iter().filter(|r| r.class == "car").take(50).collect();
    for ra in cars_a {
        let rb = b
            .iter()
            .find(|r| r.agent_id == ra.agent_id)
            .expect("same car exists in both arms");
        assert_eq!(
            (ra.v0, ra.time_gap, ra.length),
            (rb.v0, rb.time_gap, rb.length)
        );
    }
}
