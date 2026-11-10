//! Micro-scenario tests of lateral behaviour (plan P5.3–P5.6, ADR-0010).

mod common;
use common::*;
use marg_core::Simulation;

const NO_BONUS: (&str, &str) = (
    "vehicles.median_band.speed_bonus",
    "{dist: constant, value: 0}",
);
const CENTRE: &str = "{dist: constant, value: 0.5}";

fn kpi(out: &marg_core::RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

/// P5.4: |heading| ≤ θ_max at every step, for every agent, and nobody leaves
/// the carriageway. A stream dense enough that vehicles move sideways a lot.
#[test]
fn heading_stays_within_the_limit() {
    let yaml = scenario_file("corridor_2km.yaml").replace("duration_s: 3600", "duration_s: 400");
    let spec = spec(&yaml, &[]);
    let limit = spec.scalar_si("interaction.lateral.max_heading").unwrap();
    let mut sim = Simulation::new(&spec, 3, 1).unwrap();
    let (mut steepest, mut moved): (f64, f64) = (0.0, 0.0);
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            let h = a.heading(i);
            assert!(
                h.abs() <= limit + 1e-12,
                "heading {h} at t = {}",
                sim.time()
            );
            let half = 0.5 * a.width[i];
            assert!(a.y[i] - half >= -5.25 - 1e-9 && a.y[i] + half <= 5.25 + 1e-9);
            steepest = steepest.max(h.abs());
            moved = moved.max((a.y[i] - a.entry_y[i]).abs());
        }
    }
    assert!(
        steepest > 0.02 && moved > 1.0,
        "the test must exercise lateral movement (steepest {steepest}, moved {moved})"
    );
}

/// P5.3 (1): a slow leader in a wide road is passed; on a road one vehicle
/// wide the same cars stay behind it.
#[test]
fn a_slow_leader_is_passed_when_there_is_room() {
    let run = |width: f64| {
        let yaml = corridor(1500.0, 600.0, 400.0, "{hatchback: 0.7, e_rickshaw: 0.3}").replace(
            "y_min: -5.25, y_max: 5.25",
            &format!("y_min: {}, y_max: {}", -0.5 * width, 0.5 * width),
        );
        let spec = spec(
            &yaml,
            &[
                ("demand.entry_lateral_position.car", CENTRE),
                ("demand.entry_lateral_position.e_rickshaw", CENTRE),
                NO_BONUS,
            ],
        );
        let mut sim = Simulation::new(&spec, 4, 1).unwrap();
        sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        let cars: Vec<_> = out
            .agents
            .iter()
            .filter(|r| r.class == "car" && r.exited)
            .collect();
        let delay = cars.iter().map(|r| r.delay).sum::<f64>() / cars.len() as f64;
        // Overtakes: pairs that left in the opposite order to their entry.
        let mut order: Vec<_> = out.agents.iter().filter(|r| r.exited).collect();
        order.sort_by(|a, b| a.t_entry.total_cmp(&b.t_entry));
        let overtakes = order
            .windows(2)
            .filter(|w| w[1].t_exit.unwrap() < w[0].t_exit.unwrap())
            .count();
        (delay, overtakes, kpi(&out, "collision_guard_events"))
    };
    let (wide_delay, wide_overtakes, wide_guards) = run(10.5);
    let (narrow_delay, narrow_overtakes, narrow_guards) = run(2.2);
    assert_eq!(narrow_overtakes, 0, "no room to pass on a 2.2 m road");
    assert!(wide_overtakes >= 5, "only {wide_overtakes} overtakes");
    assert!(
        wide_delay < 0.5 * narrow_delay,
        "wide {wide_delay:.1} s vs narrow {narrow_delay:.1} s"
    );
    assert_eq!(wide_guards + narrow_guards, 0.0);
}

/// A closed barrier holds everybody behind the stop line and releases them
/// when it opens (ADR-0010; signals arrive in P7).
#[test]
fn a_closed_barrier_holds_the_queue() {
    let yaml = corridor(600.0, 240.0, 1800.0, "{hatchback: 0.5, motorcycle: 0.5}").replace(
        "demand:",
        "  barriers: [{id: gate, ribbon: main, s: 400, closed: [{from_s: 0, to_s: 150}]}]\ndemand:",
    );
    let yaml = yaml
        .replace("network: {ribbons", "network:\n  ribbons")
        .replace(
            "y_max: 5.25}]}]}\n  barriers",
            "y_max: 5.25}]}]\n  barriers",
        );
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 6, 1).unwrap();
    while sim.time() < 149.0 {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            assert!(
                a.s[i] + 0.5 * a.length[i] <= 400.0 + 1e-6,
                "agent past the closed barrier at t = {}",
                sim.time()
            );
        }
    }
    let queued = (0..sim.agent_count())
        .filter(|&i| sim.agents().v[i] < 0.5 && sim.agents().s[i] > 300.0)
        .count();
    assert!(queued >= 10, "only {queued} vehicles queued");
    sim.run_to_end();
    let exited = sim.summary().exited;
    let (_, out) = sim.finish().unwrap();
    assert!(exited >= 20, "only {exited} left after the barrier opened");
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}

/// P5.4: at a closed barrier, two-wheelers work their way towards the front
/// of the queue through the gaps and cars do not: on average a motorcycle
/// stops nearer the stop line than a car that arrived at the same time.
#[test]
fn two_wheelers_filter_to_the_front_of_a_queue() {
    let yaml = corridor(600.0, 200.0, 2400.0, "{hatchback: 0.5, motorcycle: 0.5}").replace(
        "demand:",
        "  barriers: [{id: gate, ribbon: main, s: 400, closed: [{from_s: 0, to_s: 1000}]}]\ndemand:",
    );
    let yaml = yaml
        .replace("network: {ribbons", "network:\n  ribbons")
        .replace(
            "y_max: 5.25}]}]}\n  barriers",
            "y_max: 5.25}]}]\n  barriers",
        );
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 8, 1).unwrap();
    sim.run_to_end();
    let a = sim.agents();
    // Rank in the queue (0 = nearest the line) against rank of arrival.
    let mut by_arrival: Vec<usize> = (0..a.len()).collect();
    by_arrival.sort_by(|&x, &y| a.t_entry[x].total_cmp(&a.t_entry[y]));
    let mut by_position: Vec<usize> = (0..a.len()).collect();
    by_position.sort_by(|&x, &y| a.s[y].total_cmp(&a.s[x]));
    let rank = |order: &[usize], i: usize| order.iter().position(|&x| x == i).unwrap() as f64;
    let gain = |class: &str| {
        let ids: Vec<usize> = (0..a.len())
            .filter(|&i| marg_params::VehicleClass::ALL[usize::from(a.class[i])].key() == class)
            .collect();
        ids.iter()
            .map(|&i| rank(&by_arrival, i) - rank(&by_position, i))
            .sum::<f64>()
            / ids.len() as f64
    };
    let (bikes, cars) = (gain("two_wheeler"), gain("car"));
    assert!(a.len() >= 40, "only {} vehicles queued", a.len());
    assert!(
        bikes > 1.0 && cars < 0.0,
        "places gained in the queue: two-wheelers {bikes:.2}, cars {cars:.2}"
    );
}

/// ADR-0007: the cooperative twin fixes κ of the squeezing rule at a
/// population quantile. Same agents and draws, different trajectories.
#[test]
fn the_cooperative_twin_changes_squeezing_only() {
    let yaml = scenario_file("corridor_2km.yaml").replace("duration_s: 3600", "duration_s: 600");
    let run = |overrides: &[(&str, &str)]| {
        let spec = spec(&yaml, overrides);
        let mut sim = Simulation::new(&spec, 2, 1).unwrap();
        let summary = sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        (summary.output_hash, out.agents)
    };
    let (observed_hash, observed) = run(&[]);
    let (twin_hash, twin) = run(&[("personas.cooperative.squeeze_kappa_quantile", "0.25")]);
    assert_ne!(observed_hash, twin_hash, "the twin switch is not read");
    // Common random numbers: the agents both arms share have the same draws.
    let mut shared = 0;
    for r in &observed {
        if let Some(t) = twin.iter().find(|t| t.agent_id == r.agent_id) {
            assert_eq!(
                (r.v0, r.kappa, r.width, r.time_gap),
                (t.v0, t.kappa, t.width, t.time_gap)
            );
            shared += 1;
        }
    }
    assert!(shared > 300, "only {shared} shared agents");
    // κ is spread as the register says (standard normal by default).
    let n = observed.len() as f64;
    let mean = observed.iter().map(|r| r.kappa).sum::<f64>() / n;
    let sd = (observed
        .iter()
        .map(|r| (r.kappa - mean).powi(2))
        .sum::<f64>()
        / n)
        .sqrt();
    assert!(
        mean.abs() < 0.15 && (sd - 1.0).abs() < 0.15,
        "κ mean {mean}, sd {sd}"
    );
}

/// Conflicts are recorded with their pair, type and minimum TTC under the
/// register's threshold, and the KPI counts the episodes after warm-up.
#[test]
fn conflict_episodes_are_consistent() {
    let yaml = scenario_file("corridor_2km.yaml")
        .replace("duration_s: 3600", "duration_s: 900")
        .replace("warmup_s: 300", "warmup_s: 100");
    let spec = spec(&yaml, &[]);
    let threshold = spec.scalar_si("observers.conflict.ttc_threshold").unwrap();
    let mut sim = Simulation::new(&spec, 1, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    assert!(out.conflicts.len() > 10, "{} episodes", out.conflicts.len());
    for c in &out.conflicts {
        assert!(c.min_ttc >= 0.0 && c.min_ttc < threshold);
        assert!(c.t_end >= c.t_start);
        assert_ne!(c.agent_id, c.other_id);
        assert!(["rear_end", "sideswipe"].contains(&c.kind), "{}", c.kind);
        assert!(c.max_drac.is_nan() || c.max_drac >= 0.0);
    }
    assert!(
        out.conflicts
            .windows(2)
            .all(|w| w[0].t_start <= w[1].t_start)
    );
    let after_warmup = out.conflicts.iter().filter(|c| c.t_start >= 100.0).count();
    assert_eq!(kpi(&out, "conflicts"), after_warmup as f64);
    assert_eq!(
        kpi(&out, "conflicts"),
        kpi(&out, "conflicts_rear_end") + kpi(&out, "conflicts_sideswipe")
    );
}
