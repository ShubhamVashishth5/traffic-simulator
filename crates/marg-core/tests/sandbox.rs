//! Sandbox S3: every shipped scenario builds and runs with the `sandbox`
//! preset, which fills what the guide leaves unset with made-up values.

mod common;
use common::*;
use indexmap::IndexMap;
use marg_core::Simulation;
use marg_params::{ResolveOptions, Scenario, resolve};

#[test]
fn every_shipped_scenario_runs_with_the_sandbox_preset() {
    let reg = register();
    let mut names: Vec<_> = std::fs::read_dir(repo().join("scenarios"))
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "yaml"))
        .collect();
    names.sort();
    assert!(names.len() >= 17);
    for path in names {
        let mut sc = Scenario::from_yaml(&std::fs::read_to_string(&path).unwrap()).unwrap();
        sc.presets = vec!["sandbox".into()];
        // A minute is enough: parameters are read when the simulation is built
        // and when each mechanism first acts.
        sc.duration_s = sc.duration_s.min(60.0);
        sc.warmup_s = 0.0;
        let spec = resolve(&reg, &sc, &IndexMap::new(), ResolveOptions::default())
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut sim =
            Simulation::new(&spec, 1, 1).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        sim.run_to_end();
        assert!(sim.summary().spawned > 0, "{}", path.display());
    }
}
