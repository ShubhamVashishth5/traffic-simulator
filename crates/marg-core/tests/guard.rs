//! P5.5: the collision guard leaves no overlapping bodies, in random dense
//! micro-scenarios (narrow roads, bottlenecks, chained ribbons, closed
//! barriers, oversaturated mixed demand), and the guard-event rate is
//! reported.
//!
//! Guard events come only from bottlenecks: the lateral model does not look
//! ahead at the road's width profile, so where the road narrows the
//! carriageway pushes vehicles sideways into their neighbours and the guard
//! steps in. Reacting to width changes ahead is road-object work (P6.1);
//! until then the test requires zero guard events where the width does not
//! change.
//!
//! `cargo test --release -p marg-core --test guard` runs a sample;
//! `-- --ignored` runs the full 10⁴ scenarios of the task card.

mod common;
use common::*;
use marg_core::Simulation;
use marg_core::model::geometry::{Body, overlap};
use marg_core::rng::{StreamKind, StreamRng};
use marg_params::VehicleType;

/// What one scenario produced.
#[derive(Debug, Default, Clone, Copy)]
struct Tally {
    /// Scenarios whose road narrows (and the guard events in them).
    bottlenecks: u64,
    bottleneck_guard_events: u64,
    scenarios: u64,
    agent_steps: u64,
    max_agents: usize,
    guard_events: u64,
    /// Scenarios with at least one guard event.
    guarded_scenarios: u64,
    overlaps: u64,
}

impl Tally {
    fn add(&mut self, o: Tally) {
        self.bottlenecks += o.bottlenecks;
        self.bottleneck_guard_events += o.bottleneck_guard_events;
        self.scenarios += o.scenarios;
        self.agent_steps += o.agent_steps;
        self.max_agents = self.max_agents.max(o.max_agents);
        self.guard_events += o.guard_events;
        self.guarded_scenarios += o.guarded_scenarios;
        self.overlaps += o.overlaps;
    }
}

/// Random scenario number `case` (YAML, and whether its road narrows),
/// drawn from its own stream.
fn random_scenario(case: u64) -> (String, bool) {
    let mut rng = StreamRng::new(case, StreamKind::Environment, 0);
    let mut u = |lo: f64, hi: f64| lo + (hi - lo) * rng.uniform();
    let length = u(120.0, 400.0).round();
    let width = u(2.0, 10.5);
    let chained = u(0.0, 1.0) < 0.3;
    let bottleneck = u(0.0, 1.0) < 0.4;
    let narrow = width * u(0.4, 0.8);
    let flow = u(1500.0, 7000.0).round();
    let duration = u(60.0, 100.0).round();
    // A width profile with an optional bottleneck in the middle third.
    let profile = |from: f64, to: f64| {
        let half = 0.5 * width;
        let point = |s: f64, h: f64| format!("{{s: {s:.1}, y_min: {:.3}, y_max: {:.3}}}", -h, h);
        if !bottleneck {
            return point(0.0, half);
        }
        let (b0, b1) = (length / 3.0, 2.0 * length / 3.0);
        let pts: Vec<String> = [
            (b0 - 15.0, half),
            (b0, 0.5 * narrow),
            (b1, 0.5 * narrow),
            (b1 + 15.0, half),
        ]
        .iter()
        .filter(|(s, _)| *s >= from && *s <= to)
        .map(|&(s, h)| point(s - from, h))
        .collect();
        if pts.is_empty() {
            point(0.0, half)
        } else {
            pts.join(", ")
        }
    };
    let split = (0.5 * length).round();
    let ribbons = if chained {
        format!(
            "    - {{id: a, polyline: [[0, 0], [{split}, 0]], width: [{}], next: b}}\n    - {{id: b, polyline: [[{split}, 0], [{length}, 0]], width: [{}]}}\n",
            profile(0.0, split),
            profile(split, length)
        )
    } else {
        format!(
            "    - {{id: a, polyline: [[0, 0], [{length}, 0]], width: [{}]}}\n",
            profile(0.0, length)
        )
    };
    let barrier = if u(0.0, 1.0) < 0.5 {
        let s = u(0.5, 0.9) * if chained { split } else { length };
        let from = u(0.0, 30.0);
        let to = from + u(20.0, 60.0);
        format!(
            "  barriers: [{{id: gate, ribbon: a, s: {s:.1}, closed: [{{from_s: {from:.1}, to_s: {to:.1}}}]}}]\n"
        )
    } else {
        String::new()
    };
    // One to six vehicle types, random shares.
    let n_types = 1 + (u(0.0, 6.0) as usize).min(5);
    let mut composition = Vec::new();
    for _ in 0..n_types {
        let t = VehicleType::ALL
            [(u(0.0, VehicleType::ALL.len() as f64) as usize).min(VehicleType::ALL.len() - 1)];
        let share = u(0.05, 1.0);
        if !composition.iter().any(|(k, _): &(&str, f64)| *k == t.key()) {
            composition.push((t.key(), share));
        }
    }
    let composition = composition
        .iter()
        .map(|(k, w)| format!("{k}: {w:.3}"))
        .collect::<Vec<_>>()
        .join(", ");
    let yaml = format!(
        "name: guard_{case}\nduration_s: {duration}\nnetwork:\n  ribbons:\n{ribbons}{barrier}demand: {{entries: [{{id: west, ribbon: a, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {{{composition}}}}}]}}\n"
    );
    (yaml, bottleneck)
}

/// Run scenario `case`, checking every pair of bodies after every step.
fn run_case(case: u64) -> Tally {
    let (yaml, bottleneck) = random_scenario(case);
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, case, 1).unwrap();
    let offsets: Vec<f64> = {
        let net = sim.network();
        let mut off = vec![0.0; net.len()];
        for r in 1..net.len() {
            off[r] = off[r - 1] + net.ribbon(r - 1).length();
        }
        off
    };
    let mut t = Tally {
        scenarios: 1,
        ..Tally::default()
    };
    let mut bodies: Vec<(Body, u64)> = Vec::new();
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        t.max_agents = t.max_agents.max(a.len());
        bodies.clear();
        bodies.extend((0..a.len()).map(|i| {
            let s = a.s[i] + offsets[a.ribbon[i] as usize];
            let b = Body::from_tan(s, a.y[i], a.length[i], a.width[i], a.heading_tan[i]);
            (b, a.id[i])
        }));
        bodies.sort_by(|x, y| x.0.s.total_cmp(&y.0.s));
        let reach = a.length.iter().copied().fold(0.0, f64::max);
        for (k, (bi, id_i)) in bodies.iter().enumerate() {
            for (bj, id_j) in &bodies[k + 1..] {
                if bj.s - bi.s > reach {
                    break;
                }
                if overlap(bi, bj) {
                    t.overlaps += 1;
                    eprintln!(
                        "case {case}: agents {id_i} and {id_j} overlap at t = {:.1} s",
                        sim.time()
                    );
                }
            }
        }
    }
    let guards = sim
        .events()
        .iter()
        .filter(|e| e.kind == "collision_guard")
        .count() as u64;
    t.guard_events = guards;
    if bottleneck {
        t.bottlenecks = 1;
        t.bottleneck_guard_events = guards;
    }
    t.guarded_scenarios = u64::from(guards > 0);
    t.agent_steps = sim.summary().agent_steps;
    t
}

/// Run `n` scenarios on all cores and report the guard-event rate.
fn run_many(n: u64) -> Tally {
    let threads = std::thread::available_parallelism().map_or(1, |p| p.get()) as u64;
    let total = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|w| {
                scope.spawn(move || {
                    let mut t = Tally::default();
                    for case in (w..n).step_by(threads as usize) {
                        t.add(run_case(case));
                    }
                    t
                })
            })
            .collect();
        let mut t = Tally::default();
        for h in handles {
            t.add(h.join().unwrap());
        }
        t
    });
    eprintln!(
        "P5.5 guard: {} scenarios, {} agent-steps (up to {} agents at once), {} overlaps after the guard; \
         {} guard events in {} scenarios = {:.2} per 10^6 agent-steps; \
         {} of the events in the {} scenarios whose road narrows",
        total.scenarios,
        total.agent_steps,
        total.max_agents,
        total.overlaps,
        total.guard_events,
        total.guarded_scenarios,
        total.guard_events as f64 * 1e6 / total.agent_steps.max(1) as f64,
        total.bottleneck_guard_events,
        total.bottlenecks,
    );
    total
}

#[test]
fn scenarios_are_dense_and_varied() {
    let (yaml, _) = random_scenario(7);
    assert!(yaml.contains("veh_per_h"));
    let t = run_case(7);
    assert!(t.max_agents >= 10, "{t:?}");
    let kinds: Vec<_> = (0..200).map(random_scenario).collect();
    let share = |f: &dyn Fn(&(String, bool)) -> bool| kinds.iter().filter(|k| f(k)).count();
    assert!(share(&|k| k.1) > 50, "bottlenecks");
    assert!(share(&|k| k.0.contains("barriers")) > 50, "barriers");
    assert!(share(&|k| k.0.contains("next: b")) > 30, "chained ribbons");
}

/// Zero overlaps; and where the road does not narrow, zero guard events.
fn check(t: &Tally) {
    assert_eq!(t.overlaps, 0, "{t:?}");
    assert_eq!(t.guard_events, t.bottleneck_guard_events, "{t:?}");
}

/// A sample of the random scenarios (fast enough for every test run).
#[test]
fn no_overlaps_after_the_guard_sample() {
    check(&run_many(200));
}

/// The task card's 10⁴ scenarios (a few minutes on 4 cores, release build).
#[test]
#[ignore]
fn no_overlaps_after_the_guard_10k() {
    check(&run_many(10_000));
}
