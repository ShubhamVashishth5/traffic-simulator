//! Diagnostic harness: run a scenario to a time and print what holds each
//! stalled vehicle (or the vehicles named), for diagnosing jams from the
//! state the model sees. Compiled once; what to inspect comes from the
//! arguments.
//!
//!     cargo run --release -p marg-core --example diag -- <scenario.yaml> <seed> <t> [agent ids...]
//!
//! With no ids it lists every vehicle slower than 0.1 m/s.

use indexmap::IndexMap;
use marg_core::Simulation;
use marg_params::{Register, ResolveOptions, Scenario, resolve};
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 4 {
        eprintln!("usage: diag <scenario.yaml> <seed> <t> [ids...]");
        std::process::exit(2);
    }
    let text = std::fs::read_to_string(&args[1]).expect("scenario");
    let seed: u64 = args[2].parse().expect("seed");
    let t_stop: f64 = args[3].parse().expect("t");
    let ids: Vec<u64> = args[4..].iter().map(|x| x.parse().expect("id")).collect();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let register = Register::load_dir(&root.join("params/base")).expect("register");
    let scenario = Scenario::from_yaml(&text).expect("yaml");
    let spec = resolve(
        &register,
        &scenario,
        &IndexMap::new(),
        ResolveOptions {
            allow_fix_override: true,
        },
    )
    .expect("resolve");
    let mut sim = Simulation::new(&spec, seed, 1).expect("sim");
    while sim.time() < t_stop - 1e-9 && !sim.finished() {
        sim.step();
    }
    let net = sim.network();
    let a = sim.agents();
    let by_id = |id: u64| (0..a.len()).find(|&k| a.id[k] == id);
    let pick: Vec<usize> = if ids.is_empty() {
        (0..a.len())
            .filter(|&i| !a.exited[i] && a.v[i] < 0.1)
            .collect()
    } else {
        ids.iter().filter_map(|&id| by_id(id)).collect()
    };
    println!(
        "t = {:.1}, {} agents, {} shown",
        sim.time(),
        a.len(),
        pick.len()
    );
    for &i in &pick {
        let rib = net.ribbon(a.ribbon[i] as usize);
        println!(
            "\n#{} id {} class {} on {} s {:.2}/{:.2} y {:.2} v {:.2} acc {:.2} L {:.1} W {:.1}",
            i,
            a.id[i],
            a.class[i],
            rib.id,
            a.s[i],
            rib.length(),
            a.y[i],
            a.v[i],
            a.acc[i],
            a.length[i],
            a.width[i]
        );
        println!(
            "   turn ({}, {}) line_held {} gate_open {} ticket {:.1} queued {} split {} aware {} land_y {:.2} y_target {:.2}",
            a.turn_junction[i],
            a.turn_out[i],
            a.line_held[i],
            a.gate_open[i],
            a.box_ticket[i],
            a.queued[i],
            a.split_state[i],
            a.aware[i],
            a.land_y[i],
            a.y_target[i]
        );
        if let Some(out) = sim.move_out(i) {
            let lead = by_id(out.leader).map_or(String::from("-"), |k| {
                format!(
                    "{} on {} s {:.2} y {:.2} v {:.2}",
                    a.id[k],
                    net.ribbon(a.ribbon[k] as usize).id,
                    a.s[k],
                    a.y[k],
                    a.v[k]
                )
            });
            println!(
                "   out acc {:.2} vy {:.2} y [{:.2}, {:.2}] leader {} | junction held {} gap {:.2} need {:.2} waiting {}",
                out.acc,
                out.vy,
                out.y_lo,
                out.y_hi,
                lead,
                out.junction.held,
                out.junction.gap,
                out.junction.needed,
                out.junction.waiting
            );
        }
        for y in sim.junctions().arbiter.yields_of(i) {
            let goer = if (y.goer as usize) < a.len() {
                format!(
                    "{} on {} s {:.2} v {:.2}",
                    a.id[y.goer as usize],
                    net.ribbon(a.ribbon[y.goer as usize] as usize).id,
                    a.s[y.goer as usize],
                    a.v[y.goer as usize]
                )
            } else {
                "stream".into()
            };
            println!("   yields to {goer}, gap {:.2}", y.gap);
        }
        for x in sim.interactions(i).iter().take(6) {
            let k = x.j as usize;
            println!(
                "   near {} on {} s {:.2} y {:.2} v {:.2}: ds {:.2} gap {:.2} lat {:.2} w {:.2} acc {:.2}",
                a.id[k],
                net.ribbon(a.ribbon[k] as usize).id,
                a.s[k],
                a.y[k],
                a.v[k],
                x.ds,
                x.gap_s,
                x.lateral_gap,
                x.weight,
                x.leader_accel
            );
        }
    }
}
