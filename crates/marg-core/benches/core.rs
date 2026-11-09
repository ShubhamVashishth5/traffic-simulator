//! Core benchmarks. The reference corridor bench arrives in phase P2; until
//! then this measures RNG throughput and the skeleton step loop.

use criterion::{Criterion, criterion_group, criterion_main};
use marg_core::Simulation;
use marg_core::rng::{StreamKind, StreamRng};
use marg_params::Scenario;
use std::hint::black_box;

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

fn skeleton(c: &mut Criterion) {
    let spec = Scenario {
        name: "bench".into(),
        description: String::new(),
        duration_s: 300.0,
        dt_s: 0.1,
        toy_arrivals_per_h: 3600.0,
    };
    c.bench_function("skeleton_300s", |b| {
        b.iter(|| {
            let mut sim = Simulation::new(spec.clone(), 1, 1).unwrap();
            black_box(sim.run_to_end())
        })
    });
}

criterion_group!(benches, rng, skeleton);
criterion_main!(benches);
