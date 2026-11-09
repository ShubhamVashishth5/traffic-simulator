//! Helpers shared by the integration tests.
#![allow(dead_code)]

use indexmap::IndexMap;
use marg_params::{ParamValue, Register, ResolveOptions, ResolvedSpec, Scenario, resolve};
use std::path::{Path, PathBuf};

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

pub fn register() -> Register {
    Register::load_dir(&repo().join("params/base")).unwrap()
}

/// Resolve a scenario (YAML text) against the shipped register, with
/// experiment overrides given as `(path, yaml value)`.
pub fn spec(yaml: &str, overrides: &[(&str, &str)]) -> ResolvedSpec {
    let scenario = Scenario::from_yaml(yaml).unwrap();
    let mut exp = IndexMap::new();
    for (k, v) in overrides {
        exp.insert(
            k.to_string(),
            serde_norway::from_str::<ParamValue>(v).unwrap(),
        );
    }
    resolve(
        &register(),
        &scenario,
        &exp,
        ResolveOptions {
            allow_fix_override: true,
        },
    )
    .unwrap()
}

pub fn scenario_file(name: &str) -> String {
    std::fs::read_to_string(repo().join("scenarios").join(name)).unwrap()
}

/// A straight corridor of `length` m and width 10.5 m with one entry.
pub fn corridor(length: f64, duration: f64, flow: f64, composition: &str) -> String {
    format!(
        r#"
name: test
duration_s: {duration}
network: {{ribbons: [{{id: main, polyline: [[0, 0], [{length}, 0]], width: [{{s: 0, y_min: -5.25, y_max: 5.25}}]}}]}}
demand: {{entries: [{{id: west, ribbon: main, flow: [{{from_s: 0, veh_per_h: {flow}}}], composition: {composition}}}]}}
detectors:
  - {{kind: screenline, id: mid, ribbon: main, s: {mid}, interval_s: 60}}
kpi: {{throughput_detector: mid}}
"#,
        mid = length / 2.0
    )
}
