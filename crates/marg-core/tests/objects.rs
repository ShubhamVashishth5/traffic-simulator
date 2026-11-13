//! Micro-scenario tests of road objects and of a narrowing carriageway
//! (plan P6.1).

mod common;
use common::*;
use marg_core::model::geometry::{Body, overlap};
use marg_core::{RunOutputs, Simulation};
use marg_params::Scenario;
use marg_params::spec::ObjectSpec;

const MIXED: &str = "{hatchback: 0.4, motorcycle: 0.4, auto_rickshaw: 0.15, bus: 0.05}";
/// A parked car in the middle of the road, 400 m along.
const PARKED: &str =
    "{id: parked, ribbon: main, s: 400, y: 0.0, length: 4.5, width: 1.8, clearance_as: car}";

fn kpi(out: &RunOutputs, name: &str) -> f64 {
    out.kpis.iter().find(|(k, _)| *k == name).unwrap().1
}

/// A 700 m road with width profile `width` (10.5 m wide if empty) and
/// `objects` (a YAML list), one entry and a screenline at 600 m.
fn road(duration: f64, flow: f64, composition: &str, width: &str, objects: &str) -> String {
    let width = if width.is_empty() {
        "{s: 0, y_min: -5.25, y_max: 5.25}"
    } else {
        width
    };
    format!(
        r#"
name: test
duration_s: {duration}
network:
  ribbons: [{{id: main, polyline: [[0, 0], [700, 0]], width: [{width}]}}]
  objects: [{objects}]
demand: {{entries: [{{id: west, ribbon: main, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {composition}}}]}}
detectors:
  - {{kind: screenline, id: out, ribbon: main, s: 600, interval_s: 60}}
kpi: {{throughput_detector: out}}
"#
    )
}

/// The footprint of object `k` as a body.
fn footprint(sim: &Simulation, k: usize) -> Body {
    let o = &sim.objects()[k];
    Body::from_tan(o.s, o.y, o.length, o.width, 0.0)
}

/// Store indices of the agents whose body overlaps `body` now.
fn on(sim: &Simulation, body: &Body) -> Vec<usize> {
    let a = sim.agents();
    (0..a.len())
        .filter(|&i| {
            let b = Body::from_tan(a.s[i], a.y[i], a.length[i], a.width[i], a.heading_tan[i]);
            overlap(&b, body)
        })
        .collect()
}

/// P6.1: vehicles go round an object. Nobody is ever on its footprint, the
/// guard is not needed, the stream gets through, and vehicles that entered
/// in line with the object have moved aside by the time they pass it.
#[test]
fn vehicles_go_round_an_object() {
    let kerbside = "{id: kerb, ribbon: main, s: 250, y: 4.2, length: 6, width: 2}";
    let yaml = road(900.0, 700.0, MIXED, "", &format!("{PARKED}, {kerbside}"));
    let mut sim = Simulation::new(&spec(&yaml, &[]), 2, 1).unwrap();
    let (parked, kerb) = (footprint(&sim, 0), footprint(&sim, 1));
    // Agents that entered within the parked car's lateral span, and passed it.
    let mut diverted = std::collections::BTreeSet::new();
    let mut closest: f64 = f64::INFINITY;
    while !sim.finished() {
        sim.step();
        assert!(on(&sim, &parked).is_empty(), "t = {}", sim.time());
        assert!(on(&sim, &kerb).is_empty(), "t = {}", sim.time());
        let a = sim.agents();
        for i in 0..a.len() {
            if (a.s[i] - 400.0).abs() < 0.5 * (a.length[i] + 4.5) {
                let gap = (a.y[i]).abs() - 0.5 * a.width[i] - 0.9;
                closest = closest.min(gap);
                if a.entry_y[i].abs() < 0.9 {
                    diverted.insert(a.id[i]);
                }
            }
        }
    }
    let (summary, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    assert!(diverted.len() > 10, "{} vehicles diverted", diverted.len());
    assert!(closest > 0.0, "closest pass {closest} m");
    // 700 veh/h for 900 s, less those still on the road: nearly all got out.
    assert!(
        summary.exited as f64 > 0.85 * summary.spawned as f64 && summary.spawned > 150,
        "{summary:?}"
    );
    // The objects are logged when they appear.
    let added: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind == "object_added")
        .collect();
    assert_eq!(added.len(), 2);
    assert!(
        added[0].attrs.contains("\"object\":\"parked\""),
        "{}",
        added[0].attrs
    );
}

/// A class told to ignore an object drives over its footprint; the others
/// still go round.
#[test]
fn a_class_can_ignore_an_object() {
    let object = "{id: rough, ribbon: main, s: 400, y: 0.0, length: 4.5, width: 1.8, response: {two_wheeler: ignore}}";
    let yaml = road(
        600.0,
        700.0,
        "{hatchback: 0.5, motorcycle: 0.5}",
        "",
        object,
    );
    // Everyone enters in line with the object and has no reason to leave it.
    let centre = "{dist: constant, value: 0.5}";
    let spec = spec(
        &yaml,
        &[
            ("demand.entry_lateral_position.car", centre),
            ("demand.entry_lateral_position.two_wheeler", centre),
            // Nothing draws them to the median side on the way.
            (
                "vehicles.median_band.speed_bonus",
                "{dist: constant, value: 0}",
            ),
        ],
    );
    let mut sim = Simulation::new(&spec, 3, 1).unwrap();
    let body = footprint(&sim, 0);
    let two_wheeler = marg_params::VehicleClass::TwoWheeler.index() as u8;
    let mut over = 0;
    while !sim.finished() {
        sim.step();
        for i in on(&sim, &body) {
            assert_eq!(sim.agents().class[i], two_wheeler);
            over += 1;
        }
    }
    assert!(
        over > 20,
        "two-wheelers were on the footprint in {over} agent-steps"
    );
    let (_, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}

/// An object with a lifetime: before it appears vehicles use its place, a
/// vehicle that is on the spot when it appears drives on, nobody enters the
/// footprint while it is there, and both changes are logged.
#[test]
fn objects_appear_and_go() {
    let object = "{id: stall, ribbon: main, s: 400, y: 0.0, length: 8, width: 3, present: [{from_s: 200, to_s: 400}]}";
    let yaml = road(600.0, 900.0, MIXED, "", object);
    let spec = spec(
        &yaml,
        &[(
            "demand.entry_lateral_position.car",
            "{dist: constant, value: 0.5}",
        )],
    );
    let mut sim = Simulation::new(&spec, 5, 1).unwrap();
    let body = footprint(&sim, 0);
    let (mut before, mut after) = (0, 0);
    let mut caught: Vec<u64> = Vec::new();
    while !sim.finished() {
        sim.step();
        let t = sim.time();
        let there: Vec<u64> = on(&sim, &body)
            .iter()
            .map(|&i| sim.agents().id[i])
            .collect();
        if t <= 200.0 {
            before += there.len();
            if t > 199.95 {
                caught = there;
            }
        } else if t <= 400.0 {
            // Only those that were under it when it appeared, on their way out.
            assert!(there.iter().all(|id| caught.contains(id)), "t = {t}");
            caught.retain(|id| there.contains(id));
        } else {
            after += there.len();
        }
    }
    assert!(before > 50 && after > 50, "before {before}, after {after}");
    let (_, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    let log: Vec<_> = out
        .events
        .iter()
        .filter(|e| e.kind.starts_with("object_"))
        .map(|e| (e.kind, (e.t * 10.0).round() / 10.0))
        .collect();
    assert_eq!(log, [("object_added", 200.0), ("object_removed", 400.0)]);
}

/// Objects can be added and removed while the simulation runs (the live
/// viewer's commands), and bad ones are refused.
#[test]
fn objects_are_added_and_removed_live() {
    let yaml = road(400.0, 700.0, MIXED, "", "");
    let mut sim = Simulation::new(&spec(&yaml, &[]), 7, 1).unwrap();
    let object: ObjectSpec = serde_norway::from_str(PARKED).unwrap();
    let body = Body::from_tan(object.s, object.y, object.length, object.width, 0.0);
    let run_to = |sim: &mut Simulation, t: f64, check: &mut dyn FnMut(&Simulation)| {
        while sim.time() < t {
            sim.step();
            check(sim);
        }
    };
    let mut through = 0;
    run_to(&mut sim, 100.0, &mut |s| through += on(s, &body).len());
    assert!(
        through > 0,
        "vehicles use the spot before the object is there"
    );
    sim.add_object(&object).unwrap();
    assert!(sim.add_object(&object).is_err(), "duplicate id");
    let off_road = ObjectSpec {
        id: "x".into(),
        s: 699.0,
        ..object.clone()
    };
    assert!(
        sim.add_object(&off_road).is_err(),
        "footprint beyond the ribbon"
    );
    // Anyone caught under it drives off within a few seconds; then it is avoided.
    run_to(&mut sim, 110.0, &mut |_| {});
    run_to(&mut sim, 250.0, &mut |s| {
        assert!(on(s, &body).is_empty(), "t = {}", s.time())
    });
    sim.remove_object("parked").unwrap();
    assert!(sim.remove_object("parked").is_err(), "already gone");
    assert!(sim.remove_object("nope").is_err());
    let mut again = 0;
    run_to(&mut sim, 400.0, &mut |s| again += on(s, &body).len());
    assert!(again > 0, "the spot is used again once the object is gone");
    let kinds: Vec<_> = sim
        .events()
        .iter()
        .filter(|e| e.kind.starts_with("object_"))
        .map(|e| e.kind)
        .collect();
    assert_eq!(kinds, ["object_added", "object_removed"]);
    assert!(!sim.events().iter().any(|e| e.kind == "collision_guard"));
}

/// Nobody is put on the road on top of an object.
#[test]
fn an_object_on_the_entry_blocks_it() {
    let wall = "{id: wall, ribbon: main, s: 3, y: 0.0, length: 6, width: 10.5}";
    let yaml = road(120.0, 900.0, MIXED, "", wall);
    let mut sim = Simulation::new(&spec(&yaml, &[]), 1, 1).unwrap();
    let summary = sim.run_to_end();
    assert_eq!(summary.spawned, 0);
}

/// A scenario with objects does not parse if an object is malformed.
#[test]
fn bad_objects_are_refused() {
    for (object, why) in [
        (
            "{id: a, ribbon: nowhere, s: 400, y: 0, length: 4, width: 2}",
            "ribbon",
        ),
        (
            "{id: a, ribbon: main, s: 400, y: 0, length: 0, width: 2}",
            "length",
        ),
        (
            "{id: a, ribbon: main, s: 699, y: 0, length: 4, width: 2}",
            "beyond the end",
        ),
        (
            "{id: a, ribbon: main, s: 400, y: 0, length: 4, width: 2, present: [{from_s: 9, to_s: 3}]}",
            "period",
        ),
    ] {
        let yaml = road(60.0, 500.0, MIXED, "", object);
        assert!(Simulation::new(&spec(&yaml, &[]), 1, 1).is_err(), "{why}");
    }
    // No capacity or flow factor can be given to an object (ADR-0005).
    let yaml = road(
        60.0,
        500.0,
        MIXED,
        "",
        "{id: a, ribbon: main, s: 400, y: 0, length: 4, width: 2, capacity_factor: 0.7}",
    );
    assert!(Scenario::from_yaml(&yaml).is_err());
}

/// A road that narrows from 10.5 m to 5 m over 40 m, fed more than the
/// narrow part can carry: the queue merges without the kerb pushing anyone
/// into a neighbour (no guard events), and keeps discharging.
const TAPER: &str = "{s: 300, y_min: -5.25, y_max: 5.25}, {s: 340, y_min: -2.5, y_max: 2.5}";

#[test]
fn a_narrowing_road_merges_without_the_guard() {
    let yaml = road(600.0, 5000.0, MIXED, TAPER, "");
    let mut sim = Simulation::new(&spec(&yaml, &[]), 4, 1).unwrap();
    let mut queued = false;
    let mut last_exit = 0.0;
    let mut exited = 0;
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        queued |= (0..a.len())
            .filter(|&i| a.s[i] < 300.0 && a.v[i] < 1.0)
            .count()
            > 20;
        for i in 0..a.len() {
            let (lo, hi) = sim.network().ribbon(0).y_bounds(a.s[i]);
            let half = 0.5 * a.width[i];
            assert!(a.y[i] - half >= lo - 1e-9 && a.y[i] + half <= hi + 1e-9);
        }
        if sim.summary().exited > exited {
            exited = sim.summary().exited;
            last_exit = sim.time();
        }
    }
    assert!(queued, "demand must exceed what the narrow part carries");
    assert!(last_exit > 590.0, "discharge stopped at t = {last_exit}");
    let (_, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    // The narrow part carries at least what one notional lane does.
    assert!(
        kpi(&out, "throughput_veh_h") > 1500.0,
        "{}",
        kpi(&out, "throughput_veh_h")
    );
}

/// Objects and a narrowing road give the same result on any number of threads.
#[test]
fn objects_and_narrowing_are_deterministic() {
    let yaml =
        road(240.0, 3000.0, MIXED, TAPER, PARKED).replace("s: 400, y: 0.0", "s: 200, y: 2.0");
    let spec = spec(&yaml, &[]);
    let run = |threads: usize| {
        let mut sim = Simulation::new(&spec, 11, threads).unwrap();
        sim.set_parallel_min(0);
        sim.run_to_end()
    };
    let one = run(1);
    assert!(one.agent_steps > 100_000, "{one:?}");
    assert_eq!(one, run(4));
}

/// ADR-0012 part 2: traffic does not lock up behind a standing body. With a
/// parked car in the middle of a 10.5 m road at 1,800 veh/h, vehicles that
/// stop behind it pull out (every class can creep sideways), and the stream
/// gets through.
#[test]
fn stopped_vehicles_pull_out_from_behind_a_parked_car() {
    let yaml = road(900.0, 1800.0, MIXED, "", PARKED);
    let mut sim = Simulation::new(&spec(&yaml, &[]), 1, 1).unwrap();
    let mut stopped_behind = std::collections::BTreeSet::new();
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            if a.v[i] < 0.05 && a.s[i] > 380.0 && a.s[i] < 400.0 && a.y[i].abs() < 1.5 {
                stopped_behind.insert(a.id[i]);
            }
        }
    }
    let a = sim.agents();
    let standing = (0..a.len())
        .filter(|&i| a.v[i] < 0.05 && a.s[i] < 400.0)
        .count();
    assert!(
        standing <= 3,
        "{standing} standing behind the object at the end"
    );
    let still_there = (0..a.len())
        .filter(|&i| stopped_behind.contains(&a.id[i]) && a.s[i] < 400.0)
        .count();
    let (_, out) = sim.finish().unwrap();
    assert!(
        kpi(&out, "throughput_veh_h") > 1600.0,
        "{}",
        kpi(&out, "throughput_veh_h")
    );
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
    eprintln!(
        "{} vehicles stopped behind the parked car; {still_there} of them still behind it at the end",
        stopped_behind.len()
    );
    assert!(still_there <= 3);
}

/// Measurement for ADR-0012 (not a check): a parked car in the middle of a
/// 10.5 m road, at four demands, with the register's creep speeds (every
/// class, option A) and with those before the decision (two-wheelers and
/// bicycles only). `cargo test --release -p marg-core --test objects
/// standing_obstacle -- --ignored --nocapture`.
#[test]
#[ignore]
fn standing_obstacle_measurement() {
    let creep: Vec<(String, &str)> = marg_params::VehicleClass::ALL
        .iter()
        .filter(|c| !matches!(c.key(), "two_wheeler" | "bicycle"))
        .map(|c| (format!("interaction.lateral.creep_speed.{}", c.key()), "0"))
        .collect();
    let creep: Vec<(&str, &str)> = creep.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    eprintln!(
        "| Demand (veh/h) | Creep | Throughput (veh/h) | Mean delay (s) | Standing behind the object at the end | Guard events |"
    );
    eprintln!("| --- | --- | --- | --- | --- | --- |");
    for flow in [600.0, 1200.0, 1800.0, 4000.0] {
        for (name, over) in [
            ("two-wheelers and bicycles only", &creep[..]),
            ("register (every class)", &[][..]),
        ] {
            let yaml = road(1800.0, flow, MIXED, "", PARKED);
            let mut sim = Simulation::new(&spec(&yaml, over), 1, 1).unwrap();
            while !sim.finished() {
                sim.step();
            }
            let a = sim.agents();
            let standing = (0..a.len())
                .filter(|&i| a.v[i] < 0.05 && a.s[i] < 400.0)
                .count();
            let (_, out) = sim.finish().unwrap();
            eprintln!(
                "| {flow:.0} | {name} | {:.0} | {:.1} | {standing} | {:.0} |",
                kpi(&out, "throughput_veh_h"),
                kpi(&out, "delay_mean_s"),
                kpi(&out, "collision_guard_events"),
            );
        }
    }
}

/// ADR-0012 part 2, option B (swing-out): cars queue behind a parked car
/// while a second obstacle blocks the way round. The first stops back far
/// enough to steer out. When the blocker goes, every car pulls out moving
/// forward and passes: creep is switched off for cars, so before swing-out
/// they would have stood there for good.
#[test]
fn cars_keep_room_to_swing_out_and_pull_out_moving_forward() {
    let yaml = r#"
name: test
duration_s: 420
network:
  ribbons: [{id: main, polyline: [[0, 0], [700, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}]
  objects:
    - {id: parked, ribbon: main, s: 300, y: 2.5, length: 4.5, width: 1.8, clearance_as: car}
    - {id: blocker, ribbon: main, s: 300, y: -1.0, length: 4.5, width: 3.0, present: [{from_s: 0, to_s: 150}]}
demand: {entries: [{id: west, ribbon: main, flow: [{from_s: 0, veh_per_h: 120}], composition: {hatchback: 1.0}}]}
detectors:
  - {kind: screenline, id: out, ribbon: main, s: 600, interval_s: 60}
kpi: {throughput_detector: out}
"#;
    let over = [
        ("interaction.lateral.creep_speed.car", "0"),
        (
            "demand.entry_lateral_position.car",
            "{dist: constant, value: 0.9}",
        ),
    ];
    let mut sim = Simulation::new(&spec(yaml, &over), 3, 1).unwrap();
    let parked_rear = 300.0 - 2.25;
    let mut furthest = std::collections::BTreeMap::new();
    let mut first_gap = None;
    while !sim.finished() {
        sim.step();
        let a = sim.agents();
        for i in 0..a.len() {
            let e = furthest.entry(a.id[i]).or_insert((sim.time(), a.s[i]));
            e.1 = a.s[i].max(e.1);
        }
        // Just before the blocker goes: the car nearest the parked one.
        if (sim.time() - 149.0).abs() < 0.05 {
            let lead = (0..a.len())
                .filter(|&i| a.s[i] < parked_rear)
                .max_by(|&x, &y| a.s[x].total_cmp(&a.s[y]))
                .expect("a queue behind the parked car");
            assert!(a.v[lead] < 0.05, "{}", a.v[lead]);
            // The sideways shift it still needs to get clear of whichever
            // body is in its way (at zero clearance: a lower bound).
            let (y, half) = (a.y[lead], 0.5 * a.width[lead]);
            let road = (-3.5 + half, 3.5 - half);
            let need = [(2.5, 0.9), (-1.0, 1.5)]
                .iter()
                .filter(|(by, bh)| (y - by).abs() < half + bh)
                .map(|(by, bh)| {
                    let left = (by + bh + half <= road.1).then_some(by + bh + half - y);
                    let right = (by - bh - half >= road.0).then_some(y - (by - bh - half));
                    left.into_iter().chain(right).fold(f64::INFINITY, f64::min)
                })
                .fold(0.0, f64::max);
            first_gap = Some((
                parked_rear - a.s[lead] - 0.5 * a.length[lead],
                a.standstill_gap[lead],
                need,
            ));
        }
    }
    let (gap, s0, need) = first_gap.unwrap();
    let room = need / 0.2_f64.tan();
    eprintln!(
        "stood {gap:.1} m behind the obstacles (standstill gap {s0:.1} m, {need:.2} m to move over: {room:.1} m of room)"
    );
    assert!(
        need > 0.05 && gap > s0 + room - 0.25,
        "{gap:.2} {s0:.2} {need:.2}"
    );
    let a = sim.agents();
    for i in 0..a.len() {
        eprintln!(
            "END id {} s {:.1} y {:.2} v {:.2} target {:.2} acc {:.2} head {:.3} lead {}",
            a.id[i] % 1000,
            a.s[i],
            a.y[i],
            a.v[i],
            a.y_target[i],
            a.acc[i],
            a.heading_tan[i],
            a.decision_leader[i] % 100000
        );
    }
    let queued: Vec<_> = furthest.values().filter(|(t0, _)| *t0 < 140.0).collect();
    assert!(queued.len() >= 3, "{}", queued.len());
    for (t0, s) in queued {
        assert!(
            *s > 400.0,
            "a car that entered at {t0:.0} s got only to {s:.0} m"
        );
    }
    let (_, out) = sim.finish().unwrap();
    assert_eq!(kpi(&out, "collision_guard_events"), 0.0);
}
