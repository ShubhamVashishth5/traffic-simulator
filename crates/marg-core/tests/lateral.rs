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

/// For every vehicle that draws past an e-rickshaw, which side it passes on:
/// `(kerb side, median side, e-rickshaws)`. The median is at `y_min`.
fn passes_by_side(sim: &mut Simulation) -> (usize, usize, usize) {
    use std::collections::BTreeMap;
    let erick = marg_params::VehicleClass::ERickshaw.index() as u8;
    // Last sign of (s_other − s_e-rickshaw) per pair.
    let mut behind: BTreeMap<(u64, u64), bool> = BTreeMap::new();
    let (mut kerb, mut median) = (0, 0);
    let mut seen = std::collections::BTreeSet::new();
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for e in (0..a.len()).filter(|&i| a.class[i] == erick) {
            seen.insert(a.id[e]);
            for v in (0..a.len()).filter(|&v| v != e && a.ribbon[v] == a.ribbon[e]) {
                let now_behind = a.s[v] < a.s[e];
                if let Some(was_behind) = behind.insert((a.id[e], a.id[v]), now_behind)
                    && was_behind
                    && !now_behind
                {
                    if a.y[v] > a.y[e] {
                        kerb += 1;
                    } else {
                        median += 1;
                    }
                }
            }
        }
    }
    (kerb, median, seen.len())
}

/// P5.6: the moving-bottleneck reference scenario. E-rickshaws at 21 km/h
/// in a stream of cars and two-wheelers are passed on both sides.
#[test]
fn the_e_rickshaw_is_passed_on_both_sides() {
    let spec = spec(&scenario_file("moving_bottleneck_e_rickshaw.yaml"), &[]);
    let mut sim = Simulation::new(&spec, 1, 1).unwrap();
    let (kerb, median, n) = passes_by_side(&mut sim);
    let (_, out) = sim.finish().unwrap();
    let passes = kerb + median;
    assert!(n >= 5 && passes >= 100, "{n} e-rickshaws, {passes} passes");
    for (side, k) in [("kerb", kerb), ("median", median)] {
        assert!(
            k as f64 >= 0.2 * passes as f64,
            "only {k} of {passes} passes on the {side} side"
        );
    }
    // They move at their own pace: 21 km/h, slower than everybody else.
    let e: Vec<_> = out
        .agents
        .iter()
        .filter(|r| r.class == "e_rickshaw" && r.exited)
        .collect();
    let speed = e.iter().map(|r| r.distance / r.travel_time).sum::<f64>() / e.len() as f64;
    assert!((speed * 3.6 - 21.0).abs() < 4.0, "{:.1} km/h", speed * 3.6);
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}

/// Pooled histogram of lateral crossing positions by class at screenline
/// `id`, after warm-up: `(y0, bin, counts)`.
fn lateral_histogram(out: &marg_core::RunOutputs, id: &str, class: &str) -> (f64, f64, Vec<f64>) {
    let mut hist: Vec<f64> = Vec::new();
    let (mut y0, mut bin) = (f64::NAN, f64::NAN);
    for r in out
        .detectors
        .iter()
        .filter(|r| r.detector_id == id && r.class == class && !r.in_warmup && r.count > 0)
    {
        (y0, bin) = (r.lateral_y0, r.lateral_bin);
        hist.resize(hist.len().max(r.lateral_hist.len()), 0.0);
        for (h, &c) in hist.iter_mut().zip(&r.lateral_hist) {
            *h += f64::from(c);
        }
    }
    (y0, bin, hist)
}

/// Mean and SD of a histogram's bin centres.
fn hist_moments((y0, bin, h): &(f64, f64, Vec<f64>)) -> (f64, f64, f64) {
    let n: f64 = h.iter().sum();
    let centre = |k: usize| y0 + bin * (k as f64 + 0.5);
    let mean = h
        .iter()
        .enumerate()
        .map(|(k, c)| c * centre(k))
        .sum::<f64>()
        / n;
    let var = h
        .iter()
        .enumerate()
        .map(|(k, c)| c * (centre(k) - mean).powi(2))
        .sum::<f64>()
        / n;
    (n, mean, var.sqrt())
}

/// P5.6: lateral positions differ by class, and two-wheelers spread wider
/// than cars (reference corridor, first 900 s, two screenlines).
#[test]
fn lateral_positions_differ_by_class() {
    let yaml = scenario_file("corridor_2km.yaml").replace("duration_s: 3600", "duration_s: 900");
    let spec = spec(&yaml, &[]);
    let mut sim = Simulation::new(&spec, 1, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    for id in ["s1000", "s1900"] {
        let tw = lateral_histogram(&out, id, "two_wheeler");
        let car = lateral_histogram(&out, id, "car");
        let ((n_tw, _, sd_tw), (n_car, _, sd_car)) = (hist_moments(&tw), hist_moments(&car));
        assert!(
            sd_tw > sd_car,
            "{id}: SD two-wheelers {sd_tw:.2} m, cars {sd_car:.2} m"
        );
        // Two-sample Kolmogorov–Smirnov on the binned distributions, 1% level.
        assert_eq!(tw.0, car.0);
        let len = tw.2.len().max(car.2.len());
        let (mut ca, mut cb, mut d) = (0.0, 0.0, 0.0f64);
        for k in 0..len {
            ca += tw.2.get(k).copied().unwrap_or(0.0) / n_tw;
            cb += car.2.get(k).copied().unwrap_or(0.0) / n_car;
            d = d.max((ca - cb).abs());
        }
        let critical = 1.628 * ((n_tw + n_car) / (n_tw * n_car)).sqrt();
        assert!(
            d > critical,
            "{id}: KS distance {d:.3}, critical {critical:.3}"
        );
    }
}

/// P5.6: the barrier reference scenario. Two-wheelers work their way forward
/// through the stopped queue and cars do not: places gained between joining
/// the queue and the opening, pooled over three closures (single closures
/// are noisy: 15–30 two-wheelers each). Every queue discharges before the
/// next closure; nothing collides.
#[test]
fn barrier_queues_are_seeped_and_discharge() {
    use std::collections::BTreeMap;
    let spec = spec(&scenario_file("barrier_queue.yaml"), &[]);
    let mut sim = Simulation::new(&spec, 1, 1).unwrap();
    let class_of = |i: usize, a: &marg_core::agents::AgentStore| {
        marg_params::VehicleClass::ALL[usize::from(a.class[i])].key()
    };
    // Places gained, summed, and vehicles, per class.
    let mut gained: BTreeMap<&str, (f64, f64)> = BTreeMap::new();
    for (close, open, next_close) in [
        (60.0, 180.0, 360.0),
        (360.0, 480.0, 660.0),
        (660.0, 780.0, 960.0),
    ] {
        while sim.time() < close {
            sim.step();
        }
        // When each vehicle first came to a stop in this closure.
        let mut stopped_at: BTreeMap<u64, f64> = BTreeMap::new();
        while sim.time() < open - 0.05 {
            sim.step();
            let a = sim.agents();
            for i in (0..a.len()).filter(|&i| a.v[i] < 0.5) {
                stopped_at.entry(a.id[i]).or_insert(sim.time());
            }
        }
        let a = sim.agents();
        let queue: Vec<usize> = (0..a.len())
            .filter(|&i| a.v[i] < 0.5 && stopped_at.contains_key(&a.id[i]))
            .collect();
        assert!(
            queue.len() >= 30,
            "only {} queued at t = {open}",
            queue.len()
        );
        let mut by_joining = queue.clone();
        by_joining.sort_by(|&x, &y| stopped_at[&a.id[x]].total_cmp(&stopped_at[&a.id[y]]));
        let mut by_position = queue.clone();
        by_position.sort_by(|&x, &y| a.s[y].total_cmp(&a.s[x]));
        let rank = |o: &[usize], i: usize| o.iter().position(|&x| x == i).unwrap() as f64;
        for &i in &queue {
            let g = gained.entry(class_of(i, a)).or_default();
            g.0 += rank(&by_joining, i) - rank(&by_position, i);
            g.1 += 1.0;
        }
        while sim.time() < next_close - 0.05 {
            sim.step();
        }
        let a = sim.agents();
        let waiting = (0..a.len())
            .filter(|&i| a.v[i] < 0.5 && a.s[i] < 600.0)
            .count();
        assert!(waiting <= 3, "{waiting} still waiting at t = {next_close}");
    }
    let gain = |class: &str| gained[class].0 / gained[class].1;
    let (bikes, cars) = (gain("two_wheeler"), gain("car"));
    assert!(
        bikes > 0.5 && cars < 0.0,
        "places gained in the queue: two-wheelers {bikes:.2}, cars {cars:.2}"
    );
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}
