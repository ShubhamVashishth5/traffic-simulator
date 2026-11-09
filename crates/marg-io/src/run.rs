//! Run one scenario and write its run folder.

use crate::manifest::{RNG_SCHEME, RUN_FORMAT_VERSION, RunManifest, RunMeta, RunStatus};
use crate::outputs::{ParquetTrajectorySink, write_outputs};
use crate::params_table::params_batch;
use crate::table::{TableError, write_batch};
use indexmap::IndexMap;
use marg_core::Simulation;
use marg_params::{
    Calibration, ParamValue, Register, ResolveOptions, ResolvedSpec, Scenario, resolve_layers,
};
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
    #[error("calibration: {0}")]
    Calibration(#[from] marg_params::CalibrationError),
    #[error("no parameter register found: set MARG_PARAMS_DIR or run from the repository")]
    NoRegister,
    #[error("live session: {0}")]
    Live(String),
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
    /// Directory of `stage_<k>.yaml` files; frozen stages are applied as the
    /// `calibration` layer. `None` = no calibration layer.
    pub calibration_dir: Option<PathBuf>,
    /// Experiment, point, arm and replication, recorded in the manifest.
    pub meta: Option<RunMeta>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            seed: 1,
            threads: 1,
            params_dir: None,
            overrides: IndexMap::new(),
            allow_fix_override: false,
            calibration_dir: None,
            meta: None,
        }
    }
}

/// The register and calibration stages a set of runs resolves against,
/// loaded once.
#[derive(Debug, Clone)]
pub struct Inputs {
    pub register: Register,
    pub calibration: Calibration,
}

impl Inputs {
    /// Load the register (`params_dir`, else [`find_params_dir`]) and the
    /// frozen calibration stages in `calibration_dir`, if given.
    pub fn load(
        params_dir: Option<&Path>,
        calibration_dir: Option<&Path>,
    ) -> Result<Self, RunError> {
        let dir = params_dir
            .map(Path::to_path_buf)
            .or_else(find_params_dir)
            .ok_or(RunError::NoRegister)?;
        let register = Register::load_dir(&dir)?;
        let calibration = match calibration_dir {
            Some(c) => Calibration::load_dir(c, &register)?,
            None => Calibration::default(),
        };
        Ok(Self {
            register,
            calibration,
        })
    }

    /// Resolve `scenario` with experiment `overrides`.
    pub fn resolve(
        &self,
        scenario: &Scenario,
        overrides: &IndexMap<String, ParamValue>,
        allow_fix_override: bool,
    ) -> Result<ResolvedSpec, RunError> {
        Ok(resolve_layers(
            &self.register,
            &self.calibration.values,
            scenario,
            overrides,
            ResolveOptions { allow_fix_override },
        )?)
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

/// Load the register (and calibration) and resolve `scenario` against it.
pub fn resolve_scenario(scenario: &Scenario, opts: &RunOptions) -> Result<ResolvedSpec, RunError> {
    Inputs::load(opts.params_dir.as_deref(), opts.calibration_dir.as_deref())?.resolve(
        scenario,
        &opts.overrides,
        opts.allow_fix_override,
    )
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
    let inputs = Inputs::load(opts.params_dir.as_deref(), opts.calibration_dir.as_deref())?;
    let spec = inputs.resolve(scenario, &opts.overrides, opts.allow_fix_override)?;
    run_spec(&spec, &inputs.calibration.stages, opts, out)
}

/// Run an already resolved spec and write its run folder to `out`.
pub fn run_spec(
    spec: &ResolvedSpec,
    calibration_stages: &[u8],
    opts: &RunOptions,
    out: &Path,
) -> Result<RunManifest, RunError> {
    let scenario = &spec.scenario;
    std::fs::create_dir_all(out)?;
    std::fs::write(out.join("spec.resolved.yaml"), spec.to_yaml()?)?;
    write_batch(&out.join("params.parquet"), &params_batch(spec)?)?;

    let run_hash = spec.run_hash(opts.seed, marg_core::VERSION);
    let start = Instant::now();
    let mut sim = Simulation::new(spec, opts.seed, opts.threads)?;
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
        rng_scheme: RNG_SCHEME.into(),
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
        experiment: opts.meta.clone(),
        calibration_stages: calibration_stages.to_vec(),
        error: None,
        live: None,
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
