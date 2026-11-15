//! Personas and the trait space (plan P8.2, ADR-0018 parts 5–6).

mod common;
use common::*;
use marg_core::demand::Draws;
use marg_core::params::CoreParams;
use marg_core::personas::NO_PERSONA;
use marg_core::{AgentRecord, Simulation};
use marg_params::{Persona, Trait, VehicleType};

/// Every share set: the population spec of these tests.
const SHARES: &[(&str, &str)] = &[
    ("personas.shares.average_commuter", "0.4"),
    ("personas.shares.rushed_driver", "0.15"),
    ("personas.shares.delivery_rider", "0.2"),
    ("personas.shares.bike_taxi_rider", "0.05"),
    ("personas.shares.e_rickshaw_driver", "0.05"),
    ("personas.shares.auto_rickshaw_driver", "0.05"),
    ("personas.shares.bus_driver", "0.03"),
    ("personas.shares.truck_lcv_driver", "0.03"),
    ("personas.shares.cautious_driver", "0.1"),
    ("personas.shares.cyclist", "0.02"),
];

fn params(overrides: &[(&str, &str)]) -> CoreParams {
    let yaml = corridor(500.0, 60.0, 600.0, "{motorcycle: 1.0}");
    CoreParams::from_spec(&spec(&yaml, overrides)).unwrap()
}

fn run(yaml: &str, overrides: &[(&str, &str)], seed: u64, threads: usize) -> Vec<AgentRecord> {
    let mut sim = Simulation::new(&spec(yaml, overrides), seed, threads).unwrap();
    sim.run_to_end();
    sim.finish().unwrap().1.agents
}

fn mean_sd(x: &[f64]) -> (f64, f64) {
    let n = x.len() as f64;
    let m = x.iter().sum::<f64>() / n;
    let v = x.iter().map(|v| (v - m) * (v - m)).sum::<f64>() / (n - 1.0);
    (m, v.sqrt())
}

/// Done-when 1: the realised trait moments match the inputs. Motorcycles
/// draw among the four two-wheeler personas in their renormalised shares;
/// within each persona every trait has mean `shift` (in SDs) and SD 1.
#[test]
fn realised_trait_moments_match_the_inputs() {
    let p = params(SHARES);
    let n = 40_000u64;
    let draws: Vec<Draws> = (0..n)
        .map(|id| Draws::sample(9, id, VehicleType::Motorcycle, &p))
        .collect();
    let eligible = Persona::eligible(VehicleType::Motorcycle.class());
    let weights = [0.4, 0.15, 0.2, 0.05];
    let total: f64 = weights.iter().sum();
    for (k, persona) in eligible.iter().enumerate() {
        let mine: Vec<&Draws> = draws
            .iter()
            .filter(|d| d.persona == persona.index() as u8)
            .collect();
        let share = mine.len() as f64 / n as f64;
        let want = weights[k] / total;
        let se = (want * (1.0 - want) / n as f64).sqrt();
        assert!(
            (share - want).abs() < 4.0 * se,
            "{}: share {share:.4} vs {want:.4}",
            persona.key()
        );
        for t in Trait::ALL {
            let x: Vec<f64> = mine.iter().map(|d| d.traits[t.index()]).collect();
            let (m, sd) = mean_sd(&x);
            let shift = p.personas.shift[t.index()][persona.index()];
            let se = 1.0 / (x.len() as f64).sqrt();
            assert!(
                (m - shift).abs() < 4.0 * se,
                "{} {}: mean {m:.3} vs {shift}",
                persona.key(),
                t.key()
            );
            assert!(
                (sd - 1.0).abs() < 0.05,
                "{} {}: sd {sd:.3}",
                persona.key(),
                t.key()
            );
        }
        // κ in the draws is the trait vector's κ.
        assert!(mine.iter().all(|d| d.kappa == d.traits[0]));
    }
    // The delivery rider is the one with very high time pressure.
    assert_eq!(p.personas.shift[1][Persona::DeliveryRider.index()], 1.5);
    // Personas off: no label, the register's trait distributions.
    let off = params(&[]);
    let d = Draws::sample(9, 1, VehicleType::Motorcycle, &off);
    assert_eq!(d.persona, NO_PERSONA);
}

/// A κ link keeps the register's distribution as the marginal (the copula of
/// ADR-0018, K2) and ties the parameter to κ with the asked rank
/// correlation; personas with a higher κ then draw higher up it.
#[test]
fn a_kappa_link_keeps_the_marginal() {
    let n = 20_000u64;
    let off = params(&[]);
    let on = params(&[("personas.links.free_flow_speed", "0.8")]);
    let vt = VehicleType::Hatchback;
    let mut a: Vec<f64> = (0..n).map(|id| Draws::sample(4, id, vt, &off).v0).collect();
    let linked: Vec<Draws> = (0..n).map(|id| Draws::sample(4, id, vt, &on)).collect();
    let mut b: Vec<f64> = linked.iter().map(|d| d.v0).collect();
    // Same marginal: two-sample Kolmogorov–Smirnov distance.
    a.sort_by(f64::total_cmp);
    b.sort_by(f64::total_cmp);
    let ks = a
        .iter()
        .map(|x| {
            let fa = a.partition_point(|y| y <= x) as f64 / n as f64;
            let fb = b.partition_point(|y| y <= x) as f64 / n as f64;
            (fa - fb).abs()
        })
        .fold(0.0, f64::max);
    assert!(ks < 0.02, "KS {ks}");
    // Rank correlation with κ: 6/π · asin(r/2) for a Gaussian copula.
    let rank = |x: Vec<f64>| -> Vec<f64> {
        let mut idx: Vec<usize> = (0..x.len()).collect();
        idx.sort_by(|&i, &j| x[i].total_cmp(&x[j]));
        let mut r = vec![0.0; x.len()];
        for (k, &i) in idx.iter().enumerate() {
            r[i] = k as f64;
        }
        r
    };
    let rk = rank(linked.iter().map(|d| d.kappa).collect());
    let rv = rank(linked.iter().map(|d| d.v0).collect());
    let m = (n as f64 - 1.0) / 2.0;
    let cov: f64 = rk.iter().zip(&rv).map(|(x, y)| (x - m) * (y - m)).sum();
    let var: f64 = rk.iter().map(|x| (x - m) * (x - m)).sum();
    let spearman = cov / var;
    let want = 6.0 / std::f64::consts::PI * (0.4f64).asin();
    assert!(
        (spearman - want).abs() < 0.02,
        "Spearman {spearman} vs {want}"
    );
    // Unlinked parameters do not move; the agent's other draws are its own.
    assert_eq!(
        Draws::sample(4, 7, vt, &off).time_gap,
        Draws::sample(4, 7, vt, &on).time_gap
    );
    // A persona with a higher κ is faster when v0 is linked.
    let both: Vec<(&str, &str)> = SHARES
        .iter()
        .copied()
        .chain([("personas.links.free_flow_speed", "0.8")])
        .collect();
    let p = params(&both);
    let v0_of = |persona: Persona| {
        let x: Vec<f64> = (0..n)
            .map(|id| Draws::sample(4, id, vt, &p))
            .filter(|d| d.persona == persona.index() as u8)
            .map(|d| d.v0)
            .collect();
        mean_sd(&x).0
    };
    assert!(v0_of(Persona::RushedDriver) > v0_of(Persona::AverageCommuter));
    assert!(v0_of(Persona::CautiousDriver) < v0_of(Persona::AverageCommuter));
}

/// Done-when 2, in a run: within a trip κ follows the OU process about its
/// entry value. `E[(κ_end − κ)²] = sd²·(1 − exp(−2θτ))` over trips of
/// length τ.
#[test]
fn kappa_follows_the_ou_process_within_trips() {
    let ou = [
        ("personas.traits.within_trip.reversion_rate", "30"), // 1/h: θ = 1/120 s
        ("personas.traits.within_trip.sd", "0.5"),
    ];
    let yaml = corridor(1000.0, 1200.0, 1500.0, "{motorcycle: 0.5, hatchback: 0.5}");
    let rows = run(&yaml, &ou, 3, 1);
    let theta = 30.0 / 3600.0;
    let (mut seen, mut expected, mut n) = (0.0, 0.0, 0);
    for r in rows.iter().filter(|r| r.exited) {
        let d = r.kappa_end - r.kappa;
        seen += d * d;
        expected += 0.25 * (1.0 - (-2.0 * theta * r.travel_time).exp());
        n += 1;
    }
    assert!(n > 300, "{n} trips");
    let ratio = seen / expected;
    assert!(
        (ratio - 1.0).abs() < 0.15,
        "variance ratio {ratio} over {n} trips"
    );
    // Without the process κ never moves.
    let still = run(&yaml, &[], 3, 1);
    assert!(still.iter().all(|r| r.kappa_end == r.kappa));
}

/// The jump on delay comes once, when the delay first passes the threshold.
#[test]
fn kappa_jumps_once_when_delay_builds_up() {
    let jump = [
        ("personas.traits.within_trip.delay_jump", "0.75"),
        ("personas.traits.within_trip.delay_threshold", "5"),
    ];
    // A dense road, so most vehicles lose more than 5 s.
    let yaml = corridor(800.0, 600.0, 3000.0, "{motorcycle: 0.4, hatchback: 0.6}");
    let rows = run(&yaml, &jump, 2, 1);
    let (mut jumped, mut not) = (0, 0);
    for r in rows.iter().filter(|r| r.exited) {
        let d = r.kappa_end - r.kappa;
        if d != 0.0 {
            assert!((d - 0.75).abs() < 1e-12, "jump {d}");
            assert!(r.delay > 5.0, "jumped with delay {}", r.delay);
            jumped += 1;
        } else {
            not += 1;
        }
    }
    assert!(
        jumped > 50 && jumped > not / 4,
        "{jumped} jumped, {not} not"
    );
}

/// Distraction episodes: phone users are the asked share; a user is
/// distracted for `duration / (1/rate + duration)` of the time; while
/// distracted it keeps its lateral target (the user's decision, ADR-0018).
#[test]
fn distracted_drivers_keep_their_lateral_target() {
    let yaml = corridor(
        1000.0,
        900.0,
        1500.0,
        "{e_rickshaw: 0.3, motorcycle: 0.4, hatchback: 0.3}",
    );
    // A third of the time distracted: off 10 s on average, then 5 s on.
    let cycle = [
        ("personas.traits.phone_use_share", "0.3"),
        ("personas.distraction.episode_rate", "360"),
        (
            "personas.distraction.episode_duration",
            "{dist: constant, value: 5}",
        ),
    ];
    let rows = run(&yaml, &cycle, 5, 1);
    let users = rows.iter().filter(|r| r.phone_user).count() as f64 / rows.len() as f64;
    assert!((users - 0.3).abs() < 0.05, "phone users {users}");
    let done: Vec<_> = rows.iter().filter(|r| r.exited).collect();
    let (on, total): (f64, f64) = done
        .iter()
        .filter(|r| r.phone_user)
        .fold((0.0, 0.0), |(a, b), r| {
            (a + r.distracted_time, b + r.travel_time)
        });
    let share = on / total;
    assert!((0.27..0.36).contains(&share), "distracted {share}");
    assert!(
        done.iter()
            .all(|r| r.phone_user || r.distraction_episodes == 0)
    );
    // Everyone distracted from the start and for good: nobody re-decides.
    let always = [
        ("personas.traits.phone_use_share", "1.0"),
        ("personas.distraction.episode_rate", "3600"),
        (
            "personas.distraction.episode_duration",
            "{dist: constant, value: 600}",
        ),
    ];
    let attentive = run(&yaml, &[], 5, 1);
    let distracted = run(&yaml, &always, 5, 1);
    let per_agent = |rows: &[AgentRecord]| {
        rows.iter().map(|r| f64::from(r.lateral_moves)).sum::<f64>() / rows.len() as f64
    };
    // Attentive drivers re-decide often; distracted ones only in the first
    // second or so, before their first episode (an off period averages 1 s).
    assert!(per_agent(&attentive) > 10.0, "{}", per_agent(&attentive));
    assert!(per_agent(&distracted) < 1.5, "{}", per_agent(&distracted));
    let (on, total) = distracted
        .iter()
        .filter(|r| r.exited)
        .fold((0.0, 0.0), |(a, b), r| {
            (a + r.distracted_time, b + r.travel_time)
        });
    assert!(on / total > 0.95, "distracted for {}", on / total);
}

/// Done-when 3: personas appear in the agent records, deterministically for
/// any number of threads, with every within-trip process on.
#[test]
fn personas_appear_in_the_records_for_any_thread_count() {
    let all: Vec<(&str, &str)> = SHARES
        .iter()
        .copied()
        .chain([
            ("personas.links.free_flow_speed", "0.5"),
            ("personas.links.time_gap", "-0.5"),
            ("personas.links.critical_gap", "-0.4"),
            ("personas.traits.within_trip.reversion_rate", "30"),
            ("personas.traits.within_trip.sd", "0.5"),
            ("personas.traits.within_trip.delay_jump", "0.5"),
            ("personas.traits.within_trip.delay_threshold", "20"),
            ("personas.traits.phone_use_share", "0.2"),
            ("personas.distraction.episode_rate", "120"),
            (
                "personas.distraction.episode_duration",
                "{dist: log_normal_median, median: 4, sigma: 0.5}",
            ),
        ])
        .collect();
    let yaml = corridor(
        800.0,
        400.0,
        2500.0,
        "{motorcycle: 0.4, hatchback: 0.3, e_rickshaw: 0.1, bus: 0.1, bicycle: 0.1}",
    );
    let one = run(&yaml, &all, 8, 1);
    let four = run(&yaml, &all, 8, 4);
    assert_eq!(one, four);
    let label = |r: &AgentRecord| r.persona.unwrap();
    for r in &one {
        let p = Persona::from_key(label(r)).unwrap();
        let class = VehicleType::from_key(r.vehicle_type).unwrap().class();
        assert!(
            Persona::eligible(class).contains(&p),
            "{} drives {}",
            p.key(),
            r.vehicle_type
        );
    }
    assert!(one.iter().any(|r| r.persona == Some("delivery_rider")));
    assert!(
        one.iter()
            .filter(|r| r.class == "bus")
            .all(|r| label(r) == "bus_driver")
    );
    // Off: no labels.
    assert!(run(&yaml, &[], 8, 1).iter().all(|r| r.persona.is_none()));
}

/// Some shares set and others not: refused, naming the missing ones.
#[test]
fn a_partial_population_is_refused() {
    let yaml = corridor(500.0, 60.0, 600.0, "{motorcycle: 1.0}");
    let err = Simulation::new(
        &spec(&yaml, &[("personas.shares.delivery_rider", "0.3")]),
        1,
        1,
    )
    .err()
    .unwrap()
    .to_string();
    assert!(
        err.contains("average_commuter") && err.contains("cyclist"),
        "{err}"
    );
}
