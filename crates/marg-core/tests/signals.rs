//! Violations at signals and signal control states (plan P8.4, P8.7,
//! ADR-0019): red-light running, early start, amber go and box blocking;
//! fixed, dark, blinker and police controllers, the outage chain and the
//! live toggle.

mod common;
use common::*;
use marg_core::{EventRecord, RunOutputs, Simulation};

/// The rule's shared inputs for these tests (the guide has no numbers:
/// assumed, as in `tests/violations.rs`).
const SHARED: &[(&str, &str)] = &[
    ("violations.coefficients.aggressiveness", "0.5"),
    ("violations.coefficients.time_saved", "0.02"),
    ("violations.coefficients.waiting_time", "0.03"),
    ("violations.coefficients.imitation_violators", "0.5"),
    ("violations.coefficients.imitation_compliers", "0.2"),
    ("violations.coefficients.enforcement", "1.0"),
    ("violations.coefficients.risk", "3.0"),
];

const RED: (&str, &str) = ("violations.red_light.intercept", "-6.0");
const EARLY: (&str, &str) = ("violations.early_start.intercept", "-2.0");
const AMBER: (&str, &str) = ("violations.amber_go.intercept", "0.0");
const BOX: (&str, &str) = ("violations.box_blocking.intercept", "-3.0");
const FORCED: (&str, &str) = ("violations.forced_entry.intercept", "-5.0");

const OFFICER: &[(&str, &str)] = &[
    ("road.signal.officer.cycle_length", "120"),
    ("road.signal.officer.major_bias", "0.5"),
];

fn with<'a>(types: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v = SHARED.to_vec();
    v.extend_from_slice(types);
    v
}

/// The signal crossing at `flow` veh/h per arm for `duration` s, with its
/// countdown display in `countdown` and `control` added to its control.
fn signal(flow: f64, duration: f64, countdown: &str, control: &str) -> String {
    scenario_file("signal_crossing.yaml")
        .replace("veh_per_h: 700", &format!("veh_per_h: {flow}"))
        .replace("duration_s: 1800", &format!("duration_s: {duration}"))
        .replace("warmup_s: 300", "warmup_s: 0")
        .replace(
            "        countdown: working\n",
            &format!("        countdown: {countdown}\n{control}"),
        )
}

fn run(yaml: &str, overrides: &[(&str, &str)], seed: u64) -> RunOutputs {
    let mut sim = Simulation::new(&spec(yaml, overrides), seed, 1).unwrap();
    sim.run_to_end();
    sim.finish().unwrap().1
}

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

/// The JSON object under `key` in an event's attributes, as `(name, value)`.
fn object(e: &EventRecord, key: &str) -> Vec<(String, f64)> {
    let start = e.attrs.find(&format!("\"{key}\":{{")).unwrap() + key.len() + 4;
    let end = start + e.attrs[start..].find('}').unwrap();
    e.attrs[start..end]
        .split(',')
        .map(|kv| {
            let (k, v) = kv.split_once(':').unwrap();
            (k.trim_matches('"').to_string(), v.parse().unwrap())
        })
        .collect()
}

fn feature(e: &EventRecord, name: &str) -> f64 {
    object(e, "features")
        .into_iter()
        .find(|(n, _)| n == name)
        .unwrap()
        .1
}

fn number(e: &EventRecord, key: &str) -> f64 {
    let rest = e.attrs.split(&format!("\"{key}\":")).nth(1).unwrap();
    let end = rest.find([',', '}']).unwrap();
    rest[..end].parse().unwrap()
}

fn text<'a>(e: &'a EventRecord, key: &str) -> Option<&'a str> {
    let rest = e.attrs.split(&format!("\"{key}\":\"")).nth(1)?;
    Some(&rest[..rest.find('"').unwrap()])
}

fn decisions<'a>(out: &'a RunOutputs, kind: &str) -> Vec<&'a EventRecord> {
    out.events
        .iter()
        .filter(|e| {
            (e.kind == "violation" || e.kind == "complied") && text(e, "type") == Some(kind)
        })
        .collect()
}

/// The logged terms of a continuous decision add up to its `η`, its hazard
/// is `λ_max σ(η)`, and it acted exactly when `Λ ≥ E` (ADR-0018).
fn check_hazard(e: &EventRecord) {
    let eta = number(e, "eta");
    let sum: f64 = object(e, "terms").iter().map(|(_, v)| v).sum();
    assert!((sum - eta).abs() < 1e-9, "{}", e.attrs);
    let hazard = number(e, "max_rate") / (1.0 + (-eta).exp());
    assert!((number(e, "hazard") - hazard).abs() < 1e-12, "{}", e.attrs);
    let fired = number(e, "integrated") >= number(e, "threshold");
    assert_eq!(fired, e.kind == "violation", "{}", e.attrs);
    let risk = feature(e, "risk");
    assert!((0.0..=1.0).contains(&risk), "{}", e.attrs);
}

/// Red-light running (P8.4): only a vehicle at the front of a red line
/// decides; one that acts crosses its line, mostly while it is still red;
/// without the rule nobody crosses on red but the few that could not stop
/// at amber. Each decision's terms add up.
#[test]
fn red_light_runners_cross_from_the_front_of_a_red_line() {
    let yaml = signal(400.0, 900.0, "none", "");
    let base = run(&yaml, &[], 1);
    let out = run(&yaml, &with(&[RED]), 1);
    let ds = decisions(&out, "red_light");
    let acts: Vec<_> = ds.iter().filter(|e| e.kind == "violation").collect();
    assert!(acts.len() >= 5, "{} red-light runs", acts.len());
    assert!(ds.len() > acts.len(), "every decision acted");
    assert_eq!(kpi(&out, "red_light_runs"), acts.len() as f64);
    for e in &ds {
        check_hazard(e);
        assert!(feature(e, "time_saved") >= 0.0, "{}", e.attrs);
        assert_eq!(feature(e, "enforcement"), 0.0);
    }
    let crossings = |o: &RunOutputs, act: Option<&str>| -> Vec<(f64, f64)> {
        o.events
            .iter()
            .filter(|e| e.kind == "stop_line_crossed" && text(e, "act") == act)
            .map(|e| (number(e, "light"), e.t))
            .collect()
    };
    let runs = crossings(&out, Some("red_light"));
    assert!(
        runs.len() + 2 >= acts.len(),
        "{} of {}",
        runs.len(),
        acts.len()
    );
    let on_red = runs.iter().filter(|c| c.0 == 3.0).count();
    assert!(
        on_red as f64 >= 0.8 * runs.len() as f64,
        "{on_red} of {} on red",
        runs.len()
    );
    // Crossing on red without the rule: only vehicles let through at amber.
    let base_red = base
        .events
        .iter()
        .filter(|e| e.kind == "stop_line_crossed" && number(e, "light") == 3.0)
        .count();
    assert!(crossings(&base, None).iter().all(|c| c.0 != 3.0) || base_red < runs.len());
    assert!(base.events.iter().all(|e| e.kind != "violation"));
    // The cycle table counts the red crossings by third of the red.
    let binned: u32 = out
        .signal_cycles
        .iter()
        .map(|c| c.red_crossings_early + c.red_crossings_middle + c.red_crossings_late)
        .sum();
    assert!(binned as usize >= on_red / 2, "{binned} binned of {on_red}");
}

/// Early start (P8.4): only with a working countdown, standing at the
/// front, in the change interval before its green (amber 3 s and all-red
/// 2 s here); never with the countdown dark or missing.
#[test]
fn early_start_needs_a_working_countdown_and_the_end_of_red() {
    let out = run(&signal(400.0, 900.0, "working", ""), &with(&[EARLY]), 2);
    let ds = decisions(&out, "early_start");
    let acts = ds.iter().filter(|e| e.kind == "violation").count();
    assert!(acts >= 3, "{acts} early starts");
    for e in &ds {
        check_hazard(e);
        let left = feature(e, "time_saved");
        assert!((0.0..=5.0 + 1e-6).contains(&left), "{}", e.attrs);
    }
    for countdown in ["none", "broken"] {
        let out = run(&signal(400.0, 600.0, countdown, ""), &with(&[EARLY]), 2);
        assert!(decisions(&out, "early_start").is_empty(), "{countdown}");
    }
}

/// Amber go (P8.4): one decision per vehicle in the option zone at amber
/// onset, `P = σ(η)` against one draw; those that go may cross.
#[test]
fn amber_go_is_decided_once_at_amber_onset() {
    let out = run(&signal(500.0, 900.0, "none", ""), &with(&[AMBER]), 3);
    let ds = decisions(&out, "amber_go");
    let go = ds.iter().filter(|e| e.kind == "violation").count();
    assert!(go >= 3 && ds.len() > go, "{go} of {}", ds.len());
    let ambers: Vec<f64> = out
        .events
        .iter()
        .filter(|e| e.kind == "signal_state" && text(e, "state") == Some("amber"))
        .map(|e| e.t)
        .collect();
    for e in &ds {
        let eta = number(e, "eta");
        let sum: f64 = object(e, "terms").iter().map(|(_, v)| v).sum();
        assert!((sum - eta).abs() < 1e-9);
        let p = 1.0 / (1.0 + (-eta).exp());
        assert!((number(e, "probability") - p).abs() < 1e-12);
        assert_eq!(number(e, "draw") < p, e.kind == "violation", "{}", e.attrs);
        let risk = feature(e, "risk");
        assert!(risk > 0.0 && risk <= 1.0, "{}", e.attrs);
        assert!(ambers.contains(&e.t), "decided at {} off amber onset", e.t);
    }
}

/// Box blocking (P8.4): with the way out of the junction blocked (a closed
/// barrier just past it), vehicles held at a green line may enter anyway.
#[test]
fn box_blocking_enters_a_junction_whose_way_out_is_blocked() {
    let yaml = signal(400.0, 900.0, "none", "").replace(
        "  junctions:\n",
        "  barriers:\n    - {id: jam, ribbon: e_out, s: 25, closed: [{from_s: 120, to_s: 100000}]}\n  junctions:\n",
    );
    let base = run(&yaml, &[], 4);
    assert_eq!(kpi(&base, "box_blocks"), 0.0);
    let out = run(&yaml, &with(&[BOX]), 4);
    let ds = decisions(&out, "box_blocking");
    let acts = ds.iter().filter(|e| e.kind == "violation").count();
    assert!(acts >= 1, "{acts} box blocks of {} decisions", ds.len());
    for e in &ds {
        check_hazard(e);
        assert!(feature(e, "waiting_time") >= 0.0);
        assert_eq!(feature(e, "time_saved"), 0.0);
    }
}

/// G§4.4 regression (P8.4 done-when): filtering, staggered following and
/// moving between notional lanes are physics. With every built violation
/// type switched on, a dense mixed corridor (no junction) has lateral moves
/// and not one violation decision.
#[test]
fn baseline_filtering_is_never_a_violation() {
    let yaml = corridor(
        600.0,
        300.0,
        3000.0,
        "{motorcycle: 0.4, scooter: 0.2, auto_rickshaw: 0.15, hatchback: 0.2, bus: 0.05}",
    );
    let out = run(&yaml, &with(&[RED, EARLY, AMBER, BOX, FORCED]), 5);
    assert!(kpi(&out, "lateral_moves") > 100.0);
    assert!(
        out.events
            .iter()
            .all(|e| e.kind != "violation" && e.kind != "complied")
    );
    assert_eq!(kpi(&out, "violations_per_1000_veh"), 0.0);
}

/// P8.7 done-when: switching to dark moves the junction into the
/// uncontrolled regime (no lights, every movement waits for a gap, forced
/// entry applies); events and `signal_cycles` record the state.
#[test]
fn a_dark_signal_is_an_uncontrolled_junction() {
    let yaml = signal(
        400.0,
        900.0,
        "working",
        "        schedule: [{at_s: 300, state: dark}]\n",
    );
    let out = run(&yaml, &with(&[FORCED]), 6);
    let changes: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "control_state")
        .collect();
    assert_eq!(changes.len(), 1);
    let c = changes[0];
    assert!((c.t - 300.0).abs() < 0.11, "{}", c.t);
    assert_eq!(
        (text(c, "from"), text(c, "to"), text(c, "cause")),
        (Some("fixed"), Some("dark"), Some("schedule"))
    );
    assert!(
        out.events
            .iter()
            .all(|e| e.kind != "signal_state" || e.t < 300.0)
    );
    let crossed: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "stop_line_crossed" && e.t > 301.0)
        .collect();
    assert!(crossed.len() > 50, "{} crossed while dark", crossed.len());
    assert!(
        crossed
            .iter()
            .all(|e| number(e, "light") == 0.0 && text(e, "control") == Some("dark"))
    );
    // Forced entry belongs to the uncontrolled regime only.
    let forced = decisions(&out, "forced_entry");
    assert!(!forced.is_empty() && forced.iter().all(|e| e.t > 300.0));
    // Gaps are read on every approach once it is dark.
    let gaps_dark = out
        .events
        .iter()
        .filter(|e| e.kind == "gap_rejected" && e.t > 300.0)
        .map(|e| e.ribbon.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(gaps_dark.len() >= 3, "{gaps_dark:?}");
    // The cycles measured are the fixed plan's, all before the outage.
    assert!(!out.signal_cycles.is_empty());
    assert!(
        out.signal_cycles
            .iter()
            .all(|c| c.t_green < 300.0 && c.control == "fixed" && c.countdown == "working")
    );
}

/// P8.7: an officer runs the plan's stages over a long cycle, with a share
/// of the minor stages' green moved to the major one (the stage with the
/// longest green: e and w here). Police control without the officer's
/// parameters is refused.
#[test]
fn police_control_runs_a_long_cycle_biased_to_the_major_approach() {
    let yaml = signal(300.0, 600.0, "working", "        state: police\n");
    let err = Simulation::new(&spec(&yaml, &[]), 1, 1)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("road.signal.officer.cycle_length"), "{err}");
    let out = run(&yaml, OFFICER, 7);
    assert!(out.signal_cycles.iter().all(|c| c.control == "police"));
    // Green 110 s of 120: the major share 35/60 + 0.5 (1 - 35/60).
    let major = 110.0 * (35.0 / 60.0 + 0.5 * (25.0 / 60.0));
    for c in &out.signal_cycles {
        let want = if c.arm == "e" || c.arm == "w" {
            major
        } else {
            110.0 - major
        };
        assert!((c.green_s - want).abs() < 1e-9, "{} {}", c.arm, c.green_s);
    }
    assert!(kpi(&out, "exited") > 50.0);
}

/// P8.7: a blinker with major arms is a priority junction (the minor arms
/// wait for gaps), switched on by the night schedule from the scenario's
/// clock.
#[test]
fn the_night_blinker_gives_priority_to_its_major_arms() {
    let yaml = format!(
        "start_time_h: 22.95\n{}",
        signal(
            300.0,
            600.0,
            "none",
            "        night_blinker: true\n        blinker_major: [e, w]\n",
        )
    );
    let out = run(&yaml, &[], 8);
    let on: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "control_state")
        .collect();
    assert_eq!(on.len(), 1);
    assert_eq!(text(on[0], "cause"), Some("blinker_on"));
    assert!((on[0].t - 180.0).abs() < 0.11, "{}", on[0].t);
    let minor_gaps = out
        .events
        .iter()
        .filter(|e| {
            e.kind == "gap_rejected" && e.t > 180.0 && (e.ribbon == "n_in" || e.ribbon == "s_in")
        })
        .count();
    assert!(minor_gaps > 0);
    // Without the clock the schedule cannot run.
    let no_clock = yaml.replace("start_time_h: 22.95\n", "");
    let err = Simulation::new(&spec(&no_clock, &[]), 1, 1)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("start_time_h"), "{err}");
}

/// P8.7: the outage chain takes a junction dark, an officer arrives after
/// the register's delay, and the signal comes back when the outage ends.
#[test]
fn an_outage_goes_dark_then_police_then_back_to_the_plan() {
    let yaml = signal(60.0, 7_200.0, "none", "");
    let mut overrides = OFFICER.to_vec();
    overrides.extend_from_slice(&[
        ("road.signal.outage_probability", "1.0"),
        (
            "road.signal.outage_duration",
            "{dist: log_normal_moments, mean: 0.1, sd: 0.02}",
        ),
        ("road.signal.officer.arrival_delay", "2"),
    ]);
    let out = run(&yaml, &overrides, 9);
    let causes: Vec<(f64, &str)> = out
        .events
        .iter()
        .filter(|e| e.kind == "control_state")
        .map(|e| (e.t, text(e, "cause").unwrap()))
        .collect();
    assert!(causes.len() >= 3, "{causes:?}");
    // Every outage runs dark → police → fixed, in that order.
    for w in causes.chunks(3) {
        if w.len() < 3 {
            break;
        }
        assert_eq!([w[0].1, w[1].1, w[2].1], ["outage", "officer", "repaired"]);
        assert!((w[1].0 - w[0].0 - 120.0).abs() < 0.11, "{w:?}");
    }
    assert!(out.signal_cycles.iter().any(|c| c.control == "police"));
    // The same draws at every thread count.
    let mut one = Simulation::new(&spec(&yaml, &overrides), 9, 1).unwrap();
    let mut four = Simulation::new(&spec(&yaml, &overrides), 9, 4).unwrap();
    assert_eq!(one.run_to_end().output_hash, four.run_to_end().output_hash);
}

/// P8.7: the viewer's live toggle takes effect at the next step and stays
/// until the next change; an unknown state or junction is refused.
#[test]
fn a_live_toggle_switches_the_signal_at_the_next_step() {
    let yaml = signal(300.0, 300.0, "none", "");
    let mut sim = Simulation::new(&spec(&yaml, &[]), 10, 1).unwrap();
    while sim.time() < 100.0 {
        sim.step();
    }
    assert!(sim.set_control("crossing", "sideways").is_err());
    assert!(sim.set_control("nowhere", "dark").is_err());
    sim.set_control("crossing", "dark").unwrap();
    let t = sim.time();
    sim.run_to_end();
    let out = sim.finish().unwrap().1;
    let c: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "control_state")
        .collect();
    assert_eq!(c.len(), 1);
    assert_eq!(text(c[0], "cause"), Some("live"));
    assert!(c[0].t >= t && c[0].t <= t + 0.11, "{} after {t}", c[0].t);
}

/// The cooperative twin (`violations.enabled = 0`, ADR-0007) starts none of
/// the signal rules: the same output as a run without them. Every rule on
/// gives the same output at 1 and 4 threads (ADR-0003).
#[test]
fn signal_rules_honour_the_twin_and_are_deterministic() {
    let yaml = signal(
        500.0,
        600.0,
        "working",
        "        schedule: [{at_s: 400, state: dark}]\n",
    );
    let all = with(&[RED, EARLY, AMBER, BOX, FORCED]);
    let mut off = all.clone();
    off.push(("violations.enabled", "0"));
    let mut a = Simulation::new(&spec(&yaml, &off), 11, 1).unwrap();
    let mut b = Simulation::new(&spec(&yaml, &[]), 11, 1).unwrap();
    assert_eq!(a.run_to_end().output_hash, b.run_to_end().output_hash);
    let mut one = Simulation::new(&spec(&yaml, &all), 11, 1).unwrap();
    let mut four = Simulation::new(&spec(&yaml, &all), 11, 4).unwrap();
    let h1 = one.run_to_end().output_hash;
    assert_eq!(h1, four.run_to_end().output_hash);
    let out = one.finish().unwrap().1;
    assert!(kpi(&out, "red_light_runs") + kpi(&out, "early_starts") > 0.0);
}
