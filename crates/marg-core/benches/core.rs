//! Core benchmarks. The reference corridor bench arrives in phase P2; until
//! then this measures RNG throughput and the skeleton step loop.

use criterion::{Criterion, criterion_group, criterion_main};
use marg_core::Simulation;
use marg_core::rng::{StreamKind, StreamRng};
use marg_params::{Register, Scenario, resolve};
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
    let yaml = r#"
name: bench
duration_s: 300
network: {ribbons: [{id: r, polyline: [[0, 0], [1000, 0]], width: [{s: 0, y_min: -3.5, y_max: 3.5}]}]}
demand: {entries: [{id: e, ribbon: r, flow: [{from_s: 0, veh_per_h: 3600}], composition: {hatchback: 1.0}}]}
"#;
    let scenario = Scenario::from_yaml(yaml).unwrap();
    let spec = resolve(
        &Register::default(),
        &scenario,
        &Default::default(),
        Default::default(),
    )
    .unwrap();
    c.bench_function("skeleton_300s", |b| {
        b.iter(|| {
            let mut sim = Simulation::new(&spec, 1, 1).unwrap();
            black_box(sim.run_to_end())
        })
    });
}

criterion_group!(benches, rng, skeleton);
criterion_main!(benches);
