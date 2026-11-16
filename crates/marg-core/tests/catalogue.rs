//! The rest of the violation catalogue (P8.4, ADR-0020) and the rules the
//! user chose on 2026-11-16: stop-line seepage, late merging, footpath
//! riding, opposite-carriageway encroachment, wrong-way riding,
//! stop-anywhere by thinning; first come first served at dark junctions in
//! the cooperative twin (ADR-0019 D1); ADR-0017 A2, B2, C2.

mod common;
use common::*;
use marg_core::agents::{STRIP_FOOTPATH, STRIP_NONE, STRIP_OPPOSITE};
use marg_core::{EventRecord, RunOutputs, Simulation};

/// The rule's shared inputs (the guide has no numbers: assumed, as in
/// `tests/signals.rs`).
const SHARED: &[(&str, &str)] = &[
    ("violations.coefficients.aggressiveness", "0.5"),
    ("violations.coefficients.time_saved", "0.02"),
    ("violations.coefficients.waiting_time", "0.03"),
    ("violations.coefficients.imitation_violators", "0.5"),
    ("violations.coefficients.imitation_compliers", "0.2"),
    ("violations.coefficients.enforcement", "1.0"),
    ("violations.coefficients.risk", "3.0"),
];

const SEEP: (&str, &str) = ("violations.stopline_seepage.intercept", "-2.0");
const LATE: (&str, &str) = ("violations.late_merge.intercept", "-1.0");
const FOOT: (&str, &str) = ("violations.footpath.intercept", "-6.0");
const OPP: (&str, &str) = ("violations.opposite_encroachment.intercept", "-6.0");
const WRONG: (&str, &str) = ("violations.wrong_way.intercept", "-1.0");
const STOP: (&str, &str) = ("violations.stop_anywhere.intercept", "15.0");

fn with<'a>(types: &[(&'a str, &'a str)]) -> Vec<(&'a str, &'a str)> {
    let mut v = SHARED.to_vec();
    v.extend_from_slice(types);
    v
}

fn run(yaml: &str, overrides: &[(&str, &str)], seed: u64) -> RunOutputs {
    let mut sim = Simulation::new(&spec(yaml, overrides), seed, 1).unwrap();
    sim.run_to_end();
    sim.finish().unwrap().1
}

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
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

fn flag(e: &EventRecord, key: &str) -> bool {
    e.attrs.contains(&format!("\"{key}\":true"))
}

fn decisions<'a>(out: &'a RunOutputs, kind: &str) -> Vec<&'a EventRecord> {
    out.events
        .iter()
        .filter(|e| {
            (e.kind == "violation" || e.kind == "complied") && text(e, "type") == Some(kind)
        })
        .collect()
}

fn acts<'a>(out: &'a RunOutputs, kind: &str) -> Vec<&'a EventRecord> {
    decisions(out, kind)
        .into_iter()
        .filter(|e| e.kind == "violation")
        .collect()
}

/// The logged terms of a decision add up to its `η`.
fn check_terms(e: &EventRecord) {
    let eta = number(e, "eta");
    let start = e.attrs.find("\"terms\":{").unwrap() + 9;
    let end = start + e.attrs[start..].find('}').unwrap();
    let sum: f64 = e.attrs[start..end]
        .split(',')
        .map(|kv| kv.split_once(':').unwrap().1.parse::<f64>().unwrap())
        .sum();
    assert!((sum - eta).abs() < 1e-9, "{}", e.attrs);
}

/// The signal crossing at `flow` veh/h per arm for `duration` s, with
/// `control` added to its control.
fn signal(flow: f64, duration: f64, control: &str) -> String {
    scenario_file("signal_crossing.yaml")
        .replace("veh_per_h: 700", &format!("veh_per_h: {flow}"))
        .replace("duration_s: 1800", &format!("duration_s: {duration}"))
        .replace("warmup_s: 300", "warmup_s: 0")
        .replace(
            "        countdown: working\n",
            &format!("        countdown: working\n{control}"),
        )
}

fn undivided(duration: f64) -> String {
    scenario_file("undivided_level_crossing.yaml")
        .replace("duration_s: 900", &format!("duration_s: {duration}"))
}

// ------------------------------------------------------------------ seepage

/// Stop-line seepage (ADR-0020 part 5): only a vehicle standing at the front
/// of a red line decides; one that acts creeps over its line on red and is
/// not counted as running the red.
#[test]
fn seepage_creeps_past_a_red_line_and_is_not_a_red_light_run() {
    let yaml = signal(300.0, 600.0, "");
    let out = run(&yaml, &with(&[SEEP]), 2);
    let all = decisions(&out, "stopline_seepage");
    let took = acts(&out, "stopline_seepage");
    assert!(!took.is_empty(), "no vehicle crept past its line");
    for e in &all {
        check_terms(e);
        // At the front of a line: on an arriving ribbon, within a few
        // metres of its end (the ribbons are 300 m long).
        assert!(e.ribbon.ends_with("_in"), "{}", e.ribbon);
        assert!(e.s > 280.0, "{} at {}", e.ribbon, e.s);
    }
    let crossed: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "stop_line_crossed" && text(e, "act") == Some("stopline_seepage"))
        .collect();
    // Most cross on red; one that decided just before its green crosses on it.
    let on_red = crossed
        .iter()
        .filter(|e| e.attrs.contains("into_red_s"))
        .count();
    assert!(2 * on_red > crossed.len(), "{on_red} of {}", crossed.len());
    // Nobody ran the red: no red crossing is counted by third of the red.
    let red: u32 = out
        .signal_cycles
        .iter()
        .map(|c| c.red_crossings_early + c.red_crossings_middle + c.red_crossings_late)
        .sum();
    assert_eq!(red, 0);
    assert_eq!(kpi(&out, "stopline_seepages"), took.len() as f64);
    // Off: nobody creeps over a red line on purpose.
    let off = run(&yaml, &[], 2);
    assert!(decisions(&off, "stopline_seepage").is_empty());
}

// ------------------------------------------------------ dark junction twin

/// ADR-0019 D1: in the cooperative twin a dark junction takes turns at its
/// lines, first come first served. No gap is read there, and more traffic
/// gets through than when every movement waits for a gap in every
/// conflicting stream.
#[test]
fn the_twin_takes_turns_at_a_dark_junction() {
    let yaml = signal(300.0, 900.0, "        state: dark\n");
    let mutual = run(&yaml, &[("violations.enabled", "0")], 4);
    let turns = run(
        &yaml,
        &[
            ("violations.enabled", "0"),
            ("road.signal.dark_first_come", "1"),
        ],
        4,
    );
    let gaps = |o: &RunOutputs| {
        o.events
            .iter()
            .filter(|e| e.kind == "gap_rejected" || e.kind == "gap_accepted")
            .count()
    };
    assert!(gaps(&mutual) > 0);
    assert_eq!(gaps(&turns), 0);
    assert!(
        kpi(&turns, "exited") > kpi(&mutual, "exited"),
        "first come first served {} vs mutual deference {}",
        kpi(&turns, "exited"),
        kpi(&mutual, "exited")
    );
    // A fixed signal does not read the switch.
    let fixed = signal(300.0, 300.0, "");
    let mut a = Simulation::new(&spec(&fixed, &[]), 4, 1).unwrap();
    let mut b =
        Simulation::new(&spec(&fixed, &[("road.signal.dark_first_come", "1")]), 4, 1).unwrap();
    assert_eq!(a.run_to_end().output_hash, b.run_to_end().output_hash);
}

// ------------------------------------------------------------------- splits

fn split(flow: f64, duration: f64) -> String {
    scenario_file("split_corridor.yaml")
        .replace("duration_s: 1800", &format!("duration_s: {duration}"))
        .replace("veh_per_h: 2400", &format!("veh_per_h: {flow}"))
}

/// ADR-0017 B2: a driver that stopped to wait at its deadline gives up
/// after standing `road.split.wait_patience`.
#[test]
fn patience_ends_a_stop_and_wait() {
    let yaml = split(1000.0, 600.0);
    let out = run(&yaml, &[("road.split.wait_patience", "20")], 1);
    let gave_up: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "split_decision" && text(e, "decision") == Some("give_up_after_wait"))
        .collect();
    assert!(!gave_up.is_empty());
    for e in &gave_up {
        let waited = number(e, "waited_s");
        assert!((20.0..20.0 + 0.2).contains(&waited), "{}", e.attrs);
    }
    let never = run(&yaml, &[], 1);
    assert!(
        never
            .events
            .iter()
            .all(|e| text(e, "decision") != Some("give_up_after_wait"))
    );
}

/// ADR-0017 A2: drivers who know their branch move over from the moment
/// they know, so fewer reach their deadline on the wrong side.
#[test]
fn base_urgency_moves_drivers_over_early() {
    let yaml = split(800.0, 600.0);
    let late = [("road.split.late_realiser_share", "0")];
    let a1 = run(&yaml, &late, 2);
    let mut a2_set = late.to_vec();
    a2_set.push(("road.split.base_urgency", "0.3"));
    let a2 = run(&yaml, &a2_set, 2);
    let wrong_side = |o: &RunOutputs| kpi(o, "split_stop_and_wait") + kpi(o, "split_give_up");
    assert!(
        wrong_side(&a2) < wrong_side(&a1),
        "A2 {} vs A1 {}",
        wrong_side(&a2),
        wrong_side(&a1)
    );
}

/// ADR-0017 C2: a late realiser that learns its branch after taking the one
/// it was in, while it can still stop at the line, decides as at a
/// deadline.
#[test]
fn a_late_realiser_who_learns_its_branch_at_the_line_decides_again() {
    let yaml = split(600.0, 900.0);
    let out = run(
        &yaml,
        &[
            ("road.split.late_realiser_share", "1"),
            (
                "road.split.late_awareness_distance",
                "{dist: uniform, min: 2, max: 40}",
            ),
            ("road.split.sign_visibility_distance", "10"),
        ],
        3,
    );
    let realised: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "split_decision" && flag(e, "realised_after_deadline"))
        .collect();
    assert!(
        !realised.is_empty(),
        "no late realiser learnt its branch in time to stop"
    );
    assert!(realised.iter().all(|e| {
        let d = text(e, "decision");
        d == Some("stop_and_wait") || d == Some("give_up")
    }));
}

/// `late_merge` (ADR-0020 part 6): decided before the deadline by a driver
/// outside its branch's part of the road while that part queues; one that
/// acts waits at its deadline to cut in.
#[test]
fn late_mergers_wait_at_their_deadline_to_cut_in() {
    let yaml = split(1000.0, 600.0);
    let out = run(&yaml, &with(&[LATE, ("road.split.wait_patience", "30")]), 1);
    let all = decisions(&out, "late_merge");
    assert!(!all.is_empty());
    all.iter().for_each(|e| check_terms(e));
    let took = acts(&out, "late_merge");
    assert!(!took.is_empty());
    let at_deadline: Vec<_> = out
        .events
        .iter()
        .filter(|e| {
            e.kind == "split_decision"
                && flag(e, "late_merge")
                && text(e, "decision") != Some("give_up_after_wait")
        })
        .collect();
    assert!(!at_deadline.is_empty());
    assert!(
        at_deadline
            .iter()
            .all(|e| text(e, "decision") == Some("stop_and_wait"))
    );
    assert_eq!(kpi(&out, "late_merges"), took.len() as f64);
}

// ------------------------------------------------------------- stop anywhere

/// ADR-0020 part 7: with the intercept set, each opportunity is taken with
/// probability σ(η). Taken every time (σ ≈ 1) it gives the same stops as
/// with the intercept unset; never taken, none; in the twin, none.
#[test]
fn stop_anywhere_opportunities_are_thinned_by_the_rule() {
    let yaml = scenario_file("stop_anywhere_metro_exit.yaml")
        .replace("duration_s: 1800", "duration_s: 600");
    let stops = |o: &RunOutputs| {
        o.events
            .iter()
            .filter(|e| e.kind == "stop_anywhere")
            .count()
    };
    let unset = run(&yaml, &[], 1);
    assert!(stops(&unset) > 0);
    assert!(decisions(&unset, "stop_anywhere").is_empty());
    let always = run(&yaml, &with(&[STOP]), 1);
    assert_eq!(stops(&always), stops(&unset));
    assert_eq!(acts(&always, "stop_anywhere").len(), stops(&always));
    let never = run(
        &yaml,
        &with(&[("violations.stop_anywhere.intercept", "-15")]),
        1,
    );
    assert_eq!(stops(&never), 0);
    assert!(decisions(&never, "stop_anywhere").len() >= stops(&unset));
    assert!(
        decisions(&never, "stop_anywhere")
            .iter()
            .all(|e| text(e, "reason") == Some("declined"))
    );
    let mut twin = with(&[STOP]);
    twin.push(("violations.enabled", "0"));
    assert_eq!(stops(&run(&yaml, &twin, 1)), 0);
}

// ------------------------------------------------ footpaths, undivided roads

/// What the agents did on the undivided road, step by step: the deepest any
/// was on the footpath and across the centreline with and without an act.
struct Seen {
    footpath_used: bool,
    across_used: bool,
    off_road_without_act: bool,
    past_footpath: bool,
    past_far_kerb: bool,
    on_bollards: bool,
    too_wide: bool,
}

fn watch(yaml: &str, overrides: &[(&str, &str)], seed: u64) -> (Seen, RunOutputs) {
    let mut sim = Simulation::new(&spec(yaml, overrides), seed, 1).unwrap();
    let east = sim.network().index("east").unwrap();
    let mut seen = Seen {
        footpath_used: false,
        across_used: false,
        off_road_without_act: false,
        past_footpath: false,
        past_far_kerb: false,
        on_bollards: false,
        too_wide: false,
    };
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            if a.ribbon[i] as usize != east {
                continue;
            }
            let half = 0.5 * a.width[i];
            let (lo, hi) = (a.y[i] - half, a.y[i] + half);
            let on_footpath = hi > 3.5 + 1e-6;
            let across = lo < -1e-6;
            if (on_footpath || across) && a.strip[i] == STRIP_NONE {
                seen.off_road_without_act = true;
            }
            if on_footpath {
                seen.footpath_used |= a.strip[i] == STRIP_FOOTPATH;
                seen.past_footpath |= hi > 3.5 + 1.5 + 1e-6;
                seen.on_bollards |= a.s[i] > 150.0 && a.s[i] < 250.0;
                seen.too_wide |= a.width[i] > 1.5;
            }
            if across {
                seen.across_used |= a.strip[i] == STRIP_OPPOSITE;
                seen.past_far_kerb |= lo < -3.5 - 1e-6;
            }
        }
    }
    (seen, sim.finish().unwrap().1)
}

/// ADR-0020 parts 1–2: a queued vehicle beside a footpath without bollards
/// may ride it, only if it fits on it; it never goes beyond it or onto the
/// stretch with bollards, and nobody leaves the carriageway without acting.
#[test]
fn footpath_riders_fit_on_it_and_keep_off_the_bollards() {
    let yaml = undivided(450.0);
    let (seen, out) = watch(&yaml, &with(&[FOOT]), 2);
    let took = acts(&out, "footpath");
    assert!(!took.is_empty());
    decisions(&out, "footpath")
        .iter()
        .for_each(|e| check_terms(e));
    assert!(seen.footpath_used);
    assert!(!seen.off_road_without_act);
    assert!(!seen.past_footpath);
    assert!(!seen.on_bollards);
    assert!(!seen.too_wide);
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    // Bollards all along: nobody can get on.
    let fenced = yaml.replace("bollards: [[150, 250]]", "bollards: [[0, 600]]");
    let (_, out) = watch(&fenced, &with(&[FOOT]), 2);
    assert!(decisions(&out, "footpath").is_empty());
}

/// ADR-0020 part 3: vehicles on an undivided road do not brake for each
/// other while each keeps to its own side (only the line is a wall for
/// their moves while one is beside the other): no head-on conflict, and the
/// eastbound vehicles take as long as with nobody coming the other way.
#[test]
fn oncoming_vehicles_on_their_own_sides_do_not_brake_for_each_other() {
    let yaml = undivided(450.0);
    let alone = yaml.replace(
        "      flow: [{from_s: 0, veh_per_h: 500}]",
        "      flow: [{from_s: 0, veh_per_h: 0}]",
    );
    let both = run(&yaml, &[], 1);
    let one = run(&alone, &[], 1);
    let east = |o: &RunOutputs| -> (usize, f64) {
        let done: Vec<_> = o
            .agents
            .iter()
            .filter(|r| r.entry == "east" && r.exited)
            .collect();
        (
            done.len(),
            done.iter().map(|r| r.travel_time).sum::<f64>() / done.len() as f64,
        )
    };
    let (n_both, t_both) = east(&both);
    let (n_one, t_one) = east(&one);
    assert!(
        n_both > 50 && n_both.abs_diff(n_one) <= 2,
        "{n_both} vs {n_one}"
    );
    assert!((t_both / t_one - 1.0).abs() < 0.02, "{t_both} vs {t_one}");
    assert!(both.conflicts.iter().all(|c| c.kind != "head_on"));
}

/// ADR-0020 part 3: a queued vehicle may move across onto the opposite
/// carriageway when an oncoming gap allows; the oncoming traffic sees it
/// head on and nobody overlaps.
#[test]
fn encroachers_meet_oncoming_traffic_without_overlap() {
    let yaml = undivided(450.0);
    let (seen, out) = watch(&yaml, &with(&[OPP]), 2);
    let took = acts(&out, "opposite_encroachment");
    assert!(!took.is_empty());
    decisions(&out, "opposite_encroachment")
        .iter()
        .for_each(|e| check_terms(e));
    assert!(seen.across_used);
    assert!(!seen.off_road_without_act);
    assert!(!seen.past_far_kerb);
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    assert_eq!(kpi(&out, "opposite_encroachments"), took.len() as f64);
}

// ---------------------------------------------------------------- wrong way

/// ADR-0020 part 4: a driver whose turn leaves by the wrong-way path
/// decides once, before the line. Riding it is the violation; otherwise it
/// takes the legal route and turns back at the opening. With the rule off
/// everyone takes the legal route.
#[test]
fn wrong_way_riders_decide_once_and_the_others_turn_back() {
    let yaml = scenario_file("wrong_way.yaml").replace("duration_s: 1200", "duration_s: 600");
    let rides = |sim: &mut Simulation| {
        let path = sim.network().index("path").unwrap();
        let mut ids = std::collections::BTreeSet::new();
        while !sim.finished() {
            sim.step();
            let a = sim.agents();
            for i in 0..a.len() {
                if a.ribbon[i] as usize == path {
                    ids.insert(a.id[i]);
                }
            }
        }
        ids
    };
    let mut off = Simulation::new(&spec(&yaml, &[]), 5, 1).unwrap();
    assert!(rides(&mut off).is_empty());
    let off = off.finish().unwrap().1;
    let detours = |o: &RunOutputs| {
        o.events
            .iter()
            .filter(|e| e.kind == "reroute" && text(e, "cause") == Some("legal_detour"))
            .count()
    };
    assert!(detours(&off) > 0);
    assert!(decisions(&off, "wrong_way").is_empty());

    let mut on = Simulation::new(&spec(&yaml, &with(&[WRONG])), 5, 1).unwrap();
    let riders = rides(&mut on);
    let out = on.finish().unwrap().1;
    let all = decisions(&out, "wrong_way");
    assert!(!riders.is_empty());
    let mut seen = std::collections::BTreeSet::new();
    for e in &all {
        check_terms(e);
        assert!(seen.insert(e.agent_id), "decided twice: {}", e.agent_id);
        assert_eq!(e.ribbon, "drive");
        // Time saved is the detour saved at its desired speed.
        let start = e.attrs.find("\"time_saved\":").unwrap() + 13;
        let t: f64 = e.attrs[start..start + e.attrs[start..].find(',').unwrap()]
            .parse()
            .unwrap();
        assert!(t > 0.0);
    }
    let took: std::collections::BTreeSet<u64> =
        acts(&out, "wrong_way").iter().map(|e| e.agent_id).collect();
    // Every rider decided to ride (some that did had not reached the path
    // when the run ended).
    assert!(riders.is_subset(&took), "{riders:?} not all in {took:?}");
    assert!(detours(&out) > 0);
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}

// --------------------------------------------------- twin and determinism

/// Every new rule honours the cooperative twin (no decision at all) and
/// gives the same output at 1 and 4 threads.
#[test]
fn new_rules_honour_the_twin_and_are_deterministic() {
    let road = undivided(300.0);
    let ww = scenario_file("wrong_way.yaml").replace("duration_s: 1200", "duration_s: 300");
    let sig = signal(300.0, 300.0, "");
    for (yaml, types) in [
        (&road, vec![FOOT, OPP]),
        (&ww, vec![WRONG]),
        (&sig, vec![SEEP]),
    ] {
        let on = with(&types);
        let mut twin = on.clone();
        twin.push(("violations.enabled", "0"));
        let off = run(yaml, &twin, 7);
        assert!(
            off.events
                .iter()
                .all(|e| e.kind != "violation" && e.kind != "complied")
        );
        let mut one = Simulation::new(&spec(yaml, &on), 7, 1).unwrap();
        let mut four = Simulation::new(&spec(yaml, &on), 7, 4).unwrap();
        four.set_parallel_min(0);
        assert_eq!(one.run_to_end().output_hash, four.run_to_end().output_hash);
    }
}

/// The network refuses a pairing that is not mutual or not reversed, and
/// a wrong-way arm without a legal route through a median opening.
#[test]
fn bad_strips_are_refused() {
    let yaml = undivided(60.0);
    let one_sided = yaml.replacen("      opposite: [east]\n", "", 1);
    let err = Simulation::new(&spec(&one_sided, &[]), 1, 1)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("opposite"), "{err}");
    let skew = yaml.replace(
        "polyline: [[600, 0], [0, 0]]",
        "polyline: [[600, 1], [0, 1]]",
    );
    let err = Simulation::new(&spec(&skew, &[]), 1, 1)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("reversed"), "{err}");
    let ww = scenario_file("wrong_way.yaml")
        .replace("        a: {b: 0.95, a: 0.05}", "        a: {b: 1.0}");
    let err = Simulation::new(&spec(&ww, &[]), 1, 1)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("median opening"), "{err}");
}

/// ADR-0016 open problem 3: a turn's sweep points to its inside: left for a
/// left turn (the near turn in left-hand traffic), right for a right turn;
/// a straight movement has none. `L² / 8ρ` is 1.25 m for a 10 m bus on a
/// 10 m radius.
#[test]
fn turn_sweeps_point_to_the_inside_of_the_turn() {
    let sim = Simulation::new(&spec(&signal(300.0, 60.0, ""), &[]), 1, 1).unwrap();
    let net = sim.network();
    let coef = |id: &str| sim.junctions().sweep_coef[net.index(id).unwrap()];
    assert!(coef("crossing:w>n") > 0.0, "left turn");
    assert!(coef("crossing:w>s") < 0.0, "right turn");
    assert_eq!(coef("crossing:w>e"), 0.0, "through");
    let rho = net.ribbon(net.index("crossing:w>n").unwrap()).min_radius;
    assert!((coef("crossing:w>n") - 1.0 / (8.0 * rho)).abs() < 1e-12);
}
