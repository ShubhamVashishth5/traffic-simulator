//! Core benchmarks (plan P0.7, P2.10). `scripts/bench-compare.sh` compares
//! them between git revisions on one machine.

use criterion::{Criterion, criterion_group, criterion_main};
use indexmap::IndexMap;
use marg_core::Simulation;
use marg_core::rng::{StreamKind, StreamRng};
use marg_params::{Register, ResolveOptions, ResolvedSpec, Scenario, resolve};
use std::hint::black_box;
use std::path::Path;

fn spec(file: &str, duration: f64) -> ResolvedSpec {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let reg = Register::load_dir(&root.join("params/base")).unwrap();
    let mut sc = Scenario::from_yaml(&std::fs::read_to_string(root.join(file)).unwrap()).unwrap();
    sc.duration_s = duration;
    sc.warmup_s = 0.0;
    sc.outputs.trajectories = None;
    resolve(&reg, &sc, &IndexMap::new(), ResolveOptions::default()).unwrap()
}

fn rng(c: &mut Criterion) {
    c.bench_function("philox_next_u64_x1000", |b| {
        let mut r = StreamRng::new(1, StreamKind::Decisions, 1);
        b.iter(|| {
            let mut acc = 0u64;
            for _ in 0..1000 {
                acc ^= r.next_u64();
            }
            black_box(acc)
        })
    });
}

fn corridor(c: &mut Criterion) {
    let mut g = c.benchmark_group("corridor");
    g.sample_size(10);
    let reference = spec("scenarios/corridor_2km.yaml", 600.0);
    g.bench_function("reference_2km_600s", |b| {
        b.iter(|| black_box(Simulation::new(&reference, 1, 1).unwrap().run_to_end()))
    });
    // Warm a dense corridor up once, then time 60 s of stepping from that state.
    let dense = spec("benches/dense_corridor.yaml", 1800.0);
    let mut warm = Simulation::new(&dense, 1, 1).unwrap();
    for _ in 0..9000 {
        warm.step();
    }
    eprintln!(
        "dense corridor: {} agents in the network after 900 s",
        warm.agent_count()
    );
    g.bench_function("dense_2km_step_x100", |b| {
        b.iter(|| {
            for _ in 0..100 {
                warm.step();
            }
        })
    });
    g.finish();
}

criterion_group!(benches, rng, corridor);
criterion_main!(benches);
