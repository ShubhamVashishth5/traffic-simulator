//! Run one scenario and write its run folder.

use crate::manifest::{RUN_FORMAT_VERSION, RunManifest, RunStatus};
use marg_core::Simulation;
use marg_params::Scenario;
use std::path::Path;
use std::time::Instant;

/// Errors from running a scenario.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("scenario YAML: {0}")]
    Yaml(#[from] serde_norway::Error),
    #[error(transparent)]
    Sim(#[from] marg_core::SimError),
    #[error("writing outputs: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialising: {0}")]
    Json(#[from] serde_json::Error),
}

/// The run's identity: blake3 of the canonical spec JSON, the seed and the
/// simulator version.
pub fn run_hash(spec: &Scenario, seed: u64) -> Result<String, serde_json::Error> {
    let mut h = blake3::Hasher::new();
    h.update(serde_json::to_string(spec)?.as_bytes());
    h.update(&seed.to_le_bytes());
    h.update(marg_core::VERSION.as_bytes());
    Ok(h.finalize().to_hex()[..16].to_string())
}

/// Parse `yaml`, run it to the end and write `out/manifest.json`.
pub fn run_scenario_text(
    yaml: &str,
    seed: u64,
    threads: usize,
    out: &Path,
) -> Result<RunManifest, RunError> {
    let spec = Scenario::from_yaml(yaml)?;
    run_scenario(spec, seed, threads, out)
}

/// Run `spec` to the end and write `out/manifest.json`.
pub fn run_scenario(
    spec: Scenario,
    seed: u64,
    threads: usize,
    out: &Path,
) -> Result<RunManifest, RunError> {
    let hash = run_hash(&spec, seed)?;
    let name = spec.name.clone();
    let start = Instant::now();
    let mut sim = Simulation::new(spec, seed, threads)?;
    let summary = sim.run_to_end();
    let (marg_version, git_sha, git_dirty, build_profile, host) = RunManifest::build_info();
    let manifest = RunManifest {
        run_format_version: RUN_FORMAT_VERSION,
        run_hash: hash,
        scenario: name,
        seed,
        rng_scheme: "philox4x32-10/v1".into(),
        marg_version,
        git_sha,
        git_dirty,
        build_profile,
        threads: rayon_threads(threads),
        host,
        wall_time_s: start.elapsed().as_secs_f64(),
        status: RunStatus::Ok,
        output_hash: summary.output_hash,
        steps: summary.steps,
        agent_steps: summary.agent_steps,
    };
    manifest.write(out)?;
    Ok(manifest)
}

fn rayon_threads(requested: usize) -> usize {
    if requested == 0 {
        std::thread::available_parallelism().map_or(1, usize::from)
    } else {
        requested
    }
}
