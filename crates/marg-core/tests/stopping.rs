//! Micro-scenario tests of stop-anywhere events (plan P6.3, ADR-0014).

mod common;
use common::*;
use marg_core::{EventRecord, RunOutputs, Simulation};

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

/// A number from an event's JSON attributes.
fn attr(e: &EventRecord, key: &str) -> f64 {
    let rest = e.attrs.split(&format!("\"{key}\":")).nth(1).unwrap();
    let end = rest.find([',', '}']).unwrap();
    rest[..end].parse().unwrap()
}

/// A 2 km road, 10.5 m wide. `extra` is YAML for the network (attractors).
fn road(duration: f64, flow: f64, composition: &str, extra: &str) -> String {
    format!(
        r#"
name: test
duration_s: {duration}
network:
  ribbons: [{{id: main, polyline: [[0, 0], [2000, 0]], width: [{{s: 0, y_min: -5.25, y_max: 5.25}}]}}]
  {extra}
demand: {{entries: [{{id: west, ribbon: main, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {composition}}}]}}
detectors:
  - {{kind: screenline, id: out, ribbon: main, s: 1900, interval_s: 60}}
kpi: {{throughput_detector: out}}
"#
    )
}

/// Autos stop once per kilometre, pull over 60% of the time, and stand for
/// about 15 s.
const AUTOS: &[(&str, &str)] = &[
    ("road.stop_anywhere.rate.auto_rickshaw", "1.0"),
    (
        "road.stop_anywhere.pull_over_probability.auto_rickshaw",
        "0.6",
    ),
    (
        "road.stop_anywhere.dwell",
        "{dist: log_normal_median, median: 15, sigma: 0.4}",
    ),
];

/// Events come at the asked rate per vehicle-kilometre, only for the class
/// that has one; the asked share pulls over; dwell times follow their
/// distribution; every vehicle that stopped drives on.
#[test]
fn events_come_at_the_rate_per_vehicle_kilometre() {
    let (mut events, mut km, mut pulled, mut dwell) = (0.0, 0.0, 0.0, 0.0);
    for seed in 1..=4 {
        let yaml = road(1500.0, 600.0, "{auto_rickshaw: 0.5, hatchback: 0.5}", "");
        let mut sim = Simulation::new(&spec(&yaml, AUTOS), seed, 1).unwrap();
        sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        let stops: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == "stop_anywhere")
            .collect();
        let autos: std::collections::BTreeSet<u64> = out
            .agents
            .iter()
            .filter(|a| a.class == "auto_rickshaw")
            .map(|a| a.agent_id)
            .collect();
        assert!(stops.iter().all(|e| autos.contains(&e.agent_id)));
        km += out
            .agents
            .iter()
            .filter(|a| a.class == "auto_rickshaw")
            .map(|a| a.distance)
            .sum::<f64>()
            / 1000.0;
        events += stops.len() as f64;
        pulled += stops
            .iter()
            .filter(|e| e.attrs.contains("\"pull_over\":true"))
            .count() as f64;
        dwell += stops.iter().map(|e| attr(e, "dwell_s")).sum::<f64>();
        for e in &stops {
            assert_eq!(attr(e, "rate_per_km"), 1.0);
            assert_eq!(attr(e, "multiplier"), 1.0);
        }
        // Standing ends: a vehicle that stood early in the run has left.
        let stood_early: Vec<u64> = out
            .events
            .iter()
            .filter(|e| e.kind == "stop_anywhere_standing" && e.t < 900.0)
            .map(|e| e.agent_id)
            .collect();
        assert!(!stood_early.is_empty());
        for id in stood_early {
            assert!(
                out.agents.iter().any(|a| a.agent_id == id && a.exited),
                "{id}"
            );
        }
        assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    }
    // Poisson with mean = vehicle-km (about 900): within 4 SD.
    assert!(
        km > 600.0 && (events - km).abs() < 4.0 * km.sqrt(),
        "{events} events in {km:.0} veh-km"
    );
    assert!(
        (pulled / events - 0.6).abs() < 0.06,
        "{pulled} of {events} pulled over"
    );
    // Lognormal, median 15 s, sigma 0.4: mean 16.25 s.
    assert!(
        (dwell / events - 16.25).abs() < 1.0,
        "mean dwell {:.2} s",
        dwell / events
    );
}

/// An attractor multiplies the rate along its stretch and nowhere else.
#[test]
fn an_attractor_raises_the_rate_locally() {
    let attractor =
        "attractors: [{id: metro, ribbon: main, s_from: 800, s_to: 1200, stop_rate_multiplier: 6}]";
    let over = [
        ("road.stop_anywhere.rate.auto_rickshaw", "0.3"),
        AUTOS[1],
        AUTOS[2],
    ];
    let (mut inside, mut outside) = (0.0_f64, 0.0_f64);
    for seed in 1..=4 {
        let yaml = road(1500.0, 700.0, "{auto_rickshaw: 1.0}", attractor);
        let mut sim = Simulation::new(&spec(&yaml, &over), seed, 1).unwrap();
        sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        for e in out.events.iter().filter(|e| e.kind == "stop_anywhere") {
            if (800.0..1200.0).contains(&e.s) {
                assert_eq!(attr(e, "multiplier"), 6.0);
                inside += 1.0;
            } else {
                assert_eq!(attr(e, "multiplier"), 1.0);
                outside += 1.0;
            }
        }
    }
    // Per kilometre of road: 0.4 km inside, 1.6 km outside. The ratio of
    // two Poisson counts (about 800 and 500): within 20%.
    let ratio: f64 = (inside / 0.4) / (outside / 1.6);
    assert!(
        inside > 300.0 && (ratio / 6.0 - 1.0).abs() < 0.2,
        "{inside} inside, {outside} outside"
    );
}

/// On a quiet road a vehicle that pulls over stands at the kerb; one that
/// does not stands where it was driving.
#[test]
fn pulling_over_reaches_the_kerb_and_stopping_in_place_does_not() {
    let stand = |p: &str| {
        let over = [
            AUTOS[0],
            ("road.stop_anywhere.pull_over_probability.auto_rickshaw", p),
            AUTOS[2],
            // Enter in the middle of the road.
            (
                "demand.entry_lateral_position.auto_rickshaw",
                "{dist: constant, value: 0.5}",
            ),
        ];
        let yaml = road(900.0, 200.0, "{auto_rickshaw: 1.0}", "");
        let mut sim = Simulation::new(&spec(&yaml, &over), 2, 1).unwrap();
        sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        let mut gaps = Vec::new();
        // How far sideways each vehicle moved between the event and standing.
        let mut shift: f64 = 0.0;
        let mut y_event = std::collections::BTreeMap::new();
        for e in &out.events {
            if e.kind == "stop_anywhere" {
                y_event.insert(e.agent_id, e.y);
            } else if e.kind == "stop_anywhere_standing" {
                gaps.push(attr(e, "kerb_gap_m"));
                shift = shift.max((e.y - y_event[&e.agent_id]).abs());
            }
        }
        gaps.sort_by(f64::total_cmp);
        assert!(gaps.len() > 40, "{}", gaps.len());
        (gaps[gaps.len() / 2], gaps[gaps.len() * 9 / 10], shift)
    };
    let (median, p90, _) = stand("1.0");
    assert!(
        median < 0.3 && p90 < 1.0,
        "pulled over: median {median:.2} m, p90 {p90:.2} m from the kerb"
    );
    let (median, _, shift) = stand("0.0");
    assert!(
        median > 2.5 && shift < 0.3,
        "in place: median {median:.2} m from the kerb, moved {shift:.2} m sideways"
    );
}

/// Traffic gets past vehicles that stop in the stream: nothing locks up and
/// nothing collides.
#[test]
fn traffic_passes_vehicles_stopped_in_the_stream() {
    let over = [
        ("road.stop_anywhere.rate.auto_rickshaw", "2.0"),
        (
            "road.stop_anywhere.pull_over_probability.auto_rickshaw",
            "0.3",
        ),
        AUTOS[2],
    ];
    let yaml = road(
        1200.0,
        2400.0,
        "{hatchback: 0.35, motorcycle: 0.4, auto_rickshaw: 0.2, bus: 0.05}",
        "",
    );
    let mut sim = Simulation::new(&spec(&yaml, &over), 3, 1).unwrap();
    sim.run_to_end();
    let a = sim.agents();
    let standing = (0..a.len()).filter(|&i| a.v[i] < 0.05).count();
    let (_, out) = sim.finish().unwrap();
    let stops = out
        .events
        .iter()
        .filter(|e| e.kind == "stop_anywhere")
        .count();
    assert!(stops > 150, "{stops}");
    assert!(standing < 15, "{standing} standing at the end");
    // Everything that arrived got in and the road keeps moving: about 140
    // vehicles are on 2 km of it at this flow and speed.
    assert!(kpi(&out, "waiting_to_enter_at_end") < 10.0);
    assert!(kpi(&out, "in_network_at_end") < 220.0);
    assert!(kpi(&out, "network_mean_speed_m_s") > 7.0);
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}

/// A rate without the knob or the dwell is refused, and so is an attractor
/// when nobody stops.
#[test]
fn missing_values_are_refused() {
    let yaml = road(60.0, 100.0, "{auto_rickshaw: 1.0}", "");
    let err = |s: marg_params::ResolvedSpec| Simulation::new(&s, 1, 1).err().unwrap().to_string();
    assert!(err(spec(&yaml, &AUTOS[..1])).contains("pull_over_probability.auto_rickshaw"));
    assert!(err(spec(&yaml, &AUTOS[..2])).contains("road.stop_anywhere.dwell"));
    let attractor =
        "attractors: [{id: m, ribbon: main, s_from: 0, s_to: 100, stop_rate_multiplier: 2}]";
    let yaml = road(60.0, 100.0, "{auto_rickshaw: 1.0}", attractor);
    assert!(err(spec(&yaml, &[])).contains("road.stop_anywhere.rate"));
}

/// Stop-anywhere gives the same result on any number of threads.
#[test]
fn stopping_is_deterministic() {
    let yaml = road(
        300.0,
        3000.0,
        "{hatchback: 0.4, motorcycle: 0.3, auto_rickshaw: 0.3}",
        "",
    );
    let spec = spec(&yaml, AUTOS);
    let run = |threads: usize| {
        let mut sim = Simulation::new(&spec, 11, threads).unwrap();
        sim.set_parallel_min(0);
        sim.run_to_end()
    };
    let one = run(1);
    assert!(one.agent_steps > 100_000, "{one:?}");
    assert_eq!(one, run(4));
}
