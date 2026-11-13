//! Micro-scenario tests of bus stops, potholes, humps, gradients, standing
//! water and pedestrians (plan P6.4–P6.7).

mod common;
use common::*;
use marg_core::model::geometry::{Body, overlap};
use marg_core::{EventRecord, RunOutputs, Simulation};
use marg_params::spec::ObjectKind;

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

fn attr(e: &EventRecord, key: &str) -> f64 {
    let rest = e.attrs.split(&format!("\"{key}\":")).nth(1).unwrap();
    rest[..rest.find([',', '}']).unwrap()].parse().unwrap()
}

fn class_of(sim: &Simulation, i: usize) -> &'static str {
    marg_params::VehicleClass::ALL[usize::from(sim.agents().class[i])].key()
}

/// A 1 km road, 10.5 m wide; `extra` is YAML for the network.
fn road(duration: f64, flow: f64, composition: &str, extra: &str) -> String {
    format!(
        r#"
name: test
duration_s: {duration}
network:
  ribbons: [{{id: main, polyline: [[0, 0], [1000, 0]], width: [{{s: 0, y_min: -5.25, y_max: 5.25}}]}}]
  {extra}
demand: {{entries: [{{id: west, ribbon: main, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {composition}}}]}}
detectors:
  - {{kind: screenline, id: out, ribbon: main, s: 900, interval_s: 60}}
kpi: {{throughput_detector: out}}
"#
    )
}

const BUS: &[(&str, &str)] = &[
    ("road.bus.dwell_time", "{dist: constant, value: 20}"),
    (
        "road.bus.stop_offset_longitudinal",
        "{dist: constant, value: 0}",
    ),
    (
        "road.bus.stop_offset_lateral",
        "{dist: constant, value: 0.3}",
    ),
];

/// P6.4: buses that serve a stop pull in, stand at the platform for the
/// dwell, outside the passengers waiting on the road, and drive on; the
/// asked share serves it; other traffic gets past.
#[test]
fn buses_stop_at_a_bus_stop() {
    let stop = |share: f64, passengers: f64| {
        format!(
            "bus_stops: [{{id: stand, ribbon: main, s: 500, length: 30, serving_share: {share}, waiting_passenger_width: {passengers}}}]"
        )
    };
    let run = |share: f64, passengers: f64| {
        let yaml = road(
            1500.0,
            900.0,
            "{bus: 0.1, hatchback: 0.55, motorcycle: 0.35}",
            &stop(share, passengers),
        );
        let mut sim = Simulation::new(&spec(&yaml, BUS), 4, 1).unwrap();
        let strip = sim
            .objects()
            .iter()
            .filter(|o| o.kind == ObjectKind::WaitingPassengers)
            .count();
        assert_eq!(strip, usize::from(passengers > 0.0));
        sim.run_to_end();
        let (_, out) = sim.finish().unwrap();
        assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
        out
    };
    let out = run(1.0, 0.0);
    let buses = out
        .agents
        .iter()
        .filter(|a| a.class == "bus" && a.exited)
        .count();
    let calls: Vec<_> = out.events.iter().filter(|e| e.kind == "bus_stop").collect();
    let stood: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "bus_standing")
        .collect();
    assert!(
        buses > 25 && calls.len() >= buses,
        "{buses} buses, {} calls",
        calls.len()
    );
    for e in &stood {
        // Standing along the platform (or one bus back, behind a bus that
        // is standing there).
        assert!(e.s > 470.0 && e.s < 510.0, "stood at s = {}", e.s);
    }
    // About 0.3 m from the kerb, as drawn; now and then traffic keeps one out.
    let mut gaps: Vec<f64> = stood.iter().map(|e| attr(e, "kerb_gap_m")).collect();
    gaps.sort_by(f64::total_cmp);
    let (median, p90) = (gaps[gaps.len() / 2], gaps[gaps.len() * 9 / 10]);
    assert!(
        (0.2..0.5).contains(&median) && p90 < 1.0,
        "median {median}, p90 {p90}"
    );
    // A bus that stood took its dwell longer than its free-flow time.
    for a in out.agents.iter().filter(|a| a.class == "bus" && a.exited) {
        assert!(a.delay > 20.0 && a.stops >= 1, "{a:?}");
    }
    for a in out.agents.iter().filter(|a| a.class != "bus" && a.exited) {
        assert!(a.delay < 60.0, "{a:?}");
    }
    // Half of the buses serve it.
    let out = run(0.5, 0.0);
    let buses = out.agents.iter().filter(|a| a.class == "bus").count() as f64;
    let calls = out.events.iter().filter(|e| e.kind == "bus_stop").count() as f64;
    assert!((calls / buses - 0.5).abs() < 0.15, "{calls} of {buses}");
    // With passengers on the road the bus stands outside them.
    let out = run(1.0, 1.2);
    let stood: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "bus_standing")
        .collect();
    let calls = out.events.iter().filter(|e| e.kind == "bus_stop").count();
    assert!(
        stood.len() > 15 && stood.len() * 10 >= calls * 7,
        "{} stood of {calls}",
        stood.len()
    );
    let mut gaps: Vec<f64> = stood.iter().map(|e| attr(e, "kerb_gap_m")).collect();
    gaps.sort_by(f64::total_cmp);
    // They aim for 1.2 + 0.3 m and stand up to about 0.9 m further out: a
    // bus comes to a stand before its lateral tracking has finished, and a
    // stopped bus cannot creep closer (its corners would swing into the
    // passengers).
    assert!(
        gaps[0] > 1.2 && gaps[gaps.len() / 2] < 2.5,
        "{:?}",
        &gaps[..3]
    );
}

const POTHOLES: &[(&str, &str)] = &[
    ("road.pothole.detection_distance", "30"),
    ("road.pothole.aggressiveness_link", "0"),
    ("road.pothole.crossing_speed.two_wheeler", "10"),
    ("road.pothole.crossing_speed.car", "15"),
    ("road.pothole.crossing_speed.heavy", "20"),
];

/// Pothole responses of every group set to one choice (`swerve`, `slow`,
/// `straddle`, or anything else for hitting).
fn responses(choice: &str) -> Vec<(String, String)> {
    let mut over: Vec<(String, String)> = POTHOLES
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    for g in ["two_wheeler", "car", "heavy"] {
        for r in ["swerve", "slow", "straddle"] {
            let p = if r == choice { "1.0" } else { "0.0" };
            over.push((format!("road.pothole.response.{g}.{r}"), p.to_string()));
        }
    }
    over
}

fn run_with(yaml: &str, over: &[(String, String)], seed: u64) -> (Simulation, RunOutputs) {
    let over: Vec<(&str, &str)> = over.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut sim = Simulation::new(&spec(yaml, &over), seed, 1).unwrap();
    let objects = Simulation::new(&spec(yaml, &over), seed, 1).unwrap();
    sim.run_to_end();
    let (_, out) = sim.finish().unwrap();
    (objects, out)
}

/// P6.5: potholes are laid at the asked density, more of them towards the
/// kerb; a driver who swerves rarely drives over one, one who slows is down
/// to the crossing speed when he reaches it, one who hits it is not, and
/// one who straddles is not slowed at all.
#[test]
fn potholes_are_swerved_slowed_for_straddled_or_hit() {
    let field = "pothole_fields: [{id: f, ribbon: main, s_from: 200, s_to: 800, per_km: 60, kerb_bias: 0.8}]";
    let yaml = road(
        900.0,
        1200.0,
        "{motorcycle: 0.5, hatchback: 0.3, truck: 0.2}",
        field,
    );
    let mut crossed = std::collections::BTreeMap::new();
    for choice in ["swerve", "slow", "straddle", "hit"] {
        let (sim, out) = run_with(&yaml, &responses(choice), 3);
        let holes: Vec<_> = sim
            .objects()
            .iter()
            .filter(|o| o.kind == ObjectKind::Pothole)
            .collect();
        // 36 expected (Poisson, SD 6); 0.8 bias: 70% in the kerb half.
        let kerb_half = holes.iter().filter(|o| o.y > 0.0).count();
        assert!((20..=55).contains(&holes.len()), "{} potholes", holes.len());
        assert!(
            kerb_half * 2 > holes.len(),
            "{kerb_half} of {}",
            holes.len()
        );
        for o in &holes {
            assert!(
                (0.1..3.0).contains(&o.width) && o.depth > 0.0 && o.depth < 0.5,
                "{o:?}"
            );
        }
        let events: Vec<_> = out
            .events
            .iter()
            .filter(|e| e.kind == "pothole_crossed")
            .collect();
        assert!(
            events
                .iter()
                .all(|e| e.attrs.contains(&format!("\"choice\":\"{choice}\"")))
        );
        let mean_speed = events.iter().map(|e| attr(e, "speed")).sum::<f64>() / events.len() as f64;
        let over_cap = events
            .iter()
            .filter(|e| {
                let class = out
                    .agents
                    .iter()
                    .find(|a| a.agent_id == e.agent_id)
                    .unwrap()
                    .class;
                let cap = match class {
                    "two_wheeler" => 10.0,
                    "car" => 15.0,
                    _ => 20.0,
                } / 3.6;
                attr(e, "speed") > cap + 1.0
            })
            .count();
        let delay = kpi(&out, "delay_mean_s");
        eprintln!(
            "{choice}: {} crossings, mean speed {:.1} km/h, {over_cap} faster than the crossing speed, mean delay {delay:.1} s",
            events.len(),
            mean_speed * 3.6
        );
        assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
        crossed.insert(choice, (events.len(), over_cap, delay));
    }
    // Swerving avoids a good part of the holes that hitting drives over
    // (not all: in traffic there is not always room).
    assert!(
        crossed["swerve"].0 * 10 < crossed["hit"].0 * 7,
        "{crossed:?}"
    );
    // Slowing: at the crossing speed on arrival. Hitting: mostly not.
    assert!(crossed["slow"].1 * 20 <= crossed["slow"].0, "{crossed:?}");
    assert!(crossed["hit"].1 * 2 > crossed["hit"].0, "{crossed:?}");
    // Straddling costs nothing; slowing costs time.
    assert!(
        crossed["slow"].2 > crossed["straddle"].2 + 2.0 && crossed["hit"].2 > crossed["straddle"].2,
        "{crossed:?}"
    );
}

/// P6.6: vehicles cross a hump at their drawn hump speed (14–23 km/h in the
/// register); where the hump stops short of the kerb, two-wheelers go round
/// its end and cars, which do not fit, do not.
#[test]
fn humps_cap_speed_and_two_wheelers_bypass_a_short_one() {
    let share_over = |hump: &str| {
        let yaml = road(900.0, 900.0, "{motorcycle: 0.5, hatchback: 0.5}", hump);
        let mut sim = Simulation::new(&spec(&yaml, &[]), 2, 1).unwrap();
        let hump = sim.objects()[0].clone();
        assert_eq!(hump.kind, ObjectKind::Hump);
        // Per class: vehicles that passed the hump's station, and over it.
        let mut seen: std::collections::BTreeMap<u64, (&str, bool)> = Default::default();
        while !sim.finished() {
            sim.step();
            let a = sim.agents();
            for i in 0..a.len() {
                if (a.s[i] - hump.s).abs() > 0.5 * hump.length {
                    continue;
                }
                let over = (a.y[i] - hump.y).abs() < 0.5 * (a.width[i] + hump.width);
                seen.entry(a.id[i]).or_insert((class_of(&sim, i), false)).1 |= over;
                if over {
                    assert!((14.0 / 3.6..=23.0 / 3.6).contains(&a.hump_speed[i]));
                    assert!(
                        a.v[i] <= a.hump_speed[i] + 0.6,
                        "{} km/h on the hump, its speed {} km/h",
                        a.v[i] * 3.6,
                        a.hump_speed[i] * 3.6
                    );
                }
            }
        }
        let share = |class: &str| {
            let of: Vec<_> = seen.values().filter(|x| x.0 == class).collect();
            assert!(of.len() > 50);
            of.iter().filter(|x| x.1).count() as f64 / of.len() as f64
        };
        (share("two_wheeler"), share("car"))
    };
    let (bikes, cars) = share_over("humps: [{id: h, ribbon: main, s: 500, length: 3.7}]");
    assert_eq!((bikes, cars), (1.0, 1.0));
    // A 1.2 m gap at the kerb: a motorcycle fits, a car does not.
    let (bikes, cars) =
        share_over("humps: [{id: h, ribbon: main, s: 500, length: 3.7, y_to: 4.05}]");
    eprintln!(
        "short hump: {:.0}% of two-wheelers and {:.0}% of cars go over it",
        bikes * 100.0,
        cars * 100.0
    );
    assert!(
        bikes < 0.9 && cars > 0.98,
        "two-wheelers {bikes:.2}, cars {cars:.2}"
    );
}

/// P6.6: an upgrade lowers desired speeds by class (the register's km/h per
/// percent), caps an e-rickshaw at what its power allows, and a rough
/// surface slows cars more than heavy vehicles.
#[test]
fn gradients_and_roughness_lower_desired_speeds() {
    let zone = "surfaces: [{id: ramp, ribbon: main, s_from: 300, s_to: 700, gradient_percent: 4, roughness_iri: 2}]";
    let yaml = road(
        600.0,
        600.0,
        "{hatchback: 0.4, truck: 0.3, e_rickshaw: 0.3}",
        zone,
    );
    let over = [
        ("vehicles.e_rickshaw.loaded_mass", "1000"),
        ("vehicles.gradient_speed_loss.auto_rickshaw", "2.0"),
        ("vehicles.gradient_speed_loss.tractor", "2.06"),
        ("road.roughness.speed_loss_per_iri.car", "3.0"),
        ("road.roughness.speed_loss_per_iri.heavy", "1.0"),
    ];
    let mut sim = Simulation::new(&spec(&yaml, &over), 1, 1).unwrap();
    // 2 kW, 1,000 kg, 4%: 5.1 m/s.
    let cap = 2000.0 / (1000.0 * 9.80665 * 0.04);
    let mut checked = [0; 3];
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            let on = (300.0..700.0).contains(&a.s[i]);
            let free = a.v0_base[i]
                + if a.y[i] - a.half_width[i] < -5.25 + 3.5 {
                    a.v0_bonus[i]
                } else {
                    0.0
                };
            let (k, expect) = match class_of(&sim, i) {
                "car" => (0, free - (4.0 * 2.96 + 2.0 * 3.0) / 3.6),
                "truck" => (1, free - (4.0 * 2.06 + 2.0 * 1.0) / 3.6),
                _ => (2, free.min(cap)),
            };
            if on {
                // (The median band's bonus is checked loosely: its width is a register value.)
                assert!(
                    a.v0[i] <= expect.max(0.5) + 3.0 && a.v0[i] < free,
                    "{}",
                    class_of(&sim, i)
                );
                if k == 2 {
                    assert!(a.v0[i] <= cap + 1e-9);
                    // (It enters faster than the ramp allows and slows on it.)
                    assert!(a.s[i] < 400.0 || a.v[i] <= cap + 0.3, "{}", a.v[i]);
                }
                checked[k] += 1;
            } else {
                assert!(a.v0[i] >= a.v0_base[i] - 1e-9);
            }
        }
    }
    assert!(checked.iter().all(|&n| n > 1000), "{checked:?}");
}

/// P6.6: shallow water caps speed on the patch, hides the potholes under it,
/// and a closed patch is driven round.
#[test]
fn water_caps_speed_closes_width_and_hides_potholes() {
    let net = "waterlogging:\n    - {id: shallow, ribbon: main, s_from: 300, s_to: 400, speed_cap_kmh: 12}\n    - {id: deep, ribbon: main, s_from: 600, s_to: 640, y_from: 1.0, closed: true}\n  pothole_fields: [{id: f, ribbon: main, s_from: 250, s_to: 450, per_km: 150, kerb_bias: 0}]";
    let yaml = road(900.0, 900.0, "{motorcycle: 0.5, hatchback: 0.5}", net);
    let over: Vec<(String, String)> = responses("swerve");
    let over: Vec<(&str, &str)> = over.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let mut sim = Simulation::new(&spec(&yaml, &over), 2, 1).unwrap();
    let hidden = sim.objects().iter().filter(|o| o.hidden).count();
    let seen = sim
        .objects()
        .iter()
        .filter(|o| o.kind == ObjectKind::Pothole && !o.hidden)
        .count();
    assert!(hidden > 5 && seen > 5, "{hidden} hidden, {seen} not");
    assert!(
        sim.objects()
            .iter()
            .filter(|o| o.hidden)
            .all(|o| (300.0..=400.0).contains(&o.s))
    );
    let deep = sim
        .objects()
        .iter()
        .find(|o| o.id == "deep")
        .unwrap()
        .clone();
    let deep_body = Body::from_tan(deep.s, deep.y, deep.length, deep.width, 0.0);
    let mut on_patch = 0;
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            // Well onto the shallow patch: at its speed.
            if (330.0..400.0).contains(&a.s[i]) {
                assert!(
                    a.v[i] <= 12.0 / 3.6 + 0.6,
                    "{} km/h in the water",
                    a.v[i] * 3.6
                );
                on_patch += 1;
            }
            let b = Body::from_tan(a.s[i], a.y[i], a.length[i], a.width[i], a.heading_tan[i]);
            assert!(!overlap(&b, &deep_body), "t = {}", sim.time());
        }
    }
    assert!(on_patch > 1000);
    let (_, out) = sim.finish().unwrap();
    let crossed: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "pothole_crossed")
        .collect();
    let unseen = crossed
        .iter()
        .filter(|e| e.attrs.contains("unseen"))
        .count();
    eprintln!("{} crossings, {unseen} of holes under water", crossed.len());
    // Everyone swerves round what he sees: most crossings are of hidden holes.
    assert!(
        unseen * 2 > crossed.len() && unseen > 20,
        "{unseen} of {}",
        crossed.len()
    );
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    assert!(kpi(&out, "exited") > 150.0);
}

const PEDESTRIANS: &[(&str, &str)] = &[
    (
        "road.pedestrian.accepted_gap",
        "{dist: log_normal_median, median: 4, sigma: 0.3}",
    ),
    (
        "road.pedestrian.group_size",
        "{dist: uniform, min: 0.5, max: 3.5}",
    ),
    ("road.pedestrian.walking_speed", "1.2"),
    ("road.pedestrian.body_size", "0.6"),
];

/// P6.7: pedestrians arrive at the asked rate, cross by gap acceptance
/// without ever being walked into or walking into a vehicle, wait at the
/// edge or on the road when traffic is too close, and get across.
#[test]
fn pedestrians_cross_by_gap_acceptance() {
    let zone = "pedestrian_crossings: [{id: market, ribbon: main, s_from: 400, s_to: 600, per_hour: 400, from_left_share: 0.5}]";
    let yaml = road(
        1500.0,
        1500.0,
        "{hatchback: 0.4, motorcycle: 0.45, auto_rickshaw: 0.1, bus: 0.05}",
        zone,
    );
    let mut sim = Simulation::new(&spec(&yaml, PEDESTRIANS), 5, 1).unwrap();
    let mut touching = 0;
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for o in sim
            .objects()
            .iter()
            .filter(|o| o.active && o.kind == ObjectKind::Pedestrians)
        {
            let ped = Body::from_tan(o.s, o.y, o.length, o.width, 0.0);
            for i in 0..a.len() {
                if (a.s[i] - o.s).abs() < 12.0 {
                    let b =
                        Body::from_tan(a.s[i], a.y[i], a.length[i], a.width[i], a.heading_tan[i]);
                    touching += usize::from(overlap(&b, &ped));
                }
            }
        }
    }
    let waiting = sim.pedestrians().crossing_now();
    let (_, out) = sim.finish().unwrap();
    let done: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "pedestrian_crossing")
        .collect();
    let n = done.len() as f64;
    let mean = |key: &str| done.iter().map(|e| attr(e, key)).sum::<f64>() / n;
    eprintln!(
        "{n} crossings ({waiting} under way at the end): waited {:.1} s at the edge, {:.1} s on the road, {:.1} s to cross; \
         {touching} body-steps touching; {} guard events; vehicle delay {:.1} s",
        mean("waited_at_edge_s"),
        mean("waited_on_road_s"),
        mean("duration_s"),
        kpi(&out, "collision_guard_events"),
        kpi(&out, "delay_mean_s"),
    );
    // 400 per hour for 25 minutes: 167 expected (SD 13).
    assert!((n + waiting as f64 - 167.0).abs() < 45.0, "{n}");
    assert!(waiting < 25, "{waiting} still waiting");
    // 10.5 m at 1.2 m/s is 9.3 s with a 0.6 m body; waiting only adds.
    assert!(done.iter().all(|e| attr(e, "duration_s") > 8.5));
    assert!(mean("waited_at_edge_s") + mean("waited_on_road_s") > 1.0);
    assert_eq!(touching, 0);
    assert!(kpi(&out, "exited") > 400.0);
}

/// Missing values are refused by name, and everything together is
/// deterministic across threads.
#[test]
fn missing_values_are_refused_and_runs_are_deterministic() {
    let err = |extra: &str, over: &[(&str, &str)]| {
        let yaml = road(60.0, 100.0, "{hatchback: 1.0}", extra);
        Simulation::new(&spec(&yaml, over), 1, 1)
            .err()
            .unwrap()
            .to_string()
    };
    let stop = "bus_stops: [{id: b, ribbon: main, s: 500, length: 30, serving_share: 1}]";
    assert!(err(stop, &[]).contains("road.bus.dwell_time"));
    assert!(err(stop, BUS).contains("road.bus.waiting_passenger_width"));
    let field = "pothole_fields: [{id: f, ribbon: main, s_from: 0, s_to: 100, kerb_bias: 0}]";
    assert!(err(field, &[]).contains("road.pothole.density"));
    let field =
        "pothole_fields: [{id: f, ribbon: main, s_from: 0, s_to: 100, per_km: 5, kerb_bias: 0}]";
    assert!(err(field, &[]).contains("road.pothole.detection_distance"));
    let ramp = "surfaces: [{id: r, ribbon: main, s_from: 0, s_to: 100, gradient_percent: 3}]";
    assert!(err(ramp, &[]).contains("is unset") && err(ramp, &[]).contains("vehicles."));
    let rough = "surfaces: [{id: r, ribbon: main, s_from: 0, s_to: 100, roughness_iri: 3}]";
    assert!(err(rough, &[]).contains("road.roughness.speed_loss_per_iri.car"));
    let peds = "pedestrian_crossings: [{id: p, ribbon: main, s_from: 0, s_to: 100, per_hour: 10, from_left_share: 0.5}]";
    assert!(err(peds, &[]).contains("road.pedestrian.walking_speed"));
    let water = "waterlogging: [{id: w, ribbon: main, s_from: 0, s_to: 100}]";
    assert!(err(water, &[]).contains("speed_cap_kmh"));

    let net = "bus_stops: [{id: b, ribbon: main, s: 300, length: 30, serving_share: 1, waiting_passenger_width: 1}]\n  humps: [{id: h, ribbon: main, s: 700, length: 3.7, y_to: 4.0}]\n  pothole_fields: [{id: f, ribbon: main, s_from: 350, s_to: 650, per_km: 80, kerb_bias: 0.5}]\n  waterlogging: [{id: w, ribbon: main, s_from: 600, s_to: 650, y_from: 2, speed_cap_kmh: 10}]\n  pedestrian_crossings: [{id: p, ribbon: main, s_from: 750, s_to: 900, per_hour: 600, from_left_share: 0.5}]";
    let yaml = road(
        300.0,
        2500.0,
        "{hatchback: 0.35, motorcycle: 0.4, bus: 0.1, truck: 0.15}",
        net,
    );
    let mut over: Vec<(String, String)> = responses("none");
    for g in ["two_wheeler", "car", "heavy"] {
        over.retain(|(k, _)| !k.contains(&format!("response.{g}")));
        over.push((format!("road.pothole.response.{g}.swerve"), "0.4".into()));
        over.push((format!("road.pothole.response.{g}.slow"), "0.3".into()));
        over.push((format!("road.pothole.response.{g}.straddle"), "0.1".into()));
    }
    over.retain(|(k, _)| !k.contains("aggressiveness_link"));
    over.push(("road.pothole.aggressiveness_link".into(), "0.5".into()));
    for (k, v) in BUS.iter().chain(PEDESTRIANS) {
        over.push((k.to_string(), v.to_string()));
    }
    let over: Vec<(&str, &str)> = over.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let spec = spec(&yaml, &over);
    let run = |threads: usize| {
        let mut sim = Simulation::new(&spec, 11, threads).unwrap();
        sim.set_parallel_min(0);
        sim.run_to_end()
    };
    let one = run(1);
    assert!(one.agent_steps > 50_000, "{one:?}");
    assert_eq!(one, run(4));
}
