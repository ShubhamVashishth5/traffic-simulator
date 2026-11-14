//! Micro-scenario tests of P7.4–P7.7 and the gridlock indicator: gap
//! decisions, U-turn paths, splits (the deadline, missed branches and the
//! way back through a median opening) and entries fed through a signal.

mod common;
use common::*;
use marg_core::{EventRecord, RunOutputs, Simulation};

fn attr(e: &EventRecord, key: &str) -> f64 {
    let rest = e.attrs.split(&format!("\"{key}\":")).nth(1).unwrap();
    rest[..rest.find([',', '}']).unwrap()].parse().unwrap()
}

fn attr_str<'a>(e: &'a EventRecord, key: &str) -> &'a str {
    let rest = e.attrs.split(&format!("\"{key}\":\"")).nth(1).unwrap();
    &rest[..rest.find('"').unwrap()]
}

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

fn run(yaml: &str, overrides: &[(&str, &str)], seed: u64) -> RunOutputs {
    let mut sim = Simulation::new(&spec(yaml, overrides), seed, 1).unwrap();
    sim.run_to_end();
    sim.finish().unwrap().1
}

fn events<'a>(out: &'a RunOutputs, kind: &str) -> Vec<&'a EventRecord> {
    out.events.iter().filter(|e| e.kind == kind).collect()
}

/// P7.4: a driver takes a gap exactly when it is at least its critical gap
/// (times the heavy-stream factor against a heavy vehicle), and every gap
/// it turns down is logged, shorter than that. These are the accepted and
/// rejected gaps a gap-acceptance survey records (G§2.4).
#[test]
fn gaps_are_taken_by_the_critical_gap_and_every_rejection_is_logged() {
    let yaml = scenario_file("priority_crossing.yaml")
        .replace("duration_s: 1800", "duration_s: 600")
        .replace("warmup_s: 300", "warmup_s: 0");
    let out = run(&yaml, &[], 2);
    let (acc, rej) = (events(&out, "gap_accepted"), events(&out, "gap_rejected"));
    assert!(
        acc.len() > 30 && rej.len() > 30,
        "{} / {}",
        acc.len(),
        rej.len()
    );
    // A gap with no conflicting vehicle in reach is logged as null.
    for e in acc.iter().filter(|e| !e.attrs.contains("\"gap_s\":null")) {
        let (gap, need) = (attr(e, "gap_s"), attr(e, "critical_gap_s"));
        assert!(gap >= need, "accepted {gap} below its critical gap {need}");
    }
    for e in &rej {
        let (gap, need) = (attr(e, "gap_s"), attr(e, "critical_gap_s"));
        assert!(gap < need, "rejected {gap} at or above {need}");
    }
    assert_eq!(kpi(&out, "gaps_rejected") as usize, rej.len());
    // Against a heavy vehicle the gap needed is longer.
    let heavy =
        |e: &&&EventRecord| matches!(attr_str(e, "against_class"), "bus" | "truck" | "tractor");
    if let Some(h) = rej.iter().find(heavy) {
        let light: Vec<_> = rej
            .iter()
            .filter(|e| e.agent_id == h.agent_id && !heavy(e))
            .collect();
        for l in light {
            assert!(attr(h, "critical_gap_s") > attr(l, "critical_gap_s"));
        }
    }
}

/// The median opening with a receiving carriageway `half` m to each side
/// of its reference line (3.5: four-lane divided; 5.25: six-lane).
fn opening(half: f64, flow: u32, duration: u32) -> String {
    let off = half + 0.5;
    scenario_file("median_opening.yaml")
        .replace(
            "y_min: -3.5, y_max: 3.5",
            &format!("y_min: -{half}, y_max: {half}"),
        )
        .replace("-4.0]", &format!("-{off}]"))
        .replace(", 4.0]", &format!(", {off}]"))
        .replace("e: {w: 0.8, e: 0.2}", "e: {w: 0.5, e: 0.5}")
        .replace("w: {e: 0.8, w: 0.2}", "w: {e: 0.5, w: 0.5}")
        .replace("veh_per_h: 900", &format!("veh_per_h: {flow}"))
        .replace("duration_s: 1800", &format!("duration_s: {duration}"))
        .replace("warmup_s: 300", "warmup_s: 0")
}

/// P7.5: where a U-turning vehicle joins the receiving carriageway follows
/// the four-lane path (a gamma) when that carriageway is narrower than six
/// notional lanes' worth, the six-lane mixture (bimodal) when it is not,
/// and the vehicle gets there.
#[test]
fn u_turn_paths_follow_the_road_width_rule() {
    for (half, six) in [(3.5, false), (5.25, true)] {
        let out = run(&opening(half, 500, 900), &[], 1);
        let turns = events(&out, "u_turn");
        assert!(turns.len() > 60, "{} U-turns", turns.len());
        let paths: Vec<f64> = turns
            .iter()
            .map(|e| attr(e, "path_from_median_m"))
            .collect();
        for e in &turns {
            assert_eq!(attr(e, "receiving_width_m"), 2.0 * half);
        }
        let n = paths.len() as f64;
        let mean = paths.iter().sum::<f64>() / n;
        let sd = (paths.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let share =
            |lo: f64, hi: f64| paths.iter().filter(|&&x| x >= lo && x < hi).count() as f64 / n;
        if six {
            // 0.6 Gamma(4, 0.3) + 0.4 N(5.5, 0.8): mean 2.92, a valley
            // between the modes at about 1.2 m and 5.5 m.
            assert!((mean - 2.92).abs() < 0.5, "six-lane mean {mean}");
            let (low, valley, high) = (share(0.0, 2.5), share(3.0, 4.0), share(4.0, 8.0));
            assert!(
                low > 0.4 && high > 0.25 && valley < 0.08,
                "{low} {valley} {high}"
            );
        } else {
            // Gamma(2.5, 0.8): mean 2.0, sd 1.26, one mode.
            assert!((mean - 2.0).abs() < 0.35, "four-lane mean {mean}");
            assert!((sd - 1.26).abs() < 0.35, "four-lane sd {sd}");
            assert!(share(5.0, 20.0) < 0.06);
        }
        assert_eq!(kpi(&out, "collision_guard_events"), 0.0, "half {half}");
    }
}

/// A split where every driver is a late realiser who never learns its
/// branch (no sign, no late awareness) and the stop-and-wait probability.
fn blind_split(p_wait: &str, late: &str) -> (String, Vec<(&'static str, String)>) {
    let yaml = scenario_file("split_corridor.yaml")
        .replace("veh_per_h: 2400", "veh_per_h: 900")
        .replace("duration_s: 1800", "duration_s: 600")
        .replace("warmup_s: 300", "warmup_s: 0");
    (
        yaml,
        vec![
            ("road.split.late_realiser_share", "1".into()),
            (
                "road.split.late_awareness_distance",
                format!("{{dist: constant, value: {late}}}"),
            ),
            ("road.split.sign_visibility_distance", "0".into()),
            ("road.split.stop_and_wait_probability", p_wait.into()),
        ],
    )
}

/// P7.6: a driver that reaches the point where it could last stop before
/// the split without knowing which branch it needs takes the branch it is
/// in, there and then (logged as a missed branch). Before this rule it was
/// held at the line by the carriageway of the branch it was not in.
#[test]
fn a_driver_that_never_learns_its_branch_takes_the_one_it_is_in() {
    let (yaml, ov) = blind_split("0.5", "0");
    let ov: Vec<(&str, &str)> = ov.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let out = run(&yaml, &ov, 1);
    let missed = events(&out, "missed_branch");
    assert!(missed.len() > 20, "{} missed", missed.len());
    assert!(missed.iter().all(|e| e.attrs.contains("\"aware\":false")));
    assert!(events(&out, "split_decision").is_empty());
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    let exited = out.agents.iter().filter(|a| a.exited).count();
    assert!(exited as f64 > 0.85 * out.agents.len() as f64);
}

/// P7.6: a driver that learns its branch just before its deadline and is on
/// the wrong side stops before the line to wait for room, when it is one
/// that stops (probability 1 here), and crosses over once there is room.
#[test]
fn a_late_realiser_on_the_wrong_side_stops_and_waits() {
    let (yaml, ov) = blind_split("1", "60");
    let ov: Vec<(&str, &str)> = ov.iter().map(|(k, v)| (*k, v.as_str())).collect();
    let out = run(&yaml, &ov, 1);
    let waits = events(&out, "split_decision");
    assert!(waits.len() > 10, "{} decisions", waits.len());
    assert!(
        waits
            .iter()
            .all(|e| attr_str(e, "decision") == "stop_and_wait")
    );
    // It waits before the line, on its own side.
    for e in &waits {
        assert!(e.s < 700.0 && e.attrs.contains("\"late_realiser\":true"));
    }
    assert_eq!(kpi(&out, "split_stop_and_wait") as usize, waits.len());
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    // Most of them get over to their branch and out.
    let through = waits
        .iter()
        .filter(|w| {
            out.agents
                .iter()
                .any(|a| a.agent_id == w.agent_id && a.exited)
        })
        .filter(|w| {
            !events(&out, "missed_branch")
                .iter()
                .any(|m| m.agent_id == w.agent_id)
        })
        .count();
    assert!(
        2 * through > waits.len(),
        "{through} of {} waiting got through",
        waits.len()
    );
}

/// P7.6: a driver that takes a branch it did not want turns back at the
/// next median opening (G§6.7): a split whose kerb-side branch leads to an
/// opening through which a driver can U-turn onto the way back.
#[test]
fn a_driver_that_misses_its_branch_turns_back_at_the_next_opening() {
    let yaml = r#"
name: reroute
duration_s: 600
dt_s: 0.1
network:
  ribbons:
    # Left-hand traffic: eastbound on the north carriageway, a 9 m median.
    - {id: main, polyline: [[0, 8], [400, 8]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
    - {id: left, polyline: [[400, 8], [700, 8]], width: [{s: 0, y_min: 0, y_max: 3.5}]}
    - {id: right, polyline: [[400, 8], [500, 6], [600, 5]], width: [{s: 0, y_min: -3.5, y_max: 0}]}
    - {id: back, polyline: [[700, -8], [300, -8]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
    - {id: far_in, polyline: [[1100, -8], [712, -8]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
    - {id: far_out, polyline: [[712, 8], [1100, 8]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
  junctions:
    - id: fork
      kind: split
      arms: [{id: main, in: main}, {id: left, out: left}, {id: right, out: right}]
      turns: {main: {left: 0.5, right: 0.5}}
    - id: opening
      arms: [{id: a, in: left, out: back}, {id: b, in: far_in, out: far_out}]
      turns: {a: {b: 0.99, a: 0.01}, b: {a: 1}}
      control: {kind: uncontrolled, major: [a, b]}
demand:
  entries:
    - {id: west, ribbon: main, flow: [{from_s: 0, veh_per_h: 500}], composition: {hatchback: 0.6, motorcycle: 0.4}}
overrides:
  road.junction.turn_lateral_acceleration: 2.0
  interaction.gap_acceptance.u_turn.critical_gap_factor: 0.8
  interaction.gap_acceptance.u_turn.path.four_lane: {dist: gamma, shape: 2.5, scale: 0.8}
  road.split.stop_and_wait_probability: 0
  road.split.route_weight: 1.0
  road.split.urgency_clearance_link: 0.5
  road.split.late_realiser_share: 1
  road.split.late_awareness_distance: {dist: constant, value: 0}
  road.split.sign_visibility_distance: 0
"#;
    let out = run(yaml, &[], 1);
    // The branch each driver finally took (one that drifts after its
    // deadline can change it again at the line).
    let all_missed = events(&out, "missed_branch");
    let missed: Vec<_> = all_missed
        .iter()
        .enumerate()
        .filter(|(n, e)| {
            attr_str(e, "took") == "left"
                && !all_missed[n + 1..]
                    .iter()
                    .any(|later| later.agent_id == e.agent_id)
        })
        .map(|(_, e)| *e)
        .collect();
    let reroutes = events(&out, "reroute");
    assert!(
        missed.len() >= 8,
        "{} missed onto the left branch",
        missed.len()
    );
    // Every driver that missed onto the branch with an opening turns back
    // there (the opening is the next junction on that branch).
    for m in &missed {
        assert!(
            reroutes
                .iter()
                .any(|r| r.agent_id == m.agent_id && attr_str(r, "via") == "opening"),
            "agent {} did not reroute",
            m.agent_id
        );
    }
    assert_eq!(kpi(&out, "reroutes") as usize, reroutes.len());
    // And makes the U-turn there.
    let turned = events(&out, "u_turn");
    let back = reroutes
        .iter()
        .filter(|r| turned.iter().any(|u| u.agent_id == r.agent_id))
        .count();
    assert!(
        back as f64 > 0.7 * reroutes.len() as f64,
        "{back} of {}",
        reroutes.len()
    );
}

/// Times at which vehicles cross station `at` of the first ribbon.
fn crossings(yaml: &str, at: f64, seed: u64) -> Vec<f64> {
    let mut sim = Simulation::new(&spec(yaml, &[]), seed, 1).unwrap();
    let mut last: std::collections::HashMap<u64, f64> = std::collections::HashMap::new();
    let mut times = Vec::new();
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            let before = last.insert(a.id[i], a.s[i]).unwrap_or(f64::NEG_INFINITY);
            if a.ribbon[i] == 0 && before < at && a.s[i] >= at {
                times.push(sim.time());
            }
        }
    }
    times.sort_by(f64::total_cmp);
    times
}

/// P7.7: downstream of a signal that feeds the entry, headways come in
/// platoons: short ones inside each platoon and one long gap per cycle (the
/// red), instead of the shifted exponential of a renewal process. Both
/// forms of the feed do it: the simulated stop line, and holding arrivals
/// back while the signal is not green.
#[test]
fn an_entry_fed_through_a_signal_arrives_in_platoons() {
    let base = scenario_file("upstream_signal.yaml")
        .replace("duration_s: 1800", "duration_s: 900")
        .replace("warmup_s: 300", "warmup_s: 0");
    let renewal = base.replace(
        "      upstream_signal: {stop_line_s: 200, cycle_s: 90, green_s: 40, amber_s: 3}\n",
        "",
    );
    let gated = base.replace("stop_line_s: 200, ", "");
    assert!(renewal != base && gated != base);
    let stats = |t: &[f64]| {
        let h: Vec<f64> = t
            .windows(2)
            .map(|w| w[1] - w[0])
            .filter(|&x| x > 0.0)
            .collect();
        let n = h.len() as f64;
        let mean = h.iter().sum::<f64>() / n;
        let sd = (h.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
        let long = h.iter().filter(|&&x| x > 25.0).count();
        (sd / mean, long, n as usize)
    };
    // From 120 s: past the first cycle.
    let after = |t: Vec<f64>| t.into_iter().filter(|&x| x > 120.0).collect::<Vec<_>>();
    // 100 m past the simulated stop line, 300 m past the gated entry: slow
    // vehicles let go at the end of green disperse the platoons further on.
    const AT: f64 = 300.0;
    let (cv_r, long_r, n_r) = stats(&after(crossings(&renewal, AT, 1)));
    let (cv_s, long_s, n_s) = stats(&after(crossings(&base, AT, 1)));
    let (cv_g, long_g, n_g) = stats(&after(crossings(&gated, AT, 1)));
    eprintln!(
        "renewal cv {cv_r:.2} long {long_r} n {n_r}; stop line cv {cv_s:.2} long {long_s} n {n_s}; gate cv {cv_g:.2} long {long_g} n {n_g}"
    );
    // A shifted exponential has a coefficient of variation below 1 and
    // almost never a 25 s gap at this flow (mean headway 3.6 s).
    assert!(
        cv_r < 1.0 && long_r <= 1,
        "renewal: cv {cv_r}, {long_r} long of {n_r}"
    );
    // 780 s of 90 s cycles: about 8 reds of 47 s.
    for (cv, long, n) in [(cv_s, long_s, n_s), (cv_g, long_g, n_g)] {
        assert!(cv > 1.3, "platoons: cv {cv}");
        assert!((7..=10).contains(&long), "{long} long gaps of {n}");
    }
    // Nothing is lost: the signal passes the flow on.
    assert!(
        (n_s as f64 - n_r as f64).abs() < 0.12 * n_r as f64,
        "{n_s} vs {n_r}"
    );
}

/// The gridlock indicator: a barrier closed for good on a corridor backs a
/// queue through the area upstream of it; the area is slow from when the
/// queue reaches it, and gridlocked a minute later.
#[test]
fn a_standing_queue_is_reported_as_gridlock() {
    let yaml = r#"
name: queue
duration_s: 600
warmup_s: 0
dt_s: 0.1
network:
  ribbons:
    - {id: main, polyline: [[0, 0], [600, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}
  barriers:
    - {id: wall, ribbon: main, s: 500, closed: [{from_s: 60, to_s: 10000}]}
demand:
  entries:
    - {id: west, ribbon: main, flow: [{from_s: 0, veh_per_h: 1500}], composition: {hatchback: 1}}
detectors:
  - {kind: area, id: before_wall, ribbon: main, s_from: 440, s_to: 495, interval_s: 60}
  - {kind: area, id: far_upstream, ribbon: main, s_from: 50, s_to: 100, interval_s: 60}
"#;
    let out = run(yaml, &[], 1);
    let onsets = events(&out, "gridlock");
    assert!(!onsets.is_empty());
    let first = onsets[0];
    assert!(first.attrs.contains("\"area\":\"before_wall\""));
    let since = attr(first, "slow_since");
    // The queue reaches the area after the barrier closes; the gridlock is
    // declared once it has been slow for the register's 60 s.
    assert!(since > 60.0 && since < 200.0, "slow since {since}");
    assert!(
        (first.t - since - 60.0).abs() < 0.11,
        "{} vs {since}",
        first.t
    );
    let onset = kpi(&out, "gridlock_onset_s");
    assert_eq!(onset, since);
    let share = kpi(&out, "gridlock_share");
    assert!(share > 0.4 && share < 0.85, "share {share}");
}
