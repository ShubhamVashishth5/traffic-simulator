//! Batch runs from an experiment's run list (plan P3.1, P3.5).
//!
//! An experiment folder holds `runs.jsonl`, one [`RunEntry`] per line, written
//! by the Python `marg.Experiment`. `marg batch` runs a slice of it:
//!
//! - **resumable:** a run whose folder already has an `ok` manifest with the
//!   same run hash is skipped, so resubmitting a half-finished sweep only runs
//!   what is missing;
//! - **atomic:** each run is written to a `*.partial-*` sibling folder and
//!   renamed into place when complete, so a killed job never leaves a folder
//!   that looks finished, and two workers racing on one run never produce two
//!   copies;
//! - **checked:** the run hash is recomputed and must equal the one in the run
//!   list. A mismatch means the register, calibration or simulator version
//!   changed since the experiment was expanded; the run fails with that reason.

use crate::manifest::{RunManifest, RunMeta, RunStatus};
use crate::run::{Inputs, RunError, RunOptions, run_spec};
use indexmap::IndexMap;
use marg_params::{ParamValue, Scenario};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// One line of `runs.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunEntry {
    /// Position in the run list (0-based, stable).
    pub run_id: u64,
    pub run_hash: String,
    pub experiment: String,
    /// Scenario name (Hive key `scenario`).
    pub scenario: String,
    /// Scenario YAML, relative to the experiment folder.
    pub scenario_file: String,
    /// Design point (Hive key `point`).
    pub point: u64,
    /// Arm name (Hive key `arm`).
    pub arm: String,
    /// Replication (Hive key `rep`).
    pub rep: u64,
    pub seed: u64,
    /// Experiment-layer overrides, in register units.
    #[serde(default)]
    pub overrides: IndexMap<String, ParamValue>,
    #[serde(default)]
    pub allow_fix_override: bool,
    /// Run folder, relative to the experiment folder.
    pub path: String,
}

impl RunEntry {
    fn meta(&self) -> RunMeta {
        RunMeta {
            experiment: self.experiment.clone(),
            run_id: self.run_id,
            point: self.point,
            arm: self.arm.clone(),
            rep: self.rep,
        }
    }
}

/// Errors reading a run list.
#[derive(Debug, thiserror::Error)]
pub enum BatchError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} line {line}: {source}")]
    Json {
        path: String,
        line: usize,
        source: serde_json::Error,
    },
    #[error(transparent)]
    Run(#[from] RunError),
}

/// Read `runs.jsonl`.
pub fn read_run_list(path: &Path) -> Result<Vec<RunEntry>, BatchError> {
    let text = std::fs::read_to_string(path).map_err(|source| BatchError::Io {
        path: path.display().to_string(),
        source,
    })?;
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(i, l)| {
            serde_json::from_str(l).map_err(|source| BatchError::Json {
                path: path.display().to_string(),
                line: i + 1,
                source,
            })
        })
        .collect()
}

/// Which entries of the run list to run.
#[derive(Debug, Clone, Default)]
pub enum Selection {
    #[default]
    All,
    /// `count` entries starting at position `start`.
    Range { start: usize, count: usize },
    /// Chunk `index` of chunks of `size` entries (a SLURM array task).
    Chunk { index: usize, size: usize },
    /// These run ids.
    Ids(Vec<u64>),
}

impl Selection {
    fn pick<'a>(&self, entries: &'a [RunEntry]) -> Vec<&'a RunEntry> {
        match self {
            Selection::All => entries.iter().collect(),
            Selection::Range { start, count } => entries.iter().skip(*start).take(*count).collect(),
            Selection::Chunk { index, size } => entries
                .iter()
                .skip(index.saturating_mul(*size))
                .take(*size)
                .collect(),
            Selection::Ids(ids) => {
                let set: std::collections::BTreeSet<u64> = ids.iter().copied().collect();
                entries.iter().filter(|e| set.contains(&e.run_id)).collect()
            }
        }
    }
}

/// How to run a batch.
#[derive(Debug, Clone, Default)]
pub struct BatchOptions {
    /// Register directory; default `<root>/params` if present, else the
    /// usual search ([`crate::find_params_dir`]).
    pub params_dir: Option<PathBuf>,
    /// Calibration directory; default `<root>/calibration` if present.
    pub calibration_dir: Option<PathBuf>,
    /// Run list; default `<root>/runs.jsonl`.
    pub run_list: Option<PathBuf>,
    pub selection: Selection,
    /// Worker threads per run (0 = all cores).
    pub threads: usize,
    /// Re-run even if an `ok` manifest exists.
    pub force: bool,
}

/// What a batch did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BatchSummary {
    pub selected: usize,
    pub ran: usize,
    pub skipped: usize,
    pub failed: usize,
    /// Runs finished by a concurrent worker first (our copy discarded).
    pub raced: usize,
}

/// Whether `dir` holds a finished run with this hash.
pub fn is_done(dir: &Path, run_hash: &str) -> bool {
    RunManifest::read(dir).is_ok_and(|m| m.status == RunStatus::Ok && m.run_hash == run_hash)
}

/// Run the selected entries of an experiment folder's run list, in order.
pub fn run_batch(root: &Path, opts: &BatchOptions) -> Result<BatchSummary, BatchError> {
    let list = opts
        .run_list
        .clone()
        .unwrap_or_else(|| root.join("runs.jsonl"));
    let entries = read_run_list(&list)?;
    let params_dir = opts
        .params_dir
        .clone()
        .or_else(|| Some(root.join("params")).filter(|p| p.is_dir()));
    let calibration_dir = opts
        .calibration_dir
        .clone()
        .or_else(|| Some(root.join("calibration")).filter(|p| p.is_dir()));
    let inputs = Inputs::load(params_dir.as_deref(), calibration_dir.as_deref())?;
    let mut scenarios: BTreeMap<String, Result<Scenario, String>> = BTreeMap::new();
    let selected = opts.selection.pick(&entries);
    let mut summary = BatchSummary {
        selected: selected.len(),
        ..Default::default()
    };
    for e in selected {
        let dir = root.join(&e.path);
        if !opts.force && is_done(&dir, &e.run_hash) {
            summary.skipped += 1;
            continue;
        }
        let scenario = scenarios
            .entry(e.scenario_file.clone())
            .or_insert_with(|| load_scenario(&root.join(&e.scenario_file)));
        match run_entry(e, scenario, &inputs, opts.threads, &dir) {
            Outcome::Ran => summary.ran += 1,
            Outcome::Failed => summary.failed += 1,
            Outcome::Raced => summary.raced += 1,
        }
    }
    Ok(summary)
}

fn load_scenario(path: &Path) -> Result<Scenario, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    Scenario::from_yaml(&text).map_err(|e| format!("{}: {e}", path.display()))
}

enum Outcome {
    Ran,
    Failed,
    Raced,
}

fn partial_dir(dir: &Path) -> PathBuf {
    let name = dir
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let host = RunManifest::build_info().4;
    dir.with_file_name(format!("{name}.partial-{host}-{}", std::process::id()))
}

fn run_entry(
    e: &RunEntry,
    scenario: &Result<Scenario, String>,
    inputs: &Inputs,
    threads: usize,
    dir: &Path,
) -> Outcome {
    let tmp = partial_dir(dir);
    let _ = std::fs::remove_dir_all(&tmp);
    let result = attempt(e, scenario, inputs, threads, &tmp);
    let failed = match result {
        Ok(()) => false,
        Err(msg) => {
            let _ = std::fs::remove_dir_all(&tmp);
            let m = RunManifest::failed(&e.run_hash, &e.scenario, e.seed, Some(e.meta()), msg);
            if m.write(&tmp).is_err() {
                return Outcome::Failed;
            }
            true
        }
    };
    if is_done(dir, &e.run_hash) {
        // Another worker finished the same run while we were running it.
        let _ = std::fs::remove_dir_all(&tmp);
        return Outcome::Raced;
    }
    let _ = std::fs::remove_dir_all(dir);
    if let Some(parent) = dir.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match std::fs::rename(&tmp, dir) {
        Ok(()) if failed => Outcome::Failed,
        Ok(()) => Outcome::Ran,
        Err(_) if is_done(dir, &e.run_hash) => {
            let _ = std::fs::remove_dir_all(&tmp);
            Outcome::Raced
        }
        Err(_) => Outcome::Failed,
    }
}

fn attempt(
    e: &RunEntry,
    scenario: &Result<Scenario, String>,
    inputs: &Inputs,
    threads: usize,
    tmp: &Path,
) -> Result<(), String> {
    let scenario = scenario.as_ref().map_err(Clone::clone)?;
    let spec = inputs
        .resolve(scenario, &e.overrides, e.allow_fix_override)
        .map_err(|err| err.to_string())?;
    let hash = spec.run_hash(e.seed, marg_core::VERSION);
    if hash != e.run_hash {
        return Err(format!(
            "run hash mismatch: run list says {}, this build resolves {hash}; the register, \
             calibration or marg version changed since the experiment was expanded",
            e.run_hash
        ));
    }
    let opts = RunOptions {
        seed: e.seed,
        threads,
        meta: Some(e.meta()),
        ..Default::default()
    };
    run_spec(&spec, &inputs.calibration.stages, &opts, tmp)
        .map(|_| ())
        .map_err(|err| err.to_string())
}
