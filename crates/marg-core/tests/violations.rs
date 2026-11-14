//! The violation decision engine (plan P8.3, ADR-0018) through its first
//! type, forced entry at an uncontrolled junction (G§2.4, §4.2).

mod common;
use common::*;
use marg_core::{EventRecord, RunOutputs, Simulation};

/// The rule's inputs for these tests (the guide has no numbers: assumed).
const FORCED: &[(&str, &str)] = &[
    ("violations.forced_entry.intercept", "-5.0"),
    ("violations.coefficients.aggressiveness", "0.5"),
    ("violations.coefficients.time_saved", "0.0"),
    ("violations.coefficients.waiting_time", "0.08"),
    ("violations.coefficients.imitation_violators", "0.5"),
    ("violations.coefficients.imitation_compliers", "0.2"),
    ("violations.coefficients.enforcement", "0.0"),
    ("violations.coefficients.risk", "2.0"),
];

fn crossing(duration: f64) -> String {
    scenario_file("priority_crossing.yaml")
        .replace("duration_s: 1800", &format!("duration_s: {duration}"))
        .replace("warmup_s: 300", "warmup_s: 0")
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

fn number(e: &EventRecord, key: &str) -> f64 {
    let rest = e.attrs.split(&format!("\"{key}\":")).nth(1).unwrap();
    let end = rest.find([',', '}']).unwrap();
    rest[..end].parse().unwrap()
}

/// P8.3's done-when: for every decision the logged terms add up to the
/// logged `η`, `λ = λ_max σ(η)`, and the logged probability is
/// `1 − exp(−Λ)`; agents that acted had `Λ ≥ E`, the others did not. Each
/// term is its coefficient times its logged feature.
#[test]
fn logged_terms_add_up_to_the_logged_probability() {
    let out = run(&crossing(900.0), FORCED, 1);
    let acts: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "violation")
        .collect();
    let complied: Vec<_> = out.events.iter().filter(|e| e.kind == "complied").collect();
    assert!(acts.len() >= 10, "{} forced entries", acts.len());
    assert!(complied.len() >= 10, "{} complied", complied.len());
    assert_eq!(kpi(&out, "forced_entries"), acts.len() as f64);
    for e in acts.iter().chain(&complied) {
        let terms = object(e, "terms");
        let features = object(e, "features");
        let coefficients = object(e, "coefficients");
        let f = |k: &str| features.iter().find(|(n, _)| n == k).unwrap().1;
        let t = |k: &str| terms.iter().find(|(n, _)| n == k).unwrap().1;
        let c = |k: &str| coefficients.iter().find(|(n, _)| n == k).unwrap().1;
        let eta = number(e, "eta");
        let sum: f64 = terms.iter().map(|(_, v)| v).sum();
        assert!((sum - eta).abs() < 1e-9, "{}: {sum} vs {eta}", e.attrs);
        let hazard = number(e, "max_rate") / (1.0 + (-eta).exp());
        assert!((number(e, "hazard") - hazard).abs() < 1e-12);
        let p = 1.0 - (-number(e, "integrated")).exp();
        assert!((number(e, "probability") - p).abs() < 1e-12);
        // Each term is the agent's coefficient times its feature; traits
        // scale the shared coefficients (ADR-0018 part 2), never the
        // intercept or κ's.
        assert_eq!((t("intercept"), c("intercept")), (-5.0, -5.0));
        assert_eq!(c("aggressiveness"), 0.5);
        assert!((t("aggressiveness") - 0.5 * f("kappa")).abs() < 1e-12);
        assert!((t("waiting_time") - c("waiting_time") * f("waiting_time")).abs() < 1e-12);
        assert!((t("risk") + c("risk") * f("risk")).abs() < 1e-12);
        let imitation = c("imitation_violators") * (1.0 + f("n_violating")).ln();
        assert!((t("imitation_violators") - imitation).abs() < 1e-12);
        let deterred = c("imitation_compliers") * (1.0 + f("n_complying")).ln();
        assert!((t("imitation_compliers") + deterred).abs() < 1e-12);
        assert!(c("waiting_time") > 0.0 && c("risk") > 0.0);
        assert!((0.0..=1.0).contains(&f("risk")));
        let fired = number(e, "integrated") >= number(e, "threshold");
        assert_eq!(fired, e.kind == "violation", "{}", e.attrs);
    }
    // Agents' counts match the events.
    let per_agent: u32 = out.agents.iter().map(|r| r.violations).sum();
    assert_eq!(per_agent as usize, acts.len());
}

/// Forcing in shortens the minor road's waits, and the major-road vehicles
/// it cuts in front of brake (arbitration) rather than run into it.
#[test]
fn forced_entry_shortens_waits_on_the_minor_road() {
    let yaml = crossing(900.0);
    let (mut base_wait, mut forced_wait, mut guards) = (0.0, 0.0, 0.0);
    for seed in 1..=2 {
        let base = run(&yaml, &[], seed);
        let forced = run(&yaml, FORCED, seed);
        let waits = |out: &RunOutputs| {
            let w: Vec<f64> = out
                .events
                .iter()
                .filter(|e| e.kind == "gap_accepted" || e.kind == "violation")
                .map(|e| {
                    if e.kind == "violation" {
                        object(e, "features")
                            .iter()
                            .find(|(n, _)| n == "waiting_time")
                            .unwrap()
                            .1
                    } else {
                        number(e, "waited_s")
                    }
                })
                .collect();
            w.iter().sum::<f64>() / w.len() as f64
        };
        base_wait += waits(&base);
        forced_wait += waits(&forced);
        guards += kpi(&forced, "collision_guard_events");
        assert!(kpi(&base, "forced_entries") == 0.0);
    }
    assert!(
        forced_wait < 0.9 * base_wait,
        "mean wait {forced_wait} with forced entry vs {base_wait}"
    );
    assert!(guards <= 20.0, "{guards} guard events");
}

/// The cooperative twin (`violations.enabled = 0`, ADR-0007) starts no
/// episode: the same output as a run without the rule.
#[test]
fn the_cooperative_twin_switches_the_rule_off() {
    let yaml = crossing(300.0);
    let mut off: Vec<(&str, &str)> = FORCED.to_vec();
    off.push(("violations.enabled", "0"));
    let mut a = Simulation::new(&spec(&yaml, &off), 3, 1).unwrap();
    let mut b = Simulation::new(&spec(&yaml, &[]), 3, 1).unwrap();
    let (ha, hb) = (a.run_to_end().output_hash, b.run_to_end().output_hash);
    assert_eq!(ha, hb);
    let out = a.finish().unwrap().1;
    assert!(
        out.events
            .iter()
            .all(|e| e.kind != "violation" && e.kind != "complied")
    );
}

/// A type switched on without its coefficients, or one not built yet, is
/// refused at start-up with a message that says what to set.
#[test]
fn a_rule_without_its_inputs_is_refused() {
    let yaml = crossing(60.0);
    let err = Simulation::new(
        &spec(&yaml, &[("violations.forced_entry.intercept", "-4")]),
        1,
        1,
    )
    .err()
    .unwrap()
    .to_string();
    assert!(err.contains("violations.coefficients"), "{err}");
    let err = Simulation::new(
        &spec(&yaml, &[("violations.red_light.intercept", "-4")]),
        1,
        1,
    )
    .err()
    .unwrap()
    .to_string();
    assert!(err.contains("not built"), "{err}");
}

/// Measurement for STATUS (not a check): the priority crossing, 30 min,
/// three seeds, without and with forced entry. Run with
/// `cargo test --release -p marg-core --test violations forced_entry_effect -- --ignored --nocapture`.
#[test]
#[ignore]
fn forced_entry_effect() {
    let yaml = crossing(1800.0);
    println!(
        "seed  arm     minor_wait_s  forced  complied  share_forced  guards  exited  delay_mean_s"
    );
    for seed in 1..=3 {
        for (arm, overrides) in [("base", &[][..]), ("forced", FORCED)] {
            let out = run(&yaml, overrides, seed);
            let count = |k: &str| out.events.iter().filter(|e| e.kind == k).count();
            let waits: Vec<f64> = out
                .events
                .iter()
                .filter(|e| e.kind == "gap_accepted" || e.kind == "violation")
                .map(|e| {
                    if e.kind == "violation" {
                        object(e, "features")
                            .iter()
                            .find(|(n, _)| n == "waiting_time")
                            .unwrap()
                            .1
                    } else {
                        number(e, "waited_s")
                    }
                })
                .collect();
            let (forced, complied) = (count("violation"), count("complied"));
            println!(
                "{seed:<5} {arm:<7} {:>12.1}  {forced:>6}  {complied:>8}  {:>12.3}  {:>6}  {:>6.0}  {:>12.1}",
                waits.iter().sum::<f64>() / waits.len() as f64,
                forced as f64 / (forced + complied).max(1) as f64,
                kpi(&out, "collision_guard_events"),
                kpi(&out, "exited"),
                kpi(&out, "delay_mean_s"),
            );
        }
    }
}
