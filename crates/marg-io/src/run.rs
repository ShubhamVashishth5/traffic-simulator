//! Run one scenario and write its run folder.

use crate::manifest::{RUN_FORMAT_VERSION, RunManifest, RunStatus};
use crate::outputs::{ParquetTrajectorySink, write_outputs};
use crate::params_table::params_batch;
use crate::table::{TableError, write_batch};
use indexmap::IndexMap;
use marg_core::Simulation;
use marg_params::{ParamValue, Register, ResolveOptions, ResolvedSpec, Scenario, resolve};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Errors from running a scenario.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("scenario YAML: {0}")]
    Yaml(#[from] serde_norway::Error),
    #[error("register: {0}")]
    Register(#[from] marg_params::register::RegisterError),
    #[error("parameters: {0}")]
    Resolve(#[from] marg_params::ResolveError),
    #[error(transparent)]
    Sim(#[from] marg_core::SimError),
    #[error("writing outputs: {0}")]
    Io(#[from] std::io::Error),
    #[error("writing tables: {0}")]
    Table(#[from] TableError),
    #[error("building tables: {0}")]
    Arrow(#[from] arrow::error::ArrowError),
    #[error("writing trajectories: {0}")]
    Trajectories(String),
    #[error("no parameter register found: set MARG_PARAMS_DIR or run from the repository")]
    NoRegister,
}

/// How to run.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub seed: u64,
    /// Worker threads (0 = all cores).
    pub threads: usize,
    /// Register directory; `None` = [`find_params_dir`].
    pub params_dir: Option<PathBuf>,
    /// Experiment-level overrides, in register units.
    pub overrides: IndexMap<String, ParamValue>,
    pub allow_fix_override: bool,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            threads: 1,
            params_dir: None,
            overrides: IndexMap::new(),
            allow_fix_override: false,
        }
    }
}

/// The register directory: `$MARG_PARAMS_DIR`, else the first `params/base`
/// found in the current directory or its ancestors.
pub fn find_params_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("MARG_PARAMS_DIR") {
        return Some(PathBuf::from(dir));
    }
    let mut dir = std::env::current_dir().ok()?;
    loop {
        let candidate = dir.join("params/base");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Load the register and resolve `scenario` against it.
pub fn resolve_scenario(scenario: &Scenario, opts: &RunOptions) -> Result<ResolvedSpec, RunError> {
    let dir = opts
        .params_dir
        .clone()
        .or_else(find_params_dir)
        .ok_or(RunError::NoRegister)?;
    let register = Register::load_dir(&dir)?;
    Ok(resolve(
        &register,
        scenario,
        &opts.overrides,
        ResolveOptions {
            allow_fix_override: opts.allow_fix_override,
        },
    )?)
}

/// Parse `yaml`, run it to the end and write the run folder to `out`.
pub fn run_scenario_text(
    yaml: &str,
    opts: &RunOptions,
    out: &Path,
) -> Result<RunManifest, RunError> {
    let scenario = Scenario::from_yaml(yaml)?;
    run_scenario(&scenario, opts, out)
}

/// Resolve and run `scenario`, writing `manifest.json`, `spec.resolved.yaml`
/// and `params.parquet` to `out`.
pub fn run_scenario(
    scenario: &Scenario,
    opts: &RunOptions,
    out: &Path,
) -> Result<RunManifest, RunError> {
    let spec = resolve_scenario(scenario, opts)?;
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("spec.resolved.yaml"), spec.to_yaml()?)?;
    write_batch(&out.join("params.parquet"), &params_batch(&spec)?)?;

    let run_hash = spec.run_hash(opts.seed, marg_core::VERSION);
    let start = Instant::now();
    let mut sim = Simulation::new(&spec, opts.seed, opts.threads)?;
    if spec.scenario.outputs.trajectories.is_some() {
        let ribbons = spec
            .scenario
            .network
            .ribbons
            .iter()
            .map(|r| r.id.clone())
            .collect();
        sim.set_trajectory_sink(Box::new(ParquetTrajectorySink::new(
            out.join("trajectories.parquet"),
            ribbons,
        )?));
    }
    sim.run_to_end();
    let (summary, outputs) = sim.finish().map_err(RunError::Trajectories)?;
    write_outputs(out, &outputs, &run_hash, opts.seed)?;
    let (marg_version, git_sha, git_dirty, build_profile, host) = RunManifest::build_info();
    let manifest = RunManifest {
        run_format_version: RUN_FORMAT_VERSION,
        run_hash,
        scenario: scenario.name.clone(),
        seed: opts.seed,
        rng_scheme: "philox4x32-10/v1".into(),
        marg_version,
        git_sha,
        git_dirty,
        build_profile,
        threads: effective_threads(opts.threads),
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

fn effective_threads(requested: usize) -> usize {
    if requested == 0 {
        std::thread::available_parallelism().map_or(1, usize::from)
    } else {
        requested
    }
}
