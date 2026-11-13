//! Micro-scenario tests of kerbside parking (plan P6.2).

mod common;
use common::*;
use marg_core::model::geometry::{Body, overlap};
use marg_core::{RunOutputs, Simulation};
use marg_params::spec::ObjectKind;

const MIXED: &str = "{hatchback: 0.4, motorcycle: 0.4, auto_rickshaw: 0.15, bus: 0.05}";

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

/// A 700 m road, 10.5 m wide, with parking along the left kerb from 200 m
/// to 500 m. `zone` is extra YAML for the parking stretch.
fn road(duration: f64, flow: f64, occupancy: f64, zone: &str) -> String {
    format!(
        r#"
name: test
duration_s: {duration}
network:
  ribbons: [{{id: main, polyline: [[0, 0], [700, 0]], width: [{{s: 0, y_min: -5.25, y_max: 5.25}}]}}]
  parking:
    - id: kerb
      ribbon: main
      s_from: 200
      s_to: 500
      occupancy: {occupancy}
      composition: {{hatchback: 0.6, sedan: 0.3, motorcycle: 0.1}}
      offset_from_kerb: {{dist: uniform, min: 0.1, max: 0.5}}
      {zone}
demand: {{entries: [{{id: west, ribbon: main, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {MIXED}}}]}}
detectors:
  - {{kind: screenline, id: out, ribbon: main, s: 600, interval_s: 60}}
kpi: {{throughput_detector: out}}
"#
    )
}

/// A stretch with nothing but parked vehicles.
const STATIC: &str =
    "double_parking_probability: 0.0\n      manoeuvres_per_100m_h: 0\n      cruising_share: 0";

/// Parked vehicles fill the asked share of the kerb, stand within the
/// stretch at their drawn offsets, and do not overlap.
#[test]
fn parked_vehicles_fill_the_kerb() {
    for occupancy in [0.0, 0.3, 0.6, 0.9] {
        let sim = Simulation::new(&spec(&road(60.0, 100.0, occupancy, STATIC), &[]), 3, 1).unwrap();
        let objects = sim.objects();
        let parked: f64 = objects.iter().map(|o| o.length).sum();
        // Short of the target by less than one vehicle.
        assert!(
            parked <= occupancy * 300.0 + 1e-9 && parked > occupancy * 300.0 - 5.5,
            "occupancy {occupancy}: {parked} m parked"
        );
        for (k, o) in objects.iter().enumerate() {
            assert_eq!(o.kind, ObjectKind::ParkedVehicle);
            assert!(o.rear() >= 200.0 && o.front() <= 500.0, "{o:?}");
            let offset = 5.25 - (o.y + 0.5 * o.width);
            assert!((0.1..=0.5).contains(&offset), "offset {offset}");
            let a = Body::from_tan(o.s, o.y, o.length, o.width, 0.0);
            for p in &objects[..k] {
                assert!(!overlap(
                    &a,
                    &Body::from_tan(p.s, p.y, p.length, p.width, 0.0)
                ));
            }
        }
    }
}

/// Double parking: about the asked share of parked vehicles has another
/// standing beside it, further from the kerb and not on top of it.
#[test]
fn some_vehicles_double_park() {
    let zone =
        "double_parking_probability: 0.25\n      manoeuvres_per_100m_h: 0\n      cruising_share: 0";
    let (mut inner, mut outer) = (0, 0);
    for seed in 1..=20 {
        let sim = Simulation::new(&spec(&road(60.0, 100.0, 0.7, zone), &[]), seed, 1).unwrap();
        let objects = sim.objects();
        for o in objects.iter().filter(|o| o.id.starts_with("kerb/d")) {
            outer += 1;
            let twin = objects
                .iter()
                .find(|p| p.id == o.id.replace("/d", "/p"))
                .unwrap();
            assert!(
                o.y + 0.5 * o.width < twin.y - 0.5 * twin.width,
                "{o:?} {twin:?}"
            );
        }
        inner += objects
            .iter()
            .filter(|o| o.id.starts_with("kerb/p"))
            .count();
    }
    let share = f64::from(outer) / inner as f64;
    // Binomial: n ≈ 900, p = 0.25, SD ≈ 0.014.
    assert!(
        inner > 700 && (share - 0.25).abs() < 0.05,
        "{outer} of {inner}"
    );
}

/// Traffic passes a parked kerb without touching it: nobody is ever on a
/// parked vehicle, the guard is not needed, and the stream gets through.
#[test]
fn traffic_keeps_clear_of_parked_vehicles() {
    let mut sim = Simulation::new(&spec(&road(600.0, 2500.0, 0.8, STATIC), &[]), 2, 1).unwrap();
    let parked: Vec<Body> = sim
        .objects()
        .iter()
        .map(|o| Body::from_tan(o.s, o.y, o.length, o.width, 0.0))
        .collect();
    // The kerb-side edge of the row of parked vehicles.
    let row = sim
        .objects()
        .iter()
        .map(|o| o.y - 0.5 * o.width)
        .fold(f64::INFINITY, f64::min);
    let mut closest = f64::INFINITY;
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            let b = Body::from_tan(a.s[i], a.y[i], a.length[i], a.width[i], a.heading_tan[i]);
            for o in &parked {
                if (o.s - b.s).abs() < o.half_length + b.half_length {
                    assert!(!overlap(&b, o), "t = {}", sim.time());
                    // The body itself; a turned body's corners are checked by
                    // the exact overlap test above.
                    closest = closest.min(o.y - o.half_width - a.y[i] - 0.5 * a.width[i]);
                }
            }
        }
    }
    let (_, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    assert!(kpi(&out, "throughput_veh_h") > 2200.0);
    assert!(
        row < 3.6 && closest > 0.0,
        "row at {row}, closest pass {closest}"
    );
}

const MANOEUVRES: &[(&str, &str)] = &[(
    "road.parking.manoeuvre_duration",
    "{dist: log_normal_median, median: 20, sigma: 0.5}",
)];

/// Manoeuvres come at the asked rate, each blocks the strip beside its space
/// for its drawn time, begins only when that strip is clear, and moves one
/// vehicle in or out of the kerb.
#[test]
fn manoeuvres_come_at_the_asked_rate_and_block_the_strip() {
    let zone =
        "double_parking_probability: 0.0\n      manoeuvres_per_100m_h: 40\n      cruising_share: 0";
    let (mut n, mut parks, mut total_duration) = (0.0, 0.0, 0.0);
    for seed in 1..=6 {
        let yaml = road(1800.0, 1500.0, 0.6, zone);
        let mut sim = Simulation::new(&spec(&yaml, MANOEUVRES), seed, 1).unwrap();
        let parked0 = sim.parking().parked("kerb").unwrap() as f64;
        let mut seen = 0;
        while !sim.finished() {
            sim.step();
            // A manoeuvre that has just begun: its strip is empty.
            let (a, events) = (sim.agents(), sim.events());
            for e in events[seen..]
                .iter()
                .filter(|e| e.kind == "parking_manoeuvre")
            {
                let o = sim
                    .objects()
                    .iter()
                    .find(|o| e.attrs.contains(&o.id))
                    .unwrap();
                assert_eq!(o.kind, ObjectKind::ParkingManoeuvre);
                let strip = Body::from_tan(o.s, o.y, o.length, o.width, 0.0);
                for i in 0..a.len() {
                    let b = Body::from_tan(
                        a.s_prev[i],
                        a.y_prev[i],
                        a.length[i],
                        a.width[i],
                        a.heading_tan_prev[i],
                    );
                    assert!(!overlap(&b, &strip), "t = {}: {o:?}", sim.time());
                }
            }
            seen = events.len();
        }
        let parked1 = sim.parking().parked("kerb").unwrap() as f64;
        let (_, out) = sim.finish().unwrap();
        let events: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == "parking_manoeuvre")
            .collect();
        let park = events
            .iter()
            .filter(|e| e.attrs.contains("\"park\""))
            .count() as f64;
        // A manoeuvre moves its vehicle when it ends.
        let duration = |e: &marg_core::EventRecord| {
            let d = e.attrs.split("\"duration_s\":").nth(1).unwrap();
            d.split(',').next().unwrap().parse::<f64>().unwrap()
        };
        let done = |park: bool| {
            events
                .iter()
                .filter(|e| e.attrs.contains("\"park\"") == park && e.t + duration(e) <= 1800.0)
                .count() as f64
        };
        // (A vehicle that parks waits in the strip while traffic is in its
        // space, so a few arrivals may not have ended on time.)
        let late = done(true) - done(false) - (parked1 - parked0);
        assert!((0.0..=2.0).contains(&late), "seed {seed}: {late}");
        n += events.len() as f64;
        parks += park;
        for e in &events {
            total_duration += duration(e);
        }
        assert_eq!(kpi(&out, "collision_guard_events"), 0.0, "seed {seed}");
    }
    // 40 per 100 m per hour on 300 m for half an hour, six seeds: 360 expected (SD 19).
    assert!((n - 360.0).abs() < 60.0, "{n} manoeuvres");
    assert!((parks / n - 0.5).abs() < 0.1, "{parks} of {n} are arrivals");
    // Lognormal with median 20 s and sigma 0.5: mean 22.7 s.
    let mean = total_duration / n;
    assert!((mean - 22.7).abs() < 2.5, "mean duration {mean:.1} s");
}

/// Vehicles looking for a space: about the asked share of the types that
/// park here keeps to the cruising speed along the stretch, and nobody else
/// is slowed.
#[test]
fn some_vehicles_cruise_for_a_space() {
    let zone = "double_parking_probability: 0.0\n      manoeuvres_per_100m_h: 0\n      cruising_share: 0.3";
    let yaml = road(1200.0, 400.0, 0.3, zone).replace(MIXED, "{hatchback: 0.7, bus: 0.3}");
    let mut sim =
        Simulation::new(&spec(&yaml, &[("road.parking.cruising_speed", "15")]), 1, 1).unwrap();
    let cap = 15.0 / 3.6;
    let (mut slow, mut cars, mut buses_slow) = (
        std::collections::BTreeSet::new(),
        std::collections::BTreeSet::new(),
        0,
    );
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            // Well inside the stretch, after braking down to the cap.
            if a.s[i] < 350.0 || a.s[i] > 480.0 {
                continue;
            }
            let car = marg_params::VehicleClass::ALL[usize::from(a.class[i])].key() == "car";
            if car {
                cars.insert(a.id[i]);
            }
            if a.v0[i] <= cap + 1e-9 {
                assert!(a.v[i] <= cap + 0.3, "v = {}", a.v[i]);
                if car {
                    slow.insert(a.id[i]);
                } else {
                    buses_slow += 1;
                }
            }
        }
    }
    let share = slow.len() as f64 / cars.len() as f64;
    assert!(
        cars.len() > 60 && (share - 0.3).abs() < 0.15,
        "{} of {}",
        slow.len(),
        cars.len()
    );
    assert_eq!(buses_slow, 0);
}

/// A parking stretch whose survey values are given nowhere is refused, with
/// the name of what is missing.
#[test]
fn missing_survey_values_are_refused() {
    let spec = spec(&road(60.0, 100.0, 0.5, ""), &[]);
    let err = Simulation::new(&spec, 1, 1).err().unwrap().to_string();
    assert!(
        err.contains("double_parking_probability") && err.contains("road.parking"),
        "{err}"
    );
    let zone =
        "double_parking_probability: 0.0\n      manoeuvres_per_100m_h: 10\n      cruising_share: 0";
    let err = Simulation::new(&common::spec(&road(60.0, 100.0, 0.5, zone), &[]), 1, 1)
        .err()
        .unwrap()
        .to_string();
    assert!(err.contains("road.parking.manoeuvre_duration"), "{err}");
}

/// Parking gives the same result on any number of threads.
#[test]
fn parking_is_deterministic() {
    let zone = "double_parking_probability: 0.2\n      manoeuvres_per_100m_h: 60\n      cruising_share: 0.2";
    let over = [MANOEUVRES[0], ("road.parking.cruising_speed", "15")];
    let spec = spec(&road(300.0, 3000.0, 0.7, zone), &over);
    let run = |threads: usize| {
        let mut sim = Simulation::new(&spec, 11, threads).unwrap();
        sim.set_parallel_min(0);
        sim.run_to_end()
    };
    let one = run(1);
    assert!(one.agent_steps > 100_000, "{one:?}");
    assert_eq!(one, run(4));
}

/// P6.2 "done when": the effective-width observer reports `W_eff` consistent
/// with the footprints plus the clearance-derived shy distance. The blocked
/// width is exactly the footprints and the strip between them and the kerb;
/// the gap traffic keeps to the parked row lies between the clearance
/// model's minimum and desired clearance, near the desired one, so the
/// observed `W_eff` agrees with the model's within half a metre per 50 m bin
/// and 0.3 m over the stretch. (Nothing in the model uses `W_eff`.)
#[test]
fn effective_width_matches_footprints_plus_shy_distance() {
    let yaml = road(1500.0, 2000.0, 0.8, STATIC).replace(
        "kpi:",
        "outputs: {effective_width: [{id: w, ribbon: main, s_from: 100, s_to: 500, bin_m: 50}]}\nkpi:",
    );
    let mut sim = Simulation::new(&spec(&yaml, &[]), 5, 1).unwrap();
    let objects = sim.objects().to_vec();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    let rows = &out.effective_width;
    assert_eq!(rows.len(), 8);
    let (mut model, mut observed, mut n) = (0.0, 0.0, 0.0);
    for r in rows {
        // Kerb to the carriageway-side edge of each parked vehicle, by the
        // length of it in the bin.
        let blocked: f64 = objects
            .iter()
            .map(|o| {
                let along = (o.front().min(r.s_to) - o.rear().max(r.s_from)).max(0.0);
                (5.25 - (o.y - 0.5 * o.width)) * along / (r.s_to - r.s_from)
            })
            .sum();
        assert!((r.blocked_width - blocked).abs() < 1e-9, "{r:?}");
        assert!((r.free_width - (10.5 - blocked)).abs() < 1e-9);
        if r.s_from < 200.0 {
            // Clear road: the whole width is used, and there is nothing to
            // shy from (but for a vehicle whose nose is at the first parked one).
            assert_eq!(r.blocked_width, 0.0);
            assert!(r.s_to > 150.0 || (r.shy_desired.is_nan() && r.time_beside == 0.0));
            assert!(r.used_hi - r.used_lo > 10.3, "{r:?}");
            continue;
        }
        assert!(
            r.blocked_width > 1.2 && r.time_beside > 0.5 * r.time_spent,
            "{r:?}"
        );
        assert!(r.shy_minimum < r.shy_desired);
        assert!(
            r.shy_observed_min > 0.0 && r.shy_observed > r.shy_minimum,
            "{r:?}"
        );
        assert!(
            (r.effective_width_observed - r.effective_width_model).abs() < 0.5,
            "{r:?}"
        );
        // Nobody drove where the parked vehicles stand.
        assert!(r.used_hi < 5.25 - 1.2, "{r:?}");
        model += r.effective_width_model;
        observed += r.effective_width_observed;
        n += 1.0;
    }
    eprintln!(
        "W_eff over the parked stretch: model {:.2} m, observed {:.2} m",
        model / n,
        observed / n
    );
    assert!((model - observed).abs() / n < 0.3);
}
